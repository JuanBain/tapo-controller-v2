use serde::Serialize;
use std::net::UdpSocket;
use tokio::net::TcpStream;
use tokio::time::{timeout, Duration};

#[derive(Debug, Clone, Serialize)]
pub struct DiscoveredDevice {
    pub ip: String,
    pub reachable: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiscoveryResult {
    pub local_ip: String,
    pub network: String,
    pub potential_devices: Vec<DiscoveredDevice>,
}

/// Get local IP by connecting a UDP socket to 8.8.8.8
fn get_local_ip() -> Result<String, String> {
    let socket = UdpSocket::bind("0.0.0.0:0")
        .map_err(|e| format!("Failed to bind UDP socket: {}", e))?;
    socket.connect("8.8.8.8:80")
        .map_err(|e| format!("Failed to connect UDP socket: {}", e))?;
    let addr = socket.local_addr()
        .map_err(|e| format!("Failed to get local address: {}", e))?;
    Ok(addr.ip().to_string())
}

/// Check if a single IP has port 80 open (potential Tapo device)
async fn check_device(ip: String) -> Option<DiscoveredDevice> {
    let addr = format!("{}:80", ip);
    match timeout(Duration::from_millis(500), TcpStream::connect(&addr)).await {
        Ok(Ok(_)) => Some(DiscoveredDevice {
            ip,
            reachable: true,
        }),
        _ => None,
    }
}

/// Scan the local /24 subnet for devices with port 80 open
pub async fn discover_devices() -> Result<DiscoveryResult, String> {
    let local_ip = get_local_ip()?;
    let parts: Vec<&str> = local_ip.split('.').collect();
    if parts.len() != 4 {
        return Err("Invalid local IP format".to_string());
    }
    let network_prefix = format!("{}.{}.{}", parts[0], parts[1], parts[2]);

    let mut handles = Vec::new();
    for i in 1..255u8 {
        let ip = format!("{}.{}", network_prefix, i);
        handles.push(tokio::spawn(check_device(ip)));
    }

    let mut devices = Vec::new();
    for handle in handles {
        if let Ok(Some(device)) = handle.await {
            devices.push(device);
        }
    }

    Ok(DiscoveryResult {
        local_ip,
        network: format!("{}.0/24", network_prefix),
        potential_devices: devices,
    })
}
