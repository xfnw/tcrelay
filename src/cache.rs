use hyper::{
    body::{Body, Bytes, Frame, SizeHint},
    Error,
};
use parking_lot::RwLock;
use std::{
    marker::Unpin,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

pub struct CacheStore {
    store: RwLock<std::collections::BTreeMap<String, Bytes>>,
}

impl CacheStore {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            store: RwLock::new(std::collections::BTreeMap::new()),
        })
    }

    pub fn get(&self, uri: &str) -> Option<Bytes> {
        self.store.read().get(uri).cloned()
    }

    pub fn insert(&self, uri: String, content: Bytes) {
        #[cfg(feature = "log")]
        eprintln!("cached {} using {} B", uri, content.len());

        self.store.write().insert(uri, content);
    }

    pub fn remove(&self, uri: &str) -> Option<Bytes> {
        let removed = self.store.write().remove(uri);
        if let Some(content) = removed {
            #[cfg(feature = "log")]
            eprintln!("removed {} freeing {} B", uri, content.len());

            return Some(content);
        }
        None
    }
}

pub struct FanoutBody<T: Body + Unpin> {
    pub body: T,
    pub uri: String,
    pub buffer: Vec<u8>,
    pub cachestore: Arc<CacheStore>,
}

impl<T: Body + Unpin> FanoutBody<T> {
    fn done(mut self: Pin<&mut Self>) {
        if self.buffer.is_empty() {
            return;
        }

        let uri = self.uri.clone();
        let cachestore = Arc::clone(&self.cachestore);

        // we cannot take the buffer since self is pinned,
        // consume it instead
        let content = std::mem::take(&mut self.buffer);

        cachestore.insert(uri, content.into());
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
                if self.is_end_stream() {
                    self.done();
                }
            }
            Poll::Ready(None) => self.done(),
            _ => (),
        };

        res
    }

    #[inline]
    fn is_end_stream(&self) -> bool {
        self.body.is_end_stream()
    }

    #[inline]
    fn size_hint(&self) -> SizeHint {
        self.body.size_hint()
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
        let cachestore = CacheStore::new();
        let body = FanoutBody {
            body: inp,
            uri: "/test".to_string(),
            buffer: Vec::new(),
            cachestore: Arc::clone(&cachestore),
        };

        // FIXME: replace with std::task::Waker::noop once stable
        // https://github.com/rust-lang/rust/issues/98286
        let waker = futures::task::noop_waker_ref();
        let mut cx = std::task::Context::from_waker(waker);
        let mut pinned = std::pin::pin!(body);

        assert_eq!(pinned.size_hint().exact(), Some(27));

        let pf = pinned.as_mut().poll_frame(&mut cx);
        let read = match pf {
            Poll::Ready(Some(Ok(ref frame))) => frame.data_ref().unwrap(),
            e => panic!("failed to poll frame: {e:?}"),
        };
        assert_eq!(read, &Bytes::from_static(b"you wouldn't download a fox"));

        assert_eq!(pinned.size_hint().exact(), Some(0));

        // wait for FanoutBody to finish caching in the background
        tokio::task::yield_now().await;
        let res = cachestore.get("/test").unwrap();
        assert_eq!(res, Bytes::from_static(b"you wouldn't download a fox"));

        match pinned.as_mut().poll_frame(&mut cx) {
            Poll::Ready(None) => (),
            e => panic!("failed to poll frame: {e:?}"),
        };

        // make sure extra polling does not mess up the cache
        tokio::task::yield_now().await;
        let res = cachestore.get("/test").unwrap();
        assert_eq!(res, Bytes::from_static(b"you wouldn't download a fox"));

        let res = cachestore.remove("/test").unwrap();
        assert_eq!(res, Bytes::from_static(b"you wouldn't download a fox"));
    }
}
