use hyper::body::{Body, Bytes, Frame};
use std::{
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tokio::{runtime::Handle, sync::RwLock};

pub type CacheStore = Arc<RwLock<std::collections::BTreeMap<String, Bytes>>>;

pub struct FanoutBody {
    pub body: hyper::body::Incoming,
    pub uri: String,
    pub buffer: Vec<u8>,
    pub cachestore: CacheStore,
}

impl FanoutBody {
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

impl Body for FanoutBody {
    type Data = Bytes;
    type Error = hyper::Error;

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
mod tests {}
