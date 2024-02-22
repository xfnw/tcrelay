use hyper::body::Body;
use hyper::body::Frame;
use std::{
    pin::Pin,
    task::{Context, Poll},
};

pub struct FanoutBody {
    pub body: hyper::body::Incoming,
    pub uri: String,
}

impl Body for FanoutBody {
    type Data = hyper::body::Bytes;
    type Error = hyper::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let res = Pin::new(&mut self.body).poll_frame(cx);
        dbg!(&res);
        res
    }
}

#[cfg(test)]
mod tests {
    use crate::cache::*;
}
