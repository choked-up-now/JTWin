use anyhow::{Context, Result};
use librqbit::{AddTorrent, AddTorrentOptions, Session, TorrentStatsState};
use std::path::PathBuf;
use std::time::Duration;

pub async fn download_p2p(
    url: &str,
    save_dir: &str,
    progress_callback: impl Fn(f32, String) + Send + Sync + 'static,
) -> Result<()> {
    progress_callback(0.0, "正在初始化 P2P 引擎...".to_string());

    let output_dir = PathBuf::from(save_dir);
    let session = Session::new(output_dir)
        .await
        .context("P2P 引擎初始化失败")?;

    let add_opts = AddTorrentOptions {
        output_folder: Some(save_dir.to_string()),
        overwrite: true,
        ..Default::default()
    };

    let response = session
        .add_torrent(AddTorrent::from_url(url), Some(add_opts))
        .await
        .context("添加 P2P 任务失败")?;

    let handle = response.into_handle().context("获取任务句柄失败")?;

    let mut last_pct = -1;

    loop {
        let stats = handle.stats();
        
        match stats.state {
            TorrentStatsState::Live => {
                let total_bytes = stats.total_bytes;
                let progress_bytes = stats.progress_bytes;

                // 【核心修复】必须确保 total_bytes > 0 且进度大于等于总大小，才算完成
                if total_bytes > 0 && progress_bytes >= total_bytes {
                    progress_callback(1.0, "✅ P2P 下载完成！".to_string());
                    break;
                }

                let total = total_bytes.max(1) as f32;
                let downloaded = progress_bytes as f32;
                let progress = downloaded / total;
                let pct = (progress * 100.0) as i32;
                
                if let Some(live) = &stats.live {
                    let speed_mbps = live.download_speed.mbps;
                    // 为了防止回调刷屏过快，这里在每次进度百分比增加时再触发
                    if pct > last_pct {
                        progress_callback(
                            progress,
                            format!("P2P 下载中 | 速度: {:.2} MB/s | {:.2}/{:.2} MB", speed_mbps, downloaded / 1024.0 / 1024.0, total / 1024.0 / 1024.0),
                        );
                        last_pct = pct;
                    }
                }
            }
            TorrentStatsState::Initializing { .. } => {
                if last_pct == -1 {
                    progress_callback(0.0, "正在获取元数据与寻找 Peer...".to_string());
                    last_pct = 0;
                }
            }
            TorrentStatsState::Paused => {
                progress_callback(0.0, "任务已暂停".to_string());
            }
            TorrentStatsState::Error => {
                anyhow::bail!("P2P 下载发生错误");
            }
        }

        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    Ok(())
}