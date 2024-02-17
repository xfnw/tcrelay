use clap::Parser;
use http_body_util::Full;
use hyper::{body::Bytes, server::conn::http1, service::service_fn, Request, Response};
use hyper_util::rt::TokioIo;
use std::{error::Error, net::SocketAddr};
use tokio::net::TcpListener;

pub mod bloom;

#[derive(Debug, Parser)]
struct Opt {
    #[arg(short, env = "BIND", default_value = "[::]:8060")]
    bindhost: SocketAddr,
}

async fn handle_conn(
    _: Request<hyper::body::Incoming>,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    Ok(Response::new(Full::new(Bytes::from("meow"))))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let opt = Opt::parse();

    let listen = TcpListener::bind(opt.bindhost).await?;

    eprintln!("listening on {}", opt.bindhost);

    loop {
        let (stream, _) = listen.accept().await?;
        let io = TokioIo::new(stream);

        tokio::task::spawn(async move {
            if let Err(e) = http1::Builder::new()
                .serve_connection(io, service_fn(handle_conn))
                .await
            {
                eprintln!("oh no {:?}", e);
            }
        });
    }
}
