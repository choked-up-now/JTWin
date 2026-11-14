use crate::core::action::ActionManager;
use anyhow::{bail, Result};
use std::path::Path;

/// 智能识别链接类型（HTTP / P2P）
pub fn is_p2p_url(url: &str) -> bool {
    url.starts_with("magnet:") || url.ends_with(".torrent")
}

/// 统一的下载后动作执行
pub fn handle_post_download(file_path: &Path, auto_action: bool) -> Result<Option<String>> {
    if !file_path.exists() {
        bail!("下载文件不存在，无法执行后续动作");
    }

    if auto_action {
        let manager = ActionManager::new();
        match manager.trigger_action(file_path) {
            Ok(msg) => Ok(msg),
            Err(e) => bail!("动作执行失败: {}", e),
        }
    } else {
        Ok(None)
    }
}

/// 从 URL 中智能提取文件名（统一逻辑）
pub fn extract_filename_from_url(url: &str) -> Option<String> {
    if url.starts_with("magnet:?") {
        if let Some(dn_part) = url.split('&').find(|s| s.starts_with("dn=")) {
            let name = &dn_part[3..];
            return Some(percent_encoding::percent_decode_str(name).decode_utf8_lossy().to_string());
        }
        return None;
    }
    let path = url.split('?').next()?;
    let parts: Vec<&str> = path.split('/').collect();
    let last = parts.last()?;
    if !last.is_empty() && last.contains('.') {
        Some(last.to_string())
    } else {
        None
    }
}