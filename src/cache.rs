use hyper::{
    body::{Body, Bytes, Frame},
    Error,
};
use std::{
    marker::Unpin,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tokio::{runtime::Handle, sync::RwLock};

pub type CacheStore = Arc<RwLock<std::collections::BTreeMap<String, Bytes>>>;

pub struct FanoutBody<T: Body + Unpin> {
    pub body: T,
    pub uri: String,
    pub buffer: Vec<u8>,
    pub cachestore: CacheStore,
}

impl<T: Body + Unpin> FanoutBody<T> {
    fn done(mut self: Pin<&mut Self>) {
        let uri = self.uri.clone();
        let cachestore = Arc::clone(&self.cachestore);

        // we cannot take the buffer since self is pinned,
        // consume it instead
        let mut content = Vec::new();
        content.append(&mut self.buffer);

        // Body trait does not allow us to be an async function,
        // nab the runtime and become one anyways >:3
        let _ = Handle::current().enter();
        tokio::task::spawn(async move {
            #[cfg(feature = "log")]
            eprintln!("cached {} using {} B", uri, content.len());

            cachestore.write().await.insert(uri, content.into());
        });
    }
}

impl<T: Body<Data = Bytes, Error = Error> + Unpin> Body for FanoutBody<T> {
    type Data = Bytes;
    type Error = Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let res = Pin::new(&mut self.body).poll_frame(cx);
        match res {
            Poll::Ready(Some(Ok(ref frame))) => {
                if let Some(data) = frame.data_ref() {
                    self.buffer.append(&mut data.to_vec());
                }
            }
            Poll::Ready(None) => self.done(),
            _ => (),
        };

        res
    }
}

#[cfg(test)]
mod tests {
    use crate::cache::*;
    use http_body_util::{BodyExt, Full};
    use hyper::body::Body;

    #[tokio::test]
    async fn cache_static() {
        let inp =
            Full::new(Bytes::from_static(b"you wouldn't download a fox")).map_err(|e| match e {});
        let cachestore: CacheStore = Arc::new(RwLock::new(std::collections::BTreeMap::new()));
        let body = FanoutBody {
            body: inp,
            uri: "/test".to_string(),
            buffer: Vec::new(),
            cachestore: Arc::clone(&cachestore),
        };

        // FIXME: replace with std::task::Waker::noop once stable
        // https://github.com/rust-lang/rust/issues/98286
        let waker = futures::task::noop_waker_ref();
        let mut cx = std::task::Context::from_waker(&waker);
        let mut pinned = std::pin::pin!(body);

        let pf = pinned.as_mut().poll_frame(&mut cx);
        let read = match pf {
            Poll::Ready(Some(Ok(ref frame))) => frame.data_ref().unwrap(),
            e => panic!("failed to poll frame: {:?}", e),
        };
        assert_eq!(read, &Bytes::from_static(b"you wouldn't download a fox"));

        match pinned.as_mut().poll_frame(&mut cx) {
            Poll::Ready(None) => (),
            e => panic!("failed to poll frame: {:?}", e),
        };

        // wait for FanoutBody to finish caching in the background
        tokio::task::yield_now().await;

        let res = cachestore.read().await;
        let res = res.get("/test").unwrap();
        assert_eq!(res, &Bytes::from_static(b"you wouldn't download a fox"));
    }
}
