use lazy_static::lazy_static;
use std::sync::Arc;
use tokio_rustls::rustls::{
    self,
    client::danger::HandshakeSignatureValid,
    crypto::{verify_tls12_signature, verify_tls13_signature, CryptoProvider},
    pki_types::{CertificateDer, ServerName, UnixTime},
    ClientConfig, DigitallySignedStruct,
};

lazy_static! {
    pub static ref CONF: Arc<ClientConfig> = Arc::new(base_config(false));
    pub static ref CONF_INSECURE: Arc<ClientConfig> = Arc::new(base_config(true));
}

fn base_config(insecure: bool) -> ClientConfig {
    let config = rustls::ClientConfig::builder();
    let config = if insecure {
        config
            .dangerous()
            .with_custom_certificate_verifier(PhonyVerify::new(
                rustls::crypto::ring::default_provider(),
            ))
    } else {
        let mut store = rustls::RootCertStore::empty();
        store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        config.with_root_certificates(store)
    };
    config.with_no_client_auth()
}

// mostly borrowed from rustls/examples/src/bin/tlsclient-mio.rs,

#[derive(Debug)]
pub struct PhonyVerify(CryptoProvider);

impl PhonyVerify {
    pub fn new(provider: CryptoProvider) -> Arc<Self> {
        Arc::new(Self(provider))
    }
}

impl rustls::client::danger::ServerCertVerifier for PhonyVerify {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}
