use crate::core::scheduler::Scheduler;
use crate::core::tracker::UdpTracker;
use crate::core::mse::mse_handshake_client;
use anyhow::{Context, Result, bail};
use percent_encoding::percent_decode_str;
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use std::sync::Arc;
use tokio::net::TcpStream;

#[derive(Debug, Clone)]
pub struct MagnetInfo {
    pub info_hash: Vec<u8>,
    pub display_name: String,
    pub trackers: Vec<String>,
}

pub fn parse_magnet(uri: &str) -> Result<MagnetInfo> {
    let stripped = uri.strip_prefix("magnet:?").context("无效的磁力链接")?;
    let mut info_hash_hex = None;
    let mut display_name = "Unknown".to_string();
    let mut trackers = Vec::new();

    for pair in stripped.split('&') {
        let mut kv = pair.splitn(2, '=');
        let key = kv.next().unwrap_or("");
        let val = kv.next().unwrap_or("");
        match key {
            "xt" => {
                if let Some(hash) = val.strip_prefix("urn:btih:") {
                    info_hash_hex = Some(hash.to_string());
                }
            }
            "dn" => display_name = percent_decode_str(val).decode_utf8_lossy().to_string(),
            "tr" => trackers.push(percent_decode_str(val).decode_utf8_lossy().to_string()),
            _ => {}
        }
    }

    let hex_hash = info_hash_hex.context("缺少 info_hash")?;
    let info_hash = if hex_hash.len() == 40 {
        decode_hex(&hex_hash)?
    } else {
        bail!("不支持的哈希长度");
    };

    Ok(MagnetInfo { info_hash, display_name, trackers })
}

fn decode_hex(s: &str) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    for i in (0..s.len()).step_by(2) {
        bytes.push(u8::from_str_radix(&s[i..i + 2], 16)?);
    }
    Ok(bytes)
}

pub fn generate_peer_id() -> Vec<u8> {
    let mut rng = StdRng::from_entropy();
    let mut random_bytes = [0u8; 12];
    rng.fill(&mut random_bytes);
    let mut peer_id = b"-JT0001-".to_vec();
    peer_id.extend_from_slice(&random_bytes);
    peer_id
}

pub async fn handshake_with_peer(peer_addr: &str, info_hash: &[u8], peer_id: &[u8]) -> Result<crate::core::mse::MseStream> {
    let stream = tokio::time::timeout(std::time::Duration::from_secs(8), TcpStream::connect(peer_addr))
        .await
        .context("连接超时")??;

    let ih_array: [u8; 20] = info_hash.try_into().map_err(|_| anyhow::anyhow!("info_hash 长度错误"))?;

    let mut mse_stream = mse_handshake_client(stream, &ih_array).await?;

    let mut handshake = Vec::with_capacity(68);
    handshake.push(19u8);
    handshake.extend_from_slice(b"BitTorrent protocol");
    handshake.extend_from_slice(&[0u8; 8]);
    handshake.extend_from_slice(info_hash);
    handshake.extend_from_slice(peer_id);

    mse_stream.write_encrypted(&handshake).await?;

    let mut response = [0u8; 68];
    mse_stream.read_decrypted(&mut response).await?;

    if response[0] != 19 || &response[1..20] != b"BitTorrent protocol" {
        bail!("BitTorrent 协议不匹配");
    }
    if &response[28..48] != info_hash {
        bail!("info_hash 不匹配");
    }

    Ok(mse_stream)
}

pub async fn download_p2p(
    url: &str,
    _save_dir: &str,
    progress_callback: impl Fn(f32, String) + Send + Sync + 'static,
) -> Result<()> {
    progress_callback(0.1, "正在解析磁力链接...".to_string());

    if url.starts_with("magnet:") {
        let magnet = parse_magnet(url)?;
        progress_callback(0.2, format!("已解析: {} (包含 {} 个 Tracker)", magnet.display_name, magnet.trackers.len()));

        if magnet.trackers.is_empty() {
            progress_callback(1.0, "❌ 该磁力链接没有 Tracker，暂不支持 DHT 发现".to_string());
            return Ok(());
        }

        let mut all_peers = Vec::new();
        for tracker_url in &magnet.trackers {
            if !tracker_url.starts_with("udp://") { continue; }
            progress_callback(0.3, format!("正在连接 Tracker: {}", tracker_url));
            
            match UdpTracker::new(tracker_url).await {
                Ok(tracker) => {
                    let peer_id = generate_peer_id();
                    match tracker.announce(&magnet.info_hash, &peer_id, 6881).await {
                        Ok(peers) => {
                            progress_callback(0.4, format!("Tracker 返回 {} 个 Peer", peers.len()));
                            all_peers.extend(peers);
                        }
                        Err(e) => progress_callback(0.4, format!("Announce 失败: {}", e)),
                    }
                }
                Err(e) => progress_callback(0.3, format!("Tracker 初始化失败: {}", e)),
            }
        }

        if all_peers.is_empty() {
            progress_callback(1.0, "❌ 未获取到任何 Peer".to_string());
            return Ok(());
        }

        let scheduler = Arc::new(Scheduler::new(32, 10));
        scheduler.register_task();
        let peer_id = generate_peer_id();
        let mut handles = vec![];

        for peer in all_peers.into_iter().take(10) {
            let peer_id = peer_id.clone();
            let info_hash = magnet.info_hash.clone();
            let sem = scheduler.global_limit.clone();
            
            let h = tokio::spawn(async move {
                let _permit = sem.acquire().await.unwrap();
                match handshake_with_peer(&peer, &info_hash, &peer_id).await {
                    Ok(_stream) => println!("[P2P] ✅ 成功与公网 Peer {} 完成 MSE 加密握手", peer),
                    Err(e) => eprintln!("[P2P] ❌ 与 Peer {} 握手失败: {}", peer, e),
                }
            });
            handles.push(h);
        }

        for h in handles { let _ = h.await; }
        progress_callback(1.0, "🎉 公网 Peer 加密握手测试结束！".to_string());
    } else {
        bail!("目前只支持 magnet 测试");
    }
    Ok(())
}