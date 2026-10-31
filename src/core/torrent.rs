use anyhow::{Context, Result};
use percent_encoding::percent_decode_str;
use rand::Rng;
use std::path::Path;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[derive(Debug, Clone)]
pub struct MagnetInfo {
    pub info_hash: Vec<u8>,
    pub display_name: String,
    pub file_size: u64,
}

pub fn parse_magnet(uri: &str) -> Result<MagnetInfo> {
    let stripped = uri.strip_prefix("magnet:?").context("无效的磁力链接")?;
    let mut info_hash_hex = None;
    let mut display_name = "Unknown".to_string();
    let mut file_size = 0;

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
            "xl" => file_size = val.parse::<u64>().unwrap_or(0),
            _ => {}
        }
    }

    let hex_hash = info_hash_hex.context("缺少 info_hash")?;
    let info_hash = if hex_hash.len() == 40 {
        decode_hex(&hex_hash)?
    } else {
        anyhow::bail!("不支持的哈希长度");
    };

    Ok(MagnetInfo { info_hash, display_name, file_size })
}

fn decode_hex(s: &str) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    for i in (0..s.len()).step_by(2) {
        bytes.push(u8::from_str_radix(&s[i..i + 2], 16)?);
    }
    Ok(bytes)
}

pub fn generate_peer_id() -> Vec<u8> {
    let mut rng = rand::thread_rng();
    let mut random_bytes = [0u8; 12];
    rng.fill(&mut random_bytes);
    let mut peer_id = b"-JT0001-".to_vec();
    peer_id.extend_from_slice(&random_bytes);
    peer_id
}

pub async fn handshake_with_peer(
    peer_addr: &str,
    info_hash: &[u8],
    peer_id: &[u8],
) -> Result<TcpStream> {
    let mut stream = TcpStream::connect(peer_addr).await?;

    let mut handshake = Vec::with_capacity(68);
    handshake.push(19u8);
    handshake.extend_from_slice(b"BitTorrent protocol");
    handshake.extend_from_slice(&[0u8; 8]);
    handshake.extend_from_slice(info_hash);
    handshake.extend_from_slice(peer_id);

    stream.write_all(&handshake).await?;
    stream.flush().await?;

    let mut response = [0u8; 68];
    stream.read_exact(&mut response).await?;

    if response[0] != 19 || &response[1..20] != b"BitTorrent protocol" {
        anyhow::bail!("协议不匹配");
    }
    if &response[28..48] != info_hash {
        anyhow::bail!("info_hash 不匹配");
    }

    Ok(stream)
}

async fn send_interested(stream: &mut TcpStream) -> Result<()> {
    stream.write_all(&[0, 0, 0, 1, 2]).await?;
    stream.flush().await?;
    Ok(())
}

async fn send_request(stream: &mut TcpStream, index: u32, begin: u32, length: u32) -> Result<()> {
    let mut msg = Vec::with_capacity(17);
    msg.extend_from_slice(&13u32.to_be_bytes());
    msg.push(6);
    msg.extend_from_slice(&index.to_be_bytes());
    msg.extend_from_slice(&begin.to_be_bytes());
    msg.extend_from_slice(&length.to_be_bytes());
    stream.write_all(&msg).await?;
    stream.flush().await?;
    Ok(())
}

pub async fn download_p2p(
    url: &str,
    save_dir: &str,
    progress_callback: impl Fn(f32, String) + Send + Sync + 'static,
) -> Result<()> {
    progress_callback(0.1, "正在解析磁力链接...".to_string());

    if url.starts_with("magnet:") {
        let magnet = parse_magnet(url)?;
        progress_callback(0.2, format!("已解析: {} ({} bytes)", magnet.display_name, magnet.file_size));

        // 【本地直连测试】写死 qBittorrent 端口 18082
        let target_peer = "127.0.0.1:18082"; 
        progress_callback(0.3, format!("正在连接本地做种节点 {} ...", target_peer));

        let peer_id = generate_peer_id();
        let mut stream = match handshake_with_peer(target_peer, &magnet.info_hash, &peer_id).await {
            Ok(s) => s,
            Err(e) => {
                progress_callback(1.0, format!("❌ 握手失败: {}", e));
                return Ok(());
            }
        };
        
        progress_callback(0.4, "✅ 握手成功，发送 Interested 消息...".to_string());
        send_interested(&mut stream).await?;

        progress_callback(0.5, "等待对方 Unchoke...".to_string());

        loop {
            let mut len_buf = [0u8; 4];
            if stream.read_exact(&mut len_buf).await.is_err() {
                anyhow::bail!("连接被断开");
            }
            let msg_len = u32::from_be_bytes(len_buf);

            if msg_len == 0 { continue; }

            let mut msg_id = [0u8; 1];
            stream.read_exact(&mut msg_id).await?;

            match msg_id[0] {
                1 => { 
                    progress_callback(0.6, "✅ 收到 Unchoke，正在请求数据块...".to_string());
                    let length = if magnet.file_size > 0 { magnet.file_size as u32 } else { 16384 };
                    send_request(&mut stream, 0, 0, length).await?;
                }
                7 => { 
                    let mut payload = vec![0u8; (msg_len - 1) as usize];
                    stream.read_exact(&mut payload).await?;
                    
                    let index = u32::from_be_bytes(payload[0..4].try_into()?);
                    let begin = u32::from_be_bytes(payload[4..8].try_into()?);
                    let data = &payload[8..];

                    progress_callback(0.8, format!("收到 Piece 数据: Index={}, Begin={}, 长度={}", index, begin, data.len()));

                    let file_path = Path::new(save_dir).join(&magnet.display_name);
                    if !save_dir.is_empty() {
                        std::fs::create_dir_all(save_dir)?;
                    }
                    
                    use std::fs::OpenOptions;
                    use std::io::{Seek, SeekFrom, Write};
                    let mut file = OpenOptions::new()
                        .create(true)
                        .write(true)
                        .open(&file_path)?;
                    
                    file.seek(SeekFrom::Start(begin as u64))?;
                    file.write_all(data)?;

                    progress_callback(1.0, format!("🎉 成功下载并写入: {:?}", file_path));
                    break;
                }
                _ => {
                    let mut skip_buf = vec![0u8; (msg_len - 1) as usize];
                    stream.read_exact(&mut skip_buf).await?;
                }
            }
        }
    } else {
        anyhow::bail!("目前只支持 magnet 测试");
    }
    Ok(())
}