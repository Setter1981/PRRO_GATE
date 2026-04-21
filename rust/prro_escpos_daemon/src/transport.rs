//! Transport drivers: turn an ESC/POS byte stream into actual I/O on
//! a physical printer.
//!
//! Step 2 ships TCP-only (most common for LAN receipt printers on
//! port 9100, RAW mode).  Serial (`serialport` crate) and USB (`rusb`)
//! are future steps behind feature flags.

use std::net::SocketAddr;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("tcp connect to {addr}: {source}")]
    TcpConnect {
        addr: SocketAddr,
        #[source]
        source: std::io::Error,
    },

    #[error("tcp write: {0}")]
    TcpWrite(#[source] std::io::Error),

    #[error("timeout after {elapsed_ms} ms")]
    Timeout { elapsed_ms: u128 },

    #[error("invalid address {addr}: {source}")]
    InvalidAddress {
        addr: String,
        #[source]
        source: std::io::Error,
    },
}

pub async fn send_tcp(
    host: &str,
    port: u16,
    bytes: &[u8],
    timeout_ms: u64,
) -> Result<(), TransportError> {
    let addr_str = format!("{host}:{port}");
    // Parse the first resolved address.  We do not want
    // happy-eyeballs / DNS loops here — keep it simple.
    let mut addrs = tokio::net::lookup_host(&addr_str)
        .await
        .map_err(|e| TransportError::InvalidAddress { addr: addr_str.clone(), source: e })?;
    let addr = addrs
        .next()
        .ok_or_else(|| TransportError::InvalidAddress {
            addr: addr_str.clone(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "no addrs"),
        })?;

    let fut = async {
        let mut stream = TcpStream::connect(addr)
            .await
            .map_err(|e| TransportError::TcpConnect { addr, source: e })?;
        stream.write_all(bytes).await.map_err(TransportError::TcpWrite)?;
        stream.shutdown().await.map_err(TransportError::TcpWrite)?;
        Ok::<(), TransportError>(())
    };
    match tokio::time::timeout(Duration::from_millis(timeout_ms), fut).await {
        Ok(inner) => inner,
        Err(_) => Err(TransportError::Timeout { elapsed_ms: timeout_ms as u128 }),
    }
}
