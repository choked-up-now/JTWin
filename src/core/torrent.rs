use crate::core::mse::MseStream;
use crate::core::tracker::UdpTracker;
use anyhow::{bail, Context, Result};
use bendy::serde::from_bytes;
use percent_encoding::percent_decode_str;
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use serde::Deserialize;
use sha1::{Digest, Sha1};
use std::collections::{HashMap, VecDeque};
use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::Mutex;

// ==========================================
// 数据结构定义
// ==========================================

#[derive(Debug, Clone)]
pub struct MagnetInfo {
    pub info_hash: Vec<u8>,
    pub display_name: String,
    pub trackers: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct TorrentInfoDict {
    name: String,
    #[serde(rename = "piece length")]
    piece_length: u64,
    pieces: serde_bytes::ByteBuf,
    length: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct TorrentMetaOuter {
    info: TorrentInfoDict,
}

#[derive(Debug, Clone)]
pub struct TorrentMeta {
    pub piece_length: u64,
    pub pieces: Vec<u8>,
    pub total_size: u64,
    pub file_name: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct UtMetadataHandshake {
    m: HashMap<String, u8>,
    metadata_size: Option<u64>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct UtMetadataMsg {
    msg_type: u8,
    piece: u32,
}

struct PieceTask {
    index: u32,
    length: u32,
    is_done: bool,
    is_inflight: bool,
}

// ==========================================
// Magnet 解析与 Peer ID 生成
// ==========================================

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

// ==========================================
// 核心：获取并校验元数据 (BEP-9)
// ==========================================

async fn fetch_and_validate_metadata(
    stream: &mut MseStream,
    info_hash: &[u8; 20],
) -> Result<TorrentMeta> {
    // 1. 发送扩展握手
    let mut payload = Vec::new();
    payload.push(20); // Extended ID
    payload.push(0);  // ext_id = 0
    payload.extend_from_slice(b"d1:md11:ut_metadatai1eee");
    stream.write_encrypted(&payload).await?;

    // 2. 读取扩展握手响应
    let mut len_buf = [0u8; 4];
    stream.read_decrypted(&mut len_buf).await?;
    let msg_len = u32::from_be_bytes(len_buf);
    if msg_len == 0 { bail!("KeepAlive"); }

    let mut msg_id = [0u8; 1];
    stream.read_decrypted(&mut msg_id).await?;
    if msg_id[0] != 20 { bail!("非扩展消息"); }

    let mut ext_id = [0u8; 1];
    stream.read_decrypted(&mut ext_id).await?;

    let mut h_payload = vec![0u8; (msg_len - 2) as usize];
    stream.read_decrypted(&mut h_payload).await?;
    let h: UtMetadataHandshake = from_bytes(&h_payload).map_err(|_| anyhow::anyhow!("握手解析失败"))?;

    let metadata_size = h.metadata_size.context("Peer 不支持元数据获取")?;
    let num_blocks = (metadata_size as usize + 16383) / 16384;

    // 3. 请求元数据块
    let mut metadata = vec![0u8; metadata_size as usize];
    for piece in 0..num_blocks {
        let req = format!("d8:msg_typei0e5:piecei{}ee", piece);
        let mut msg = vec![20, 1]; // 20=Ext, 1=ut_metadata
        msg.extend_from_slice(req.as_bytes());
        stream.write_encrypted(&msg).await?;

        let mut len_buf = [0u8; 4];
        stream.read_decrypted(&mut len_buf).await?;
        let msg_len = u32::from_be_bytes(len_buf);
        let mut msg_id = [0u8; 1];
        stream.read_decrypted(&mut msg_id).await?;
        if msg_id[0] != 20 { bail!("非扩展消息"); }

        let mut ext_id = [0u8; 1];
        stream.read_decrypted(&mut ext_id).await?;

        let mut payload = vec![0u8; (msg_len - 2) as usize];
        stream.read_decrypted(&mut payload).await?;

        // 解析字典结束位置
        let mut depth = 0;
        let mut dict_end = 0;
        for (i, &b) in payload.iter().enumerate() {
            if b == b'd' { depth += 1; }
            else if b == b'e' { depth -= 1; if depth == 0 { dict_end = i + 1; break; } }
        }

        let m_msg: UtMetadataMsg = from_bytes(&payload[..dict_end]).map_err(|_| anyhow::anyhow!("元数据消息解析失败"))?;
        if m_msg.msg_type != 1 { bail!("期望数据消息"); }

        let data = &payload[dict_end..];
        let offset = piece * 16384;
        let end = (offset + data.len()).min(metadata_size as usize);
        metadata[offset..end].copy_from_slice(&data[..end - offset]);
    }

    // 4. 验证 info_hash (必须完全一致才代表这是目标种子)
    let mut hasher = Sha1::new();
    hasher.update(&metadata);
    if hasher.finalize().as_slice() != info_hash {
        bail!("info_hash 校验失败，防止了投毒攻击");
    }

    // 5. 解析元数据
    let parsed: TorrentMetaOuter = from_bytes(&metadata).map_err(|_| anyhow::anyhow!("Bencode 解析失败"))?;
    let total_size = parsed.info.length.context("不支持多文件种子")?;

    Ok(TorrentMeta {
        piece_length: parsed.info.piece_length,
        pieces: parsed.info.pieces.to_vec(),
        total_size,
        file_name: parsed.info.name,
    })
}

// ==========================================
// 核心：请求数据块 (Piece)
// ==========================================

async fn request_piece(stream: &mut MseStream, index: u32, length: u32) -> Result<Vec<u8>> {
    let mut msg = Vec::with_capacity(17);
    msg.extend_from_slice(&13u32.to_be_bytes());
    msg.push(6); // Request
    msg.extend_from_slice(&index.to_be_bytes());
    msg.extend_from_slice(&0u32.to_be_bytes()); // begin = 0
    msg.extend_from_slice(&length.to_be_bytes());
    stream.write_encrypted(&msg).await?;

    loop {
        let mut len_buf = [0u8; 4];
        stream.read_decrypted(&mut len_buf).await?;
        let msg_len = u32::from_be_bytes(len_buf);
        if msg_len == 0 { continue; }

        let mut msg_id = [0u8; 1];
        stream.read_decrypted(&mut msg_id).await?;

        if msg_id[0] == 7 { // Piece
            let mut header = [0u8; 8];
            stream.read_decrypted(&mut header).await?;
            let idx = u32::from_be_bytes(header[0..4].try_into()?);
            let begin = u32::from_be_bytes(header[4..8].try_into()?);

            let data_len = (msg_len - 9) as usize;
            let mut data = vec![0u8; data_len];
            stream.read_decrypted(&mut data).await?;

            if idx == index && begin == 0 {
                return Ok(data);
            }
        } else {
            let skip_len = (msg_len - 1) as usize;
            let mut skip = vec![0u8; skip_len];
            stream.read_decrypted(&mut skip).await?;
        }
    }
}

// ==========================================
// 总控：下载任务
// ==========================================

pub async fn download_p2p(
    url: &str,
    save_dir: &str,
    progress_callback: impl Fn(f32, String) + Send + Sync + 'static,
) -> Result<()> {
    progress_callback(0.05, "正在解析磁力链接...".to_string());
    let magnet = parse_magnet(url)?;
    let info_hash: [u8; 20] = magnet.info_hash.clone().try_into().map_err(|_| anyhow::anyhow!("info_hash 长度错误"))?;

    let mut all_peers = Vec::new();
    for tracker_url in &magnet.trackers {
        if !tracker_url.starts_with("udp://") { continue; }
        progress_callback(0.1, "正在连接 Tracker...".to_string());
        if let Ok(tracker) = UdpTracker::new(tracker_url).await {
            let peer_id = generate_peer_id();
            if let Ok(peers) = tracker.announce(&magnet.info_hash, &peer_id, 6881).await {
                all_peers.extend(peers);
            }
        }
    }

    if all_peers.is_empty() {
        bail!("未获取到任何 Peer");
    }
    progress_callback(0.2, format!("获取到 {} 个 Peer，正在连接元数据...", all_peers.len()));

    // 1. 先找任意一个可用 Peer 获取元数据
    let mut meta = None;
    for peer in &all_peers {
        if let Ok(stream) = TcpStream::connect(peer).await {
            if let Ok(mut mse) = crate::core::mse::mse_handshake_client(stream, &info_hash).await {
                if let Ok(m) = fetch_and_validate_metadata(&mut mse, &info_hash).await {
                    meta = Some(m);
                    break;
                }
            }
        }
    }

    let meta = meta.context("从所有 Peer 获取元数据失败")?;
    progress_callback(0.3, format!("元数据获取成功: {} ({} bytes)", meta.file_name, meta.total_size));

    let num_pieces = ((meta.total_size + meta.piece_length - 1) / meta.piece_length) as u32;
    
    // 2. 准备本地文件
    let file_path = Path::new(save_dir).join(&meta.file_name);
    if !save_dir.is_empty() { std::fs::create_dir_all(save_dir)?; }
    let file = OpenOptions::new().create(true).write(true).open(&file_path)?;
    file.set_len(meta.total_size)?;
    drop(file);

    // 3. 任务队列
    let mut tasks = VecDeque::new();
    for i in 0..num_pieces {
        let len = if i == num_pieces - 1 {
            (meta.total_size - i as u64 * meta.piece_length) as u32
        } else {
            meta.piece_length as u32
        };
        tasks.push_back(PieceTask { index: i, length: len, is_done: false, is_inflight: false });
    }

    let task_queue = Arc::new(Mutex::new(tasks));
    let downloaded_bytes = Arc::new(Mutex::new(0u64));
    let meta = Arc::new(meta);
    let progress_cb = Arc::new(progress_callback);
    let mut peer_handles = Vec::new();

    // 4. 启动多 Peer 下载
    for peer in all_peers.into_iter().take(10) {
        let tq = task_queue.clone();
        let db = downloaded_bytes.clone();
        let m = meta.clone();
        let cb = progress_cb.clone();
        let fp = file_path.clone();
        let ih = info_hash.clone();

        let handle = tokio::spawn(async move {
            let stream = match TcpStream::connect(&peer).await { Ok(s) => s, Err(_) => return };
            let mut mse = match crate::core::mse::mse_handshake_client(stream, &ih).await { Ok(s) => s, Err(_) => return };

            // BitTorrent 握手
            let peer_id = generate_peer_id();
            let mut hs = Vec::with_capacity(68);
            hs.push(19u8); hs.extend_from_slice(b"BitTorrent protocol"); hs.extend_from_slice(&[0u8; 8]);
            hs.extend_from_slice(&ih); hs.extend_from_slice(&peer_id);
            
            if mse.write_encrypted(&hs).await.is_err() { return; }
            let mut resp = [0u8; 68];
            if mse.read_decrypted(&mut resp).await.is_err() { return; }
            if resp[0] != 19 { return; }

            // Interested
            mse.write_encrypted(&[0, 0, 0, 1, 2]).await.ok();

            let mut unchoked = false;
            // 循环领取任务
            loop {
                if !unchoked {
                    let mut len_buf = [0u8; 4];
                    if mse.read_decrypted(&mut len_buf).await.is_err() { break; }
                    let msg_len = u32::from_be_bytes(len_buf);
                    if msg_len == 0 { continue; }
                    
                    let mut msg_id = [0u8; 1];
                    if mse.read_decrypted(&mut msg_id).await.is_err() { break; }
                    
                    if msg_id[0] == 1 { // Unchoke
                        unchoked = true;
                    } else {
                        let mut skip = vec![0u8; (msg_len - 1) as usize];
                        mse.read_decrypted(&mut skip).await.ok();
                    }
                    continue;
                }

                // 领取任务
                let task = {
                    let mut q = tq.lock().await;
                    let mut found = None;
                    for t in q.iter_mut() {
                        if !t.is_done && !t.is_inflight {
                            t.is_inflight = true;
                            found = Some((t.index, t.length));
                            break;
                        }
                    }
                    found
                };

                let (idx, length) = match task {
                    Some(t) => t,
                    None => break, // 队列空了
                };

                // 下载
                if let Ok(data) = request_piece(&mut mse, idx, length).await {
                    // 校验
                    let expected = &m.pieces[idx as usize * 20..(idx as usize + 1) * 20];
                    if Sha1::digest(&data).as_slice() == expected {
                        // 写入
                        let offset = idx as u64 * m.piece_length;
                        if let Ok(mut f) = OpenOptions::new().write(true).open(&fp) {
                            let _ = f.seek(SeekFrom::Start(offset));
                            let _ = f.write_all(&data);
                        }
                        
                        let mut q = tq.lock().await;
                        if let Some(t) = q.iter_mut().find(|t| t.index == idx) {
                            t.is_done = true;
                            t.is_inflight = false;
                        }
                        
                        let mut db_lock = db.lock().await;
                        *db_lock += data.len() as u64;
                        let p = *db_lock as f32 / m.total_size as f32;
                        cb(p, format!("P2P 下载中 ({:.2} MB)", *db_lock as f64 / 1024.0 / 1024.0));
                        
                        if *db_lock >= m.total_size {
                            break;
                        }
                    } else {
                        // 校验失败，放回队列
                        let mut q = tq.lock().await;
                        if let Some(t) = q.iter_mut().find(|t| t.index == idx) {
                            t.is_inflight = false;
                        }
                    }
                } else {
                    // 网络错误
                    let mut q = tq.lock().await;
                    if let Some(t) = q.iter_mut().find(|t| t.index == idx) {
                        t.is_inflight = false;
                    }
                    break;
                }
            }
        });
        peer_handles.push(handle);
    }

    for h in peer_handles { let _ = h.await; }
    
    // 检查是否真的下完了
    let final_bytes = *downloaded_bytes.lock().await;
    if final_bytes >= meta.total_size {
        progress_cb(1.0, "✅ P2P 下载完成！".to_string());
    } else {
        bail!("部分数据块下载失败，请重试或更换节点");
    }

    Ok(())
}