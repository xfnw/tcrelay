use http_body_util::{combinators::BoxBody, BodyExt, Full};
use hyper::{body::Bytes, Response};
use std::sync::{
    atomic::{AtomicUsize, Ordering::Relaxed},
    Arc,
};

#[derive(Default)]
pub struct Metrics {
    requests: AtomicUsize,
    hits: AtomicUsize,
    misses: AtomicUsize,
    cached: AtomicUsize,
    deletes: AtomicUsize,
    not_found: AtomicUsize,
}

macro_rules! trace_functions {
    (($name:ident, $field:ident)) => {
        pub fn $name(&self) {
            self.$field.fetch_add(1, Relaxed);
        }
    };
    (($name:ident, $field:ident), $($tail:tt)*) => {
        trace_functions!(($name, $field));
        trace_functions!($($tail)*);
    };
}

impl Metrics {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn output(&self) -> String {
        format!(
            r"requests {}
hits {}
misses {}
cached {}
deletes {}
not_found {}
",
            self.requests.load(Relaxed),
            self.hits.load(Relaxed),
            self.misses.load(Relaxed),
            self.cached.load(Relaxed),
            self.deletes.load(Relaxed),
            self.not_found.load(Relaxed)
        )
    }

    pub fn response(&self) -> Response<BoxBody<Bytes, hyper::Error>> {
        Response::new(
            Full::new(Bytes::from(self.output()))
                .map_err(|e| match e {})
                .boxed(),
        )
    }

    trace_functions!(
        (trace_request, requests),
        (trace_hit, hits),
        (trace_miss, misses),
        (trace_cache, cached),
        (trace_delete, deletes),
        (trace_404, not_found)
    );
}

#[cfg(test)]
mod tests {
    use crate::metrics::Metrics;

    #[test]
    fn incremented() {
        let m = Metrics::new();

        for _ in 0..102 {
            m.trace_request()
        }
        for _ in 0..111 {
            m.trace_hit()
        }
        for _ in 0..120 {
            m.trace_miss()
        }
        for _ in 0..105 {
            m.trace_cache()
        }
        for _ in 0..101 {
            m.trace_delete()
        }
        for _ in 0..115 {
            m.trace_404()
        }

        assert_eq!(
            m.output(),
            r"requests 102
hits 111
misses 120
cached 105
deletes 101
not_found 115
"
        );
    }
}
