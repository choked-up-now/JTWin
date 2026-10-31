use anyhow::{Context, Result};
use rand::Rng;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::UdpSocket;

const PROTOCOL_ID: u64 = 0x0000_0417_2710_1980;

pub struct UdpTracker {
    socket: UdpSocket,
    tracker_addr: SocketAddr,
}

impl UdpTracker {
    pub async fn new(tracker_url: &str) -> Result<Self> {
        let addr_str = tracker_url.strip_prefix("udp://").unwrap_or(tracker_url);
        let addr_str = addr_str.split('/').next().unwrap_or(addr_str);

        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        let addr = tokio::net::lookup_host(addr_str).await?
            .next().context("无法解析 Tracker 域名")?;
        
        Ok(Self { socket, tracker_addr: addr })
    }

    async fn connect(&self) -> Result<u64> {
        let mut transaction_id = [0u8; 4];
        rand::thread_rng().fill(&mut transaction_id);

        let mut req = Vec::with_capacity(16);
        req.extend_from_slice(&PROTOCOL_ID.to_be_bytes());
        req.extend_from_slice(&0u32.to_be_bytes()); // action = 0
        req.extend_from_slice(&transaction_id);

        self.socket.send_to(&req, self.tracker_addr).await?;

        let mut buf = [0u8; 16];
        let (len, _) = tokio::time::timeout(Duration::from_secs(5), self.socket.recv_from(&mut buf)).await??;

        if len < 16 { anyhow::bail!("Tracker 响应过短"); }
        let action = u32::from_be_bytes(buf[0..4].try_into()?);
        if action != 0 { anyhow::bail!("Tracker 返回错误 action: {}", action); }
        if buf[4..8] != transaction_id { anyhow::bail!("Transaction ID 不匹配"); }

        let connection_id = u64::from_be_bytes(buf[8..16].try_into()?);
        Ok(connection_id)
    }

    pub async fn announce(&self, info_hash: &[u8], peer_id: &[u8], port: u16) -> Result<Vec<String>> {
        let connection_id = self.connect().await?;
        let mut transaction_id = [0u8; 4];
        rand::thread_rng().fill(&mut transaction_id);

        let mut req = Vec::with_capacity(98);
        req.extend_from_slice(&connection_id.to_be_bytes());
        req.extend_from_slice(&1u32.to_be_bytes()); // action = 1
        req.extend_from_slice(&transaction_id);
        req.extend_from_slice(info_hash);
        req.extend_from_slice(peer_id);
        req.extend_from_slice(&0u64.to_be_bytes()); // downloaded
        req.extend_from_slice(&0u64.to_be_bytes()); // left
        req.extend_from_slice(&0u64.to_be_bytes()); // uploaded
        req.extend_from_slice(&0u32.to_be_bytes()); // event = 0 (none)
        req.extend_from_slice(&0u32.to_be_bytes()); // ip
        req.extend_from_slice(&0u32.to_be_bytes()); // key
        req.extend_from_slice(&(-1i32).to_be_bytes()); // num_want (-1)
        req.extend_from_slice(&port.to_be_bytes());

        self.socket.send_to(&req, self.tracker_addr).await?;

        let mut buf = [0u8; 2048];
        let (len, _) = tokio::time::timeout(Duration::from_secs(5), self.socket.recv_from(&mut buf)).await??;

        if len < 20 { anyhow::bail!("Announce 响应过短"); }
        let action = u32::from_be_bytes(buf[0..4].try_into()?);
        if action != 1 { anyhow::bail!("Announce 返回错误 action: {}", action); }

        let mut peers = Vec::new();
        let peer_data = &buf[20..len];
        for chunk in peer_data.chunks_exact(6) {
            let ip = format!("{}.{}.{}.{}", chunk[0], chunk[1], chunk[2], chunk[3]);
            let port = u16::from_be_bytes([chunk[4], chunk[5]]);
            peers.push(format!("{}:{}", ip, port));
        }
        Ok(peers)
    }
}