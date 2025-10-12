use std::{
    net::{IpAddr, Ipv6Addr, SocketAddr},
    sync::Arc,
    time::{Duration, Instant},
};

use rustls_pki_types::ServerName;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::mpsc,
    time::sleep,
};
use tokio_rustls::{TlsConnector, rustls::{ClientConfig, RootCertStore}};

use crate::{Metrics, Transport};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Destination Address Selection - sort addresses by precedence
fn sort_addr(addrs: &mut [SocketAddr]) {
    addrs.sort_by(|a, b| {
        let precedence_a = addr_precedence(&a.ip());
        let precedence_b = addr_precedence(&b.ip());
        
        precedence_b.cmp(&precedence_a) // Higher precedence first
    });
}

/// Get precedence for an IP address
fn addr_precedence(ip: &IpAddr) -> u8 {
    match ip {
        IpAddr::V4(_) => 35,  // IPv4
        IpAddr::V6(v6) => {
            if v6.is_loopback() {
                50  // ::1/128 (loopback)
            } else if is_ipv4_mapped(v6) {
                35  // ::ffff:0:0/96 (IPv4-mapped)
            } else if is_6to4(v6) {
                30  // 2002::/16 (6to4)
            } else if is_teredo(v6) {
                5   // 2001::/32 (Teredo)
            } else {
                40  // ::/0 (native IPv6)
            }
        }
    }
}

/// Check if IPv6 address is IPv4-mapped
fn is_ipv4_mapped(addr: &Ipv6Addr) -> bool {
    addr.to_ipv4_mapped().is_some()
}

/// Check if IPv6 address is 6to4
fn is_6to4(addr: &Ipv6Addr) -> bool {
    addr.segments()[0] == 0x2002
}

/// Check if IPv6 address is Teredo
fn is_teredo(addr: &Ipv6Addr) -> bool {
    addr.segments()[0] == 0x2001 && addr.segments()[1] == 0x0000
}

/// Interleave IPv6 and IPv4 addresses according to RFC 8305 (Happy Eyeballs)
pub fn interleave_addresses(addrs: &[SocketAddr]) -> Vec<SocketAddr> {
    // Step 1: Apply RFC 6724 Destination Address Selection
    let mut sorted_addrs = addrs.to_vec();
    sort_addr(&mut sorted_addrs);
    
    // Step 2: Separate IPv6 and IPv4 addresses (maintaining RFC 6724 order within each family)
    let (v6, v4): (Vec<SocketAddr>, Vec<SocketAddr>) = sorted_addrs.into_iter().partition(|a| a.is_ipv6());
    
    // Step 3: Interleave according to Happy Eyeballs (IPv6 first)
    let mut candidates = Vec::new();
    let mut i = 0usize;
    
    while i < v6.len() || i < v4.len() {
        if i < v6.len() {
            candidates.push(v6[i]);
        }
        if i < v4.len() {
            candidates.push(v4[i]);
        }
        i += 1;
    }
    
    candidates
}

/// Launch connection attempts with Happy Eyeballs timing
pub async fn launch_connection_attempts(
    candidates: Vec<SocketAddr>,
    host: String,
    path: String,
    dns_ms: u32,
) -> Result<Vec<Metrics>> {
    // Create unbounded MPSC channel (Multiple Producer, Single Consumer)
    let (transmitter, mut receiver) = mpsc::unbounded_channel();
    
    // Launch first connection immediately
    if let Some(&first_addr) = candidates.first() { 
        let tx_clone = transmitter.clone();
        let host_clone = host.clone();
        let path_clone = path.clone();
        launch_connection(first_addr, host_clone, path_clone, Transport::TcpTls, dns_ms, tx_clone);
    }
    
    // Launch additional connections with 250ms delays
    for (i, &addr) in candidates.iter().enumerate() {
        if i == 0 { continue; } // Skip first (already launched)
        
        let delay = Duration::from_millis(250 * i as u64);
        let tx_clone = transmitter.clone();
        let host_clone = host.clone();
        let path_clone = path.clone();
        
        tokio::spawn(async move {
            sleep(delay).await;
            println!("Launching TLS to {} after {}ms delay", addr, delay.as_millis());
            launch_connection(addr, host_clone, path_clone, Transport::TcpTls, dns_ms, tx_clone);
        });
    }
    
    // Collect results
    drop(transmitter); // Close transmitter to signal completion
    
    let mut results = Vec::new();
    while let Some(result) = receiver.recv().await {
        println!("result: {:?}", result);
        let success = result.success;
        results.push(result);
        
        // Stop after first success for demo purposes
        if success {
            break;
        }
    }
    
    Ok(results)
}

/// Launch a single connection attempt
fn launch_connection(
    addr: SocketAddr,
    host: String,
    path: String,
    transport: Transport,
    dns_ms: u32,
    transmitter: mpsc::UnboundedSender<Metrics>,
) {
    tokio::spawn(async move {
        let mut metrics = Metrics {
            addr,
            family: if addr.is_ipv6() { "IPv6" } else { "IPv4" },
            transport,
            dns_ms: Some(dns_ms),
            ..Default::default()
        };
        
        let total_start = Instant::now();
        
        match transport {
            Transport::TcpTls => {
                match attempt_tcp_tls(&addr, &host, &path, &mut metrics).await {
                    Ok(_) => {
                        metrics.success = true;
                        metrics.total_ms = total_start.elapsed().as_millis() as u32;
                    }
                    Err(e) => {
                        metrics.error = Some(e.to_string());
                        metrics.total_ms = total_start.elapsed().as_millis() as u32;
                    }
                }
            }
            Transport::Quic => {
                // QUIC support is disabled for now
                metrics.error = Some("QUIC support temporarily disabled".to_string());
            }
        }
        
        let _ = transmitter.send(metrics);
    });
}

/// Attempt TCP+TLS connection
async fn attempt_tcp_tls(
    addr: &SocketAddr,
    host: &str,
    path: &str,
    metrics: &mut Metrics,
) -> Result<()> {
    // TCP connection
    let tcp_start = Instant::now();
    let tcp_stream = TcpStream::connect(addr).await?;
    let tcp_ms = tcp_start.elapsed().as_millis() as u32;
    metrics.tcp_ms = Some(tcp_ms);
    
    // TLS handshake
    let tls_start = Instant::now();
    
    let mut root_store = RootCertStore::empty();
    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    
    let config = ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();
    
    let connector = TlsConnector::from(Arc::new(config));
    let server_name = ServerName::try_from(host.to_string())?;
    let tls_stream = connector.connect(server_name, tcp_stream).await?;
    let tls_ms = tls_start.elapsed().as_millis() as u32;
    metrics.tls_ms = Some(tls_ms);
    
    // HTTP request
    let http_start = Instant::now();
    let (mut reader, mut writer) = tokio::io::split(tls_stream);
    
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nUser-Agent: HappyEyeballs/1.0\r\n\r\n",
        path, host
    );
    
    writer.write_all(request.as_bytes()).await?;
    
    let mut response = Vec::new();
    reader.read_to_end(&mut response).await?;
    let http_ms = http_start.elapsed().as_millis() as u32;
    metrics.http_ms = Some(http_ms);
    
    // Parse status line
    let response_str = String::from_utf8_lossy(&response);
    if let Some(first_line) = response_str.lines().next() {
        metrics.status_line = Some(first_line.to_string());
    }
    
    Ok(())
}
