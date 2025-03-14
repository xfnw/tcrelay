use http_body_util::Empty;
use hyper::{body::Bytes, Request, Response, Uri};
use hyper_util::rt::TokioIo;
use std::sync::Arc;
use tokio::{io, net::TcpStream};
use tokio_rustls::{rustls::pki_types, TlsConnector};

mod tls_configs;

pub enum Scheme {
    Https,
    HttpsInsecure,
    Http,
}

pub async fn try_get(
    mirrors: &[String],
    path: &str,
) -> Option<(Response<hyper::body::Incoming>, usize)> {
    for (i, m) in mirrors.iter().enumerate() {
        let url = format!("{m}{path}");
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
                return Some((r, i));
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
    uri: Uri,
) -> Result<Response<hyper::body::Incoming>, Box<dyn std::error::Error + Send + Sync>> {
    let scheme = match uri.scheme_str() {
        Some("https") => Scheme::Https,
        Some("https+insecure") => Scheme::HttpsInsecure,
        Some("http") | None => Scheme::Http,
        Some(_) => return Err("unsupported scheme".into()),
    };

    let h = uri.host().ok_or("mangled host")?;
    let p = uri.port_u16().unwrap_or(match scheme {
        Scheme::Https | Scheme::HttpsInsecure => 443,
        Scheme::Http => 80,
    });
    let addr = format!("{h}:{p}");

    let stream = TcpStream::connect(addr).await?;

    match scheme {
        Scheme::Https => {
            let connector = TlsConnector::from(Arc::clone(&*tls_configs::CONF));
            let domain = pki_types::ServerName::try_from(h)?.to_owned();
            let stream = connector.connect(domain, stream).await?;

            get_with_stream(stream, uri).await
        }
        Scheme::HttpsInsecure => {
            let connector = TlsConnector::from(Arc::clone(&*tls_configs::CONF_INSECURE));
            let domain = pki_types::ServerName::try_from(h)?.to_owned();
            let stream = connector.connect(domain, stream).await?;

            get_with_stream(stream, uri).await
        }
        Scheme::Http => get_with_stream(stream, uri).await,
    }
}

async fn get_with_stream<T: io::AsyncRead + io::AsyncWrite + Send + Unpin + 'static>(
    stream: T,
    uri: Uri,
) -> Result<Response<hyper::body::Incoming>, Box<dyn std::error::Error + Send + Sync>> {
    let io = TokioIo::new(stream);
    let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await?;

    tokio::task::spawn(async move {
        if let Err(_e) = conn.await {
            #[cfg(feature = "log")]
            eprintln!("connection failed: {:?}", _e);
        }
    });

    let addr = uri.authority().expect("failed to parse authority").as_str();

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
    async fn get_https() {
        let url = "https://mozilla-modern.badssl.com/";
        let res = get_request(url.parse().unwrap()).await.unwrap();
        assert!(res.status().is_success());

        let url = "https+insecure://self-signed.badssl.com/";
        let res = get_request(url.parse().unwrap()).await.unwrap();
        assert!(res.status().is_success());

        let url = "https://mitm-software.badssl.com:443/";
        // would be better to check the error kind specifically for
        // InvalidCertificate(UnknownIssuer), but it is boxed :(
        get_request(url.parse().unwrap()).await.unwrap_err();
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
        assert_eq!(res.0.headers().get("content-length").unwrap(), "42");
    }
}
