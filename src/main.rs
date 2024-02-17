use clap::Parser;
use http_body_util::Full;
use hyper::{body::Bytes, server::conn::http1, service::service_fn, Request, Response};
use hyper_util::rt::TokioIo;
use std::{
    error::Error,
    net::SocketAddr,
    sync::{Arc, RwLock},
};
use tokio::net::TcpListener;

pub mod bloom;

#[derive(Debug, Parser)]
struct Opt {
    #[arg(short, env = "BIND", default_value = "[::]:8060")]
    bindhost: SocketAddr,

    /// urls to check for a package, in order of precedence
    #[arg(required = true)]
    mirrors: Vec<String>,
}

async fn handle_conn(
    _: Request<hyper::body::Incoming>,
    mirrors: Arc<Vec<String>>,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    Ok(Response::new(Full::new(Bytes::from(mirrors[0].clone()))))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let opt = Opt::parse();

    let listen = TcpListener::bind(opt.bindhost).await?;

    eprintln!("listening on {}", opt.bindhost);

    let mirrors = Arc::new(opt.mirrors);

    loop {
        let (stream, _) = listen.accept().await?;
        let io = TokioIo::new(stream);

        let mirrors = Arc::clone(&mirrors);

        let service = service_fn(move |req| handle_conn(req, Arc::clone(&mirrors)));

        tokio::task::spawn(async move {
            if let Err(e) = http1::Builder::new().serve_connection(io, service).await {
                eprintln!("oh no {:?}", e);
            }
        });
    }
}
