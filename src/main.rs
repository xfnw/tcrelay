use clap::Parser;
use http_body_util::{combinators::BoxBody, BodyExt, Full};
use hyper::{body::Bytes, server::conn::http1, service::service_fn, Request, Response};
use hyper_util::rt::TokioIo;
use std::{error::Error, net::SocketAddr, sync::Arc};
use tokio::{net::TcpListener, sync::RwLock};

pub mod bloom;
pub mod cache;
pub mod hclient;

#[derive(Debug, Parser)]
struct Opt {
    #[arg(short, env = "BIND", default_value = "[::]:8060")]
    bindhost: SocketAddr,

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
    req: Request<hyper::body::Incoming>,
    mirrors: Arc<Vec<String>>,
    filter: Arc<RwLock<[u8; 8192]>>,
    cachestore: cache::CacheStore,
) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::http::Error> {
    let uri = req.uri().path();
    let uri_bytes = uri.as_bytes();
    let seen = bloom::check(&*filter.read().await, uri.as_bytes());

    if seen {
        if let Some(data) = cachestore.read().await.get(uri) {
            return Ok(Response::new(
                Full::new(Bytes::clone(data))
                    .map_err(|e| match e {})
                    .boxed(),
            ));
        }
    }

    match hclient::try_get(&mirrors, uri).await {
        Some(data) => {
            let obody = data.into_body();
            let body = match seen {
                true => {
                    let sbody = cache::FanoutBody {
                        body: obody,
                        uri: uri.to_string(),
                        buffer: Vec::new(),
                        cachestore,
                    };
                    sbody.boxed()
                }
                false => {
                    bloom::add(&mut *filter.write().await, uri_bytes);
                    obody.boxed()
                }
            };

            Ok(Response::new(body))
        }
        None => not_found(),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let opt = Opt::parse();

    let listen = TcpListener::bind(opt.bindhost).await?;

    eprintln!("listening on {}", listen.local_addr()?);

    let mirrors = Arc::new(opt.mirrors);
    let filter = Arc::new(RwLock::new([0_u8; 8192]));
    let cachestore = cache::new_store();

    loop {
        let (stream, _) = listen.accept().await?;
        let io = TokioIo::new(stream);

        let mirrors = Arc::clone(&mirrors);
        let filter = Arc::clone(&filter);
        let cachestore = Arc::clone(&cachestore);

        let service = service_fn(move |req| {
            handle_conn(
                req,
                Arc::clone(&mirrors),
                Arc::clone(&filter),
                Arc::clone(&cachestore),
            )
        });

        tokio::task::spawn(async move {
            if let Err(e) = http1::Builder::new().serve_connection(io, service).await {
                eprintln!("oh no {:?}", e);
            }
        });
    }
}
