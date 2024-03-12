use http_body_util::Empty;
use hyper::{body::Bytes, Request, Response};
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;

mod tls_configs;

pub async fn try_get(mirrors: &[String], path: &str) -> Option<Response<hyper::body::Incoming>> {
    for m in mirrors {
        let url = format!("{}{}", m, path);
        let uri = match url.parse() {
            Ok(u) => u,
            Err(_e) => {
                #[cfg(feature = "log")]
                eprintln!("failed to parse {}: {:?}", url, _e);
                continue;
            }
        };
        match get_request(uri).await {
            Ok(r) => {
                if !r.status().is_success() {
                    #[cfg(feature = "log")]
                    eprintln!("{} from {}", r.status().as_str(), url);
                    continue;
                }

                #[cfg(feature = "log")]
                eprintln!("got {}", url);
                return Some(r);
            }
            Err(_e) => {
                #[cfg(feature = "log")]
                eprintln!("failed to get {}: {:?}", url, _e);
                continue;
            }
        };
    }
    None
}

pub async fn get_request(
    uri: hyper::Uri,
) -> Result<Response<hyper::body::Incoming>, Box<dyn std::error::Error + Send + Sync>> {
    let h = uri.host().ok_or("mangled host")?;
    let p = uri.port_u16().unwrap_or(80);
    let addr = format!("{}:{}", h, p);

    let stream = TcpStream::connect(addr).await?;
    let io = TokioIo::new(stream);
    let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await?;

    tokio::task::spawn(async move {
        if let Err(_e) = conn.await {
            #[cfg(feature = "log")]
            eprintln!("connection failed: {:?}", _e);
        }
    });

    let addr = uri
        .authority()
        .expect("got host but not authority? what")
        .as_str();
    let req = Request::builder()
        .uri(uri.path())
        .header(hyper::header::HOST, addr)
        .body(Empty::<Bytes>::new())?;

    Ok(sender.send_request(req).await?)
}

#[cfg(test)]
mod tests {
    use crate::hclient::*;

    #[tokio::test]
    #[ignore]
    async fn get() {
        let url = "http://tinycorelinux.net/10.x/x86/tcz/mirrors.tcz.md5.txt";
        let res = get_request(url.parse().unwrap()).await.unwrap();
        assert!(res.status().is_success());
    }

    #[tokio::test]
    #[ignore]
    async fn get_fallback() {
        let mirrors = [
            "http://deb.debian.org",    // should 404
            "http://github.com:443",    // should get nonsense
            "http://owo: whats this!",  // should be unparseable
            "http://tinycorelinux.net", // should actually get
        ]
        .map(|m| m.to_string());
        let res = try_get(&mirrors, "/10.x/x86/tcz/sed.tcz.md5.txt")
            .await
            .unwrap();
        assert_eq!(res.headers().get("content-length").unwrap(), "42");
    }
}
