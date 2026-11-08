//! Production-Grade Pure-Rust TLS Transport for HTTP/1.1 and HTTPS.
//!
//! Implements memory-safe TLS 1.2 and TLS 1.3 encrypted transport streams using `rustls`
//! and Mozilla's `webpki-roots`, with zero C/OpenSSL dependencies.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, OnceLock};

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};

/// Abstract transport stream multiplexing between plain TCP and TLS-encrypted connections.
pub enum HttpTransportStream {
    Plain(TcpStream),
    Tls(Box<StreamOwned<ClientConnection, TcpStream>>),
}

impl Read for HttpTransportStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Plain(stream) => stream.read(buf),
            Self::Tls(stream) => stream.read(buf),
        }
    }
}

impl Write for HttpTransportStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Self::Plain(stream) => stream.write(buf),
            Self::Tls(stream) => stream.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::Plain(stream) => stream.flush(),
            Self::Tls(stream) => stream.flush(),
        }
    }
}

static TLS_CLIENT_CONFIG: OnceLock<Arc<ClientConfig>> = OnceLock::new();

/// Returns a shared, lazily-initialized TLS `ClientConfig` with Mozilla WebPKI root certificates.
pub fn get_or_init_tls_config() -> Arc<ClientConfig> {
    TLS_CLIENT_CONFIG
        .get_or_init(|| {
            let mut root_store = RootCertStore::empty();
            root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

            let mut config = ClientConfig::builder_with_provider(Arc::new(
                rustls::crypto::ring::default_provider(),
            ))
            .with_safe_default_protocol_versions()
            .expect("Valid default protocol versions")
            .with_root_certificates(root_store)
            .with_no_client_auth();

            config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

            Arc::new(config)
        })
        .clone()
}

impl HttpTransportStream {
    /// Returns the negotiated ALPN protocol, if TLS was used and negotiated.
    pub fn alpn_protocol(&self) -> Option<&[u8]> {
        match self {
            Self::Plain(_) => None,
            Self::Tls(stream) => stream.conn.alpn_protocol(),
        }
    }
}

/// Creates an encrypted TLS client transport stream wrapping an established `TcpStream`.
pub fn create_tls_stream(
    mut tcp_stream: TcpStream,
    host: &str,
) -> Result<HttpTransportStream, String> {
    let server_name = ServerName::try_from(host.to_string())
        .map_err(|e| format!("Invalid DNS hostname for TLS SNI '{host}': {e}"))?;

    let config = get_or_init_tls_config();
    let mut conn = ClientConnection::new(config, server_name)
        .map_err(|e| format!("TLS client handshake initialization failed: {e}"))?;

    while conn.is_handshaking() {
        conn.complete_io(&mut tcp_stream)
            .map_err(|e| format!("TLS handshake failed: {e}"))?;
    }

    Ok(HttpTransportStream::Tls(Box::new(StreamOwned::new(conn, tcp_stream))))
}
