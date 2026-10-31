use anyhow::{Context, Result};
use bendy::serde::from_bytes;
use percent_encoding::percent_decode_str;
use serde::Deserialize;
use std::path::Path;

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

pub async fn download_p2p(url: &str, _save_dir: &str, progress_callback: impl Fn(f32, String) + Send + Sync + 'static) -> Result<()> {
    progress_callback(0.1, "正在初始化 P2P 引擎...".to_string());
    
    if url.starts_with("magnet:") {
        // 简单提取磁力链接的 dn (Display Name)
        let dn = url.split('&')
            .find(|s| s.starts_with("dn="))
            .map(|s| percent_decode_str(&s[3..]).decode_utf8_lossy().to_string())
            .unwrap_or_else(|| "Unknown_Magnet".to_string());
        
        progress_callback(0.5, format!("已解析磁力链接: {}", dn));
        progress_callback(1.0, "磁力下载功能正在开发中 (v0.5.0 见)！".to_string());
        // TODO: 接入 DHT 和真实的 P2P 下载
    } else if url.ends_with(".torrent") {
        // 如果是本地 .torrent 文件，尝试解析
        if Path::new(url).exists() {
            let bytes = std::fs::read(url)?;
            let meta: TorrentMeta = from_bytes(&bytes).context("解析种子文件失败，请检查格式")?;
            progress_callback(0.5, format!("已解析种子文件: {} ({} bytes)", meta.info.name, meta.info.length.unwrap_or(0)));
            progress_callback(1.0, "BT 下载功能正在开发中 (v0.5.0 见)！".to_string());
            // TODO: 接入 Peer 连接和下载
        } else {
            anyhow::bail!("找不到指定的种子文件: {}", url);
        }
    } else {
        anyhow::bail!("不支持的 P2P 格式");
    }
    
    Ok(())
}