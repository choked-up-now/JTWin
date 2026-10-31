use anyhow::{Context, Result};
use bendy::serde::from_bytes;
use percent_encoding::percent_decode_str;
use rand::Rng;
use reqwest::Url;
use serde::Deserialize;
use std::path::Path;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

// ============================================================
// 1. 数据结构
// ============================================================

#[derive(Debug, Deserialize)]
pub struct TorrentInfo {
    pub name: String,
    #[serde(rename = "piece length")]
    pub piece_length: u64,
    #[serde(with = "serde_bytes")]
    pub pieces: Vec<u8>,
    pub length: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct TorrentMeta {
    pub announce: String,
    pub info: TorrentInfo,
}

/// 解析后的磁力链接信息
#[derive(Debug, Clone)]
pub struct MagnetInfo {
    pub info_hash: Vec<u8>, // 20字节 SHA1
    pub display_name: String,
    pub trackers: Vec<String>,
}

// ============================================================
// 2. Magnet URI 解析
// ============================================================

/// 解析 magnet:?xt=urn:btih:<hash>&dn=<name>&tr=<tracker>...
pub fn parse_magnet(uri: &str) -> Result<MagnetInfo> {
    let stripped = uri
        .strip_prefix("magnet:?")
        .context("无效的磁力链接，必须以 magnet:? 开头")?;

    let mut info_hash_hex: Option<String> = None;
    let mut display_name = String::from("Unknown_Magnet");
    let mut trackers = Vec::new();

    for pair in stripped.split('&') {
        let mut kv = pair.splitn(2, '=');
        let key = kv.next().unwrap_or("");
        let val = kv.next().unwrap_or("");

        match key {
            "xt" => {
                // xt=urn:btih:<hash>
                if let Some(hash_str) = val.strip_prefix("urn:btih:") {
                    info_hash_hex = Some(hash_str.to_string());
                }
            }
            "dn" => {
                display_name = percent_decode_str(val).decode_utf8_lossy().to_string();
            }
            "tr" => {
                let tracker = percent_decode_str(val).decode_utf8_lossy().to_string();
                trackers.push(tracker);
            }
            _ => {}
        }
    }

    let hex_hash = info_hash_hex.context("磁力链接中缺少 xt=urn:btih:<hash> 参数")?;

    // info_hash 可以是 40 位十六进制 (20字节) 或 32 位 Base32
    let info_hash = if hex_hash.len() == 40 {
        decode_hex(&hex_hash)?
    } else if hex_hash.len() == 32 {
        decode_base32(&hex_hash)?
    } else {
        anyhow::bail!(
            "不支持的 info_hash 长度: {} (需要 40 位十六进制或 32 位 Base32)",
            hex_hash.len()
        );
    };

    if trackers.is_empty() {
        // 如果磁力链接没带 tracker，给一个公共的兜底
        trackers.push("udp://tracker.opentrackr.org:1337/announce".to_string());
    }

    Ok(MagnetInfo {
        info_hash,
        display_name,
        trackers,
    })
}

fn decode_hex(s: &str) -> Result<Vec<u8>> {
    if s.len() % 2 != 0 {
        anyhow::bail!("十六进制字符串长度必须为偶数");
    }
    let mut bytes = Vec::with_capacity(s.len() / 2);
    for i in (0..s.len()).step_by(2) {
        let byte =
            u8::from_str_radix(&s[i..i + 2], 16).context("十六进制字符串包含非法字符")?;
        bytes.push(byte);
    }
    Ok(bytes)
}

fn decode_base32(s: &str) -> Result<Vec<u8>> {
    // 简单的 Base32 解码（RFC 4648）
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut buffer: u64 = 0;
    let mut bits_left: u32 = 0;
    let mut result = Vec::new();

    for &c in s.as_bytes() {
        let val = ALPHABET
            .iter()
            .position(|&x| x == c.to_ascii_uppercase())
            .context("Base32 字符串包含非法字符")? as u64;
        buffer = (buffer << 5) | val;
        bits_left += 5;
        if bits_left >= 8 {
            bits_left -= 8;
            result.push((buffer >> bits_left) as u8);
            buffer &= (1 << bits_left) - 1;
        }
    }
    Ok(result)
}

// ============================================================
// 3. Tracker 请求与 Compact Peers 解析
// ============================================================

/// 生成 20 字节随机 Peer ID (格式: -JT0001-xxxxxxxxxxxx)
pub fn generate_peer_id() -> Vec<u8> {
    let mut rng = rand::thread_rng();
    // 修复：使用 fill 代替 gen，以兼容 Rust 2024 edition 的保留字
    let mut random_bytes = [0u8; 12];
    rng.fill(&mut random_bytes);
    let mut peer_id = b"-JT0001-".to_vec();
    peer_id.extend_from_slice(&random_bytes);
    peer_id
}

/// 向 Tracker 发送 HTTP GET 请求，获取 Peer 列表
/// BEP-23: compact=1 时，peers 字段是 6 字节/peer 的紧凑二进制串
pub async fn fetch_peers(announce_url: &str, info_hash: &[u8]) -> Result<Vec<String>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    let peer_id = generate_peer_id();

    // 构建 URL 参数（info_hash 和 peer_id 是二进制，必须百分号编码）
    let mut url = Url::parse(announce_url)?;
    url.query_pairs_mut()
        .append_pair("info_hash", &String::from_utf8_lossy(info_hash))
        .append_pair("peer_id", &String::from_utf8_lossy(&peer_id))
        .append_pair("port", "6881")
        .append_pair("uploaded", "0")
        .append_pair("downloaded", "0")
        .append_pair("left", "1024")
        .append_pair("compact", "1")
        .append_pair("event", "started");

    println!("[P2P] 正在请求 Tracker: {}", announce_url);
    let res = client.get(url).send().await?;
    let status = res.status();
    let bytes = res.bytes().await?;

    if !status.is_success() {
        anyhow::bail!("Tracker 返回 HTTP 状态码: {}", status);
    }

    #[derive(Deserialize, Debug)]
    struct TrackerResponse {
        peers: Option<serde_bytes::ByteBuf>,
        failure: Option<String>,
    }

    let parsed: TrackerResponse = from_bytes(&bytes).context("Tracker 响应 Bencode 解析失败")?;

    if let Some(fail) = parsed.failure {
        anyhow::bail!("Tracker 返回错误: {}", fail);
    }

    let mut peer_list = Vec::new();
    if let Some(peers) = parsed.peers {
        // Compact 格式：每 6 字节代表一个 Peer（4 字节 IPv4 + 2 字节大端端口）
        for chunk in peers.chunks_exact(6) {
            let ip = format!("{}.{}.{}.{}", chunk[0], chunk[1], chunk[2], chunk[3]);
            let port = u16::from_be_bytes([chunk[4], chunk[5]]);
            peer_list.push(format!("{}:{}", ip, port));
        }
    }

    Ok(peer_list)
}

// ============================================================
// 4. Peer Wire 握手 (BEP-3)
// ============================================================

/// 与指定 Peer 进行 BitTorrent 握手
/// 握手包固定 68 字节：<pstrlen(19)><pstr("BitTorrent protocol")><reserved(8B)><info_hash(20B)><peer_id(20B)>
pub async fn handshake_with_peer(
    peer_addr: &str,
    info_hash: &[u8],
    peer_id: &[u8],
) -> Result<TcpStream> {
    let mut stream = TcpStream::connect(peer_addr)
        .await
        .with_context(|| format!("无法连接到 Peer: {}", peer_addr))?;

    let mut handshake = Vec::with_capacity(68);
    handshake.push(19u8);
    handshake.extend_from_slice(b"BitTorrent protocol");
    handshake.extend_from_slice(&[0u8; 8]); // reserved 扩展位，暂时全 0
    handshake.extend_from_slice(info_hash);
    handshake.extend_from_slice(peer_id);

    stream.write_all(&handshake).await?;
    stream.flush().await?;

    // 读取对方回传的 68 字节握手包
    let mut response = [0u8; 68];
    stream.read_exact(&mut response).await?;

    if response[0] != 19 || &response[1..20] != b"BitTorrent protocol" {
        anyhow::bail!("Peer {} 握手协议不匹配", peer_addr);
    }

    if &response[28..48] != info_hash {
        anyhow::bail!("Peer {} 返回的 info_hash 与本地不一致", peer_addr);
    }

    println!("[P2P] ✅ 成功与 Peer {} 完成握手", peer_addr);
    Ok(stream)
}

// ============================================================
// 5. 统一入口：download_p2p
// ============================================================

pub async fn download_p2p(
    url: &str,
    _save_dir: &str,
    progress_callback: impl Fn(f32, String) + Send + Sync + 'static,
) -> Result<()> {
    progress_callback(0.05, "正在初始化 P2P 引擎...".to_string());

    if url.starts_with("magnet:") {
        let magnet = parse_magnet(url)?;
        progress_callback(
            0.2,
            format!(
                "已解析磁力链接: {} (info_hash: {})",
                magnet.display_name,
                hex_encode(&magnet.info_hash)
            ),
        );

        // 1. 向 Tracker 请求 Peer 列表
        let mut all_peers = Vec::new();
        for tracker in &magnet.trackers {
            progress_callback(0.3, format!("正在向 Tracker 请求 Peers: {}", tracker));
            match fetch_peers(tracker, &magnet.info_hash).await {
                Ok(peers) => {
                    progress_callback(0.4, format!("Tracker 返回 {} 个 Peer", peers.len()));
                    all_peers.extend(peers);
                }
                Err(e) => {
                    progress_callback(0.4, format!("Tracker 请求失败: {}", e));
                }
            }
        }

        if all_peers.is_empty() {
            progress_callback(1.0, "❌ 未能获取到任何 Peer，请检查磁力链接或网络".to_string());
            return Ok(());
        }

        // 2. 尝试与第一个 Peer 握手
        let target_peer = &all_peers[0];
        progress_callback(0.5, format!("正在尝试与 Peer {} 握手...", target_peer));

        let peer_id = generate_peer_id();
        match handshake_with_peer(target_peer, &magnet.info_hash, &peer_id).await {
            Ok(_stream) => {
                progress_callback(
                    1.0,
                    format!(
                        "🎉 成功连接到 Peer {}！P2P 下载逻辑将在 v0.6.0 实现",
                        target_peer
                    ),
                );
            }
            Err(e) => {
                progress_callback(1.0, format!("❌ 与 Peer 握手失败: {}", e));
            }
        }
    } else if url.ends_with(".torrent") {
        if Path::new(url).exists() {
            let bytes = std::fs::read(url)?;
            let meta: TorrentMeta = from_bytes(&bytes).context("解析种子文件失败，请检查格式")?;
            progress_callback(
                0.5,
                format!(
                    "已解析种子文件: {} ({} bytes)",
                    meta.info.name,
                    meta.info.length.unwrap_or(0)
                ),
            );
            progress_callback(1.0, "BT 文件下载逻辑正在开发中 (v0.6.0 见)！".to_string());
        } else {
            anyhow::bail!("找不到指定的种子文件: {}", url);
        }
    } else {
        anyhow::bail!("不支持的 P2P 格式");
    }

    Ok(())
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}