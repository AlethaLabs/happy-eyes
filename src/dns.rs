use std::{
    net::{IpAddr, SocketAddr},
    time::Instant,
};

use hickory_resolver::TokioResolver;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// DNS resolution 
pub async fn resolve_dns(host: &str, port: u16) -> Result<(Vec<SocketAddr>, u32)> {
    let dns_start = Instant::now();
    
    // Create resolver
    let resolver = TokioResolver::builder_tokio()?.build();
    
    println!("Starting DNS resolution for {}", host);
    
    // Make queries
    let qa_start = Instant::now();
    let mut qa_future = Box::pin(resolver.ipv6_lookup(host));

    let a_start = Instant::now(); 
    let mut a_future = Box::pin(resolver.ipv4_lookup(host));
    
    let mut addresses = Vec::new();

    let mut qa_completed = false;
    let mut a_completed = false;
    
    // Handle whichever response comes first
    tokio::select! {
        qa_result = &mut qa_future, if !qa_completed => {
            // AAAA came first - process immediately 
            let qa_ms = qa_start.elapsed().as_millis() as u32;

            #[allow(unused_assignments)]
            { qa_completed = true; }
            
            match qa_result {
                Ok(qa_lookup) => {
                    for qa_record in qa_lookup.iter() {
                        addresses.push(SocketAddr::new(IpAddr::V6(qa_record.0), port));
                    }
                    println!("AAAA query completed FIRST in {}ms - {} IPv6 addresses", qa_ms, qa_lookup.iter().count());
                }
                Err(e) => {
                    println!("AAAA query failed in {}ms: {}", qa_ms, e);
                }
            }
            
            // Still wait for A query to complete
            if !a_completed {
                match a_future.await {
                    Ok(a_lookup) => {
                        let a_ms = a_start.elapsed().as_millis() as u32;
                        for a_record in a_lookup.iter() {
                            addresses.push(SocketAddr::new(IpAddr::V4(a_record.0), port));
                        }
                        println!("A query completed in {}ms - {} IPv4 addresses", a_ms, a_lookup.iter().count());
                    }
                    Err(e) => {
                        let a_ms = a_start.elapsed().as_millis() as u32;
                        println!("A query failed in {}ms: {}", a_ms, e);
                    }
                }
            }
        }
        a_result = &mut a_future, if !a_completed => {
            // A came first - implement 50ms Resolution Delay for IPv6 preference
            let a_ms = a_start.elapsed().as_millis() as u32;
            #[allow(unused_assignments)]
            { a_completed = true; }
            
            println!("A query completed FIRST in {}ms - waiting 50ms Resolution Delay for AAAA", a_ms);
            
            // Wait 50ms for potential AAAA response - Resolution Delay
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            
            // Check if AAAA completed during the 50ms wait
            if !qa_completed {
                match tokio::time::timeout(tokio::time::Duration::from_millis(0), &mut qa_future).await {
                    Ok(qa_result) => {
                        // AAAA completed during wait - process it first (IPv6 preference)
                        let qa_ms = qa_start.elapsed().as_millis() as u32;
                        match qa_result {
                            Ok(qa_lookup) => {
                                for qa_record in qa_lookup.iter() {
                                    addresses.push(SocketAddr::new(IpAddr::V6(qa_record.0), port));
                                }
                                println!("AAAA query completed during Resolution Delay in {}ms - {} IPv6 addresses", qa_ms, qa_lookup.iter().count());
                            }
                            Err(e) => {
                                println!("AAAA query failed during Resolution Delay in {}ms: {}", qa_ms, e);
                            }
                        }
                        #[allow(unused_assignments)]
                        { qa_completed = true; }
                    }
                    Err(_) => {
                        // AAAA still not ready after 50ms wait - continue without it
                        println!("AAAA query still pending after 50ms Resolution Delay - proceeding with A records");
                    }
                }
            }
            
            // Process A result
            match a_result {
                Ok(a_lookup) => {
                    for a_record in a_lookup.iter() {
                        addresses.push(SocketAddr::new(IpAddr::V4(a_record.0), port));
                    }
                    println!("A query processed - {} IPv4 addresses", a_lookup.iter().count());
                }
                Err(e) => {
                    println!("A query failed: {}", e);
                }
            }
        }
    }
    
    let total_dns_ms = dns_start.elapsed().as_millis() as u32;

    if addresses.is_empty() {
        return Err("No addresses found for host".into());
    }
    
    println!("DNS resolution complete: {} total addresses in {}ms", addresses.len(), total_dns_ms);
    println!("   Addresses: {:#?}", addresses);
    
    Ok((addresses, total_dns_ms))
}
