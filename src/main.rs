use clap::Parser;
use http_body_util::{combinators::BoxBody, BodyExt, Full};
use hyper::{body::Bytes, server::conn::http1, service::service_fn, Request, Response};
use hyper_util::rt::TokioIo;
use std::{error::Error, net::SocketAddr, sync::Arc};
use tokio::{net::TcpListener, sync::RwLock};

pub mod bloom;
pub mod cache;
pub mod hclient;
pub mod metrics;
pub mod ranges;

#[derive(Debug, Parser)]
struct Opt {
    #[arg(short, env = "BIND", default_value = "[::]:8060")]
    bindhost: SocketAddr,

    #[arg(short, default_value = "0")]
    skip: usize,

    /// urls to check for a package, in order of precedence
    #[arg(required = true)]
    mirrors: Vec<String>,
}

fn not_found() -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::http::Error> {
    Response::builder()
        .status(hyper::StatusCode::NOT_FOUND)
        .body(
            Full::new(Bytes::from_static(b"knot found\n"))
                .map_err(|e| match e {})
                .boxed(),
        )
}

async fn handle_conn(
    req: Request<impl hyper::body::Body + Send>,
    mirrors: Arc<Vec<String>>,
    filter: Arc<RwLock<[u8; 8192]>>,
    cachestore: Arc<cache::CacheStore>,
    metrics: Arc<metrics::Metrics>,
    skip: usize,
) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::http::Error> {
    let uri = req.uri().path();
    metrics.trace_request();

    if req.method() == hyper::Method::DELETE {
        metrics.trace_delete();
        return if cachestore.remove(uri).await.is_some() {
            Ok(Response::new(
                Full::new(Bytes::from_static(b"nom nom\n"))
                    .map_err(|e| match e {})
                    .boxed(),
            ))
        } else {
            not_found()
        };
    }

    if uri == "/_tcrelay/metrics" {
        return Ok(metrics.response());
    }

    let uri_bytes = uri.as_bytes();
    let seen = bloom::check(&*filter.read().await, uri_bytes);

    if seen {
        if let Some(data) = cachestore.get(uri).await {
            let res = Response::builder().header("Accept-Ranges", "bytes");
            metrics.trace_hit();

            if let Some(range) = req.headers().get("Range") {
                return ranges::ranged_response(res, data, range);
            }

            return res.body(Full::new(data).map_err(|e| match e {}).boxed());
        }
    }

    if let Some((data, mindex)) = hclient::try_get(&mirrors, uri).await {
        metrics.trace_miss();
        let obody = data.into_body();
        let body = if seen && mindex >= skip {
            metrics.trace_cache();
            let sbody = cache::FanoutBody {
                body: obody,
                uri: uri.to_string(),
                buffer: Vec::new(),
                cachestore,
            };
            sbody.boxed()
        } else {
            bloom::add(&mut *filter.write().await, uri_bytes);
            obody.boxed()
        };

        Ok(Response::new(body))
    } else {
        metrics.trace_404();
        not_found()
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let opt = Opt::parse();

    let listen = TcpListener::bind(opt.bindhost).await?;

    eprintln!("listening on {}", listen.local_addr()?);

    let mirrors = Arc::new(opt.mirrors);
    let filter = Arc::new(RwLock::new([0_u8; 8192]));
    let cachestore = cache::CacheStore::new();
    let metrics = metrics::Metrics::new();

    loop {
        let (stream, _) = listen.accept().await?;
        let io = TokioIo::new(stream);

        let mirrors = Arc::clone(&mirrors);
        let filter = Arc::clone(&filter);
        let cachestore = Arc::clone(&cachestore);
        let metrics = Arc::clone(&metrics);

        let service = service_fn(move |req| {
            handle_conn(
                req,
                Arc::clone(&mirrors),
                Arc::clone(&filter),
                Arc::clone(&cachestore),
                Arc::clone(&metrics),
                opt.skip,
            )
        });

        tokio::task::spawn(async move {
            if let Err(e) = http1::Builder::new().serve_connection(io, service).await {
                eprintln!("oh no {:?}", e);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use crate::*;

    #[test]
    fn verify_clap() {
        use clap::CommandFactory;
        Opt::command().debug_assert();
    }

    #[tokio::test]
    async fn no_mirrors() {
        use http_body_util::Empty;
        use hyper::body::Body;

        let mirrors = Arc::new(vec![]);
        let filter = Arc::new(RwLock::new([0_u8; 8192]));
        let cachestore = cache::CacheStore::new();
        let metrics = metrics::Metrics::new();

        let req = Request::builder()
            .uri("/meow")
            .body(Empty::<Bytes>::new())
            .unwrap();

        let res = handle_conn(req, mirrors, filter, cachestore, metrics, 0)
            .await
            .unwrap();

        assert_eq!(res.body().size_hint().exact(), Some(11));

        // FIXME: comparing Debug is probably a bad idea,
        // but BoxBody does not implement Eq, so...
        let res = format!("{:?}", res);
        assert_eq!(res, format!("{:?}", not_found().unwrap()));
    }
}
