use std::net::SocketAddr;

use tokio_rustls::rustls;

mod dns;
mod happy_eyeballs;

use dns::resolve_dns;
use happy_eyeballs::{interleave_addresses, launch_connection_attempts};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Debug, Clone, Copy)]
pub enum Transport {
    TcpTls,
    Quic,
}

#[derive(Debug)]
pub struct Metrics {
    pub addr: SocketAddr,
    pub family: &'static str,
    pub transport: Transport,
    pub dns_ms: Option<u32>,
    pub tcp_ms: Option<u32>,
    pub tls_ms: Option<u32>,
    pub http_ms: Option<u32>,
    pub total_ms: u32,
    pub success: bool,
    pub status_line: Option<String>,
    pub error: Option<String>,
}

impl Default for Metrics {
    fn default() -> Self {
        Self {
            addr: "0.0.0.0:0".parse().unwrap(),
            family: "IPv4",
            transport: Transport::TcpTls,
            dns_ms: None,
            tcp_ms: None,
            tls_ms: None,
            http_ms: None,
            total_ms: 0,
            success: false,
            status_line: None,
            error: None,
        }
    }
}

/// Print connection summary
pub fn print_summary(results: &[Metrics]) {
    println!("\nConnection Summary:");
    for result in results {
        let dns_str = result.dns_ms.map_or("N/A".to_string(), |ms| format!("{}ms", ms));
        let tcp_str = result.tcp_ms.map_or("N/A".to_string(), |ms| format!("{}ms", ms));
        let tls_str = result.tls_ms.map_or("N/A".to_string(), |ms| format!("{}ms", ms));
        let http_str = result.http_ms.map_or("N/A".to_string(), |ms| format!("{}ms", ms));
        
        println!(
            "{:?} {} {} dns={}ms tcp={}ms tls={}ms http={}ms total={}ms success={} status={:?} err={:?}",
            result.transport,
            result.family,
            result.addr,
            dns_str,
            tcp_str,
            tls_str,
            http_str,
            result.total_ms,
            result.success,
            result.status_line,
            result.error
        );
    }
}

/// Initialize the cryptographic provider for rustls
fn init_crypto_provider() -> Result<()> {
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .map_err(|_| "Failed to install default crypto provider")?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    init_crypto_provider()?;

    // CONFIG - Test targets
    let test_targets = vec![
        ("example.com", 443, "/"),
        ("github.com", 443, "/"),
        ("alethalabs.com", 443, "/"),
        ("crypto.com", 443, "/"),
    ];

    for (host, port, path) in test_targets {
        println!("\nTESTING TARGET: {}", host);
        println!("-------------------------------------");
        
        // DNS resolution 
        let (addrs, dns_ms) = resolve_dns(host, port).await?;

        // Interleave addresses
        let candidates = interleave_addresses(&addrs);

        // Launch connection attempts
        let results = launch_connection_attempts(candidates, host.to_string(), path.to_string(), dns_ms).await?;

        // Print connection summary
        print_summary(&results);
    }

    resolve_dns("example.com", 443).await?;
    Ok(())
}