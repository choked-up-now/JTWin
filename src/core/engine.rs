use crate::core::models::{Chunk, Task, TaskStatus};
use anyhow::{bail, Result};
use futures_util::StreamExt;
use reqwest::header::RANGE;
use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::sync::{mpsc, Mutex};

/// 拦截常见的网盘分享链接，避免下载到 HTML 页面
pub fn validate_download_url(url: &str) -> Result<()> {
    if let Ok(parsed) = reqwest::Url::parse(url) {
        if let Some(host) = parsed.host_str() {
            let cloud_drives = [
                ("pan.baidu.com", "百度网盘"), ("aliyundrive.com", "阿里云盘"),
                ("cloud.189.cn", "天翼云盘"), ("115.com", "115网盘"),
                ("weiyun.com", "腾讯微云"), ("quark.cn", "夸克网盘"),
            ];
            for (domain, name) in cloud_drives {
                if host.contains(domain) {
                    bail!("检测到 {} 分享链接。汲湍仅支持 HTTP 直链下载，请使用对应的专用客户端获取真实直链。", name);
                }
            }
        }
    }
    Ok(())
}

/// 核心下载任务（动态线程池 + AIMD 自适应）
pub async fn download_task(
    task: Arc<Mutex<Task>>,
    client: reqwest::Client,
    initial_workers: usize,
    max_workers: usize,
) -> Result<()> {
    let (_url, file_path, total_size, chunk_queue) = {
        let mut t = task.lock().await;
        t.status = TaskStatus::Downloading;
        let mut queue = Vec::new();
        for c in &t.chunks {
            if !c.is_finished {
                queue.push(c.clone());
            }
        }
        (t.url.clone(), t.file_path.clone(), t.total_size, queue)
    };

    if chunk_queue.is_empty() {
        task.lock().await.status = TaskStatus::Completed;
        return Ok(());
    }

    // 任务分发通道（队列）
    let (tx, rx) = mpsc::channel::<Chunk>(chunk_queue.len() + 100);
    for chunk in chunk_queue {
        let _ = tx.send(chunk).await;
    }
    let rx = Arc::new(Mutex::new(rx));
    
    let task_clone = Arc::clone(&task);
    let tx_clone = tx.clone();
    let rx_clone = Arc::clone(&rx);
    let file_path_str = file_path.to_string_lossy().to_string();
    
    let active_workers = Arc::new(AtomicUsize::new(initial_workers));
    let target_workers = Arc::new(AtomicUsize::new(initial_workers));
    let fcontrol_errors = Arc::new(AtomicUsize::new(0));
    let is_done = Arc::new(AtomicUsize::new(0));

    // 动态调节控制器
    let controller = tokio::spawn(async move {
        let mut last_downloaded = 0;
        let mut cooldown = 0;

        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;
            
            let current_downloaded = { task_clone.lock().await.downloaded_bytes };
            let speed = current_downloaded - last_downloaded;
            last_downloaded = current_downloaded;

            // 检查是否已下完
            if current_downloaded >= total_size || is_done.load(Ordering::Relaxed) > 0 {
                let _ = tx_clone.send(Chunk::new(999999, 0, 0)).await; // 毒丸退出
                break;
            }

            let mut target = target_workers.load(Ordering::Relaxed);
            let errors = fcontrol_errors.swap(0, Ordering::Relaxed);

            if cooldown > 0 {
                cooldown -= 1;
                continue;
            }

            // AIMD（加性增加，乘性减少）
            if errors > 0 {
                target = (target / 2).max(1);
                println!("[动态调度] ⚠️ 触发限流/报错，减少线程至 {}", target);
                cooldown = 3; // 冷却 6 秒
            } else if speed < 1024 * 100 && target < max_workers {
                target += 1;
                println!("[动态调度] 🐢 速度较慢，缓慢增加线程至 {}", target);
            }

            target_workers.store(target, Ordering::Relaxed);
            
            let mut active = active_workers.load(Ordering::Relaxed);
            while active < target {
                let rx = Arc::clone(&rx_clone);
                let task = Arc::clone(&task_clone);
                let client = client.clone();
                let path = file_path_str.clone();
                let target_ref = Arc::clone(&target_workers);
                let active_ref = Arc::clone(&active_workers);
                let err_ref = Arc::clone(&fcontrol_errors);
                
                tokio::spawn(async move {
                    loop {
                        if active_ref.load(Ordering::Relaxed) > target_ref.load(Ordering::Relaxed) {
                            active_ref.fetch_sub(1, Ordering::Relaxed);
                            break;
                        }

                        let chunk_opt = { rx.lock().await.recv().await };
                        let chunk = match chunk_opt {
                            Some(c) if c.index == 999999 => break,
                            Some(c) => c,
                            None => break,
                        };

                        let mut success = false;
                        for _ in 0..3 {
                            if fetch_chunk(&client, &task, &chunk, &path).await.is_ok() {
                                success = true;
                                break;
                            }
                            tokio::time::sleep(Duration::from_millis(500)).await;
                        }

                        if !success {
                            err_ref.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                });
                active += 1;
                active_workers.store(active, Ordering::Relaxed);
            }
        }
    });

    controller.await?;
    
    {
        let mut t = task.lock().await;
        t.status = TaskStatus::Completed;
    }
    
    Ok(())
}

async fn fetch_chunk(client: &reqwest::Client, task: &Arc<Mutex<Task>>, chunk: &Chunk, path: &str) -> Result<()> {
    let url = { task.lock().await.url.clone() };
    let range_header = format!("bytes={}-{}", chunk.start, chunk.end);
    let res = client.get(&url).header(RANGE, range_header).send().await?;

    if res.status() == reqwest::StatusCode::TOO_MANY_REQUESTS || res.status() == reqwest::StatusCode::FORBIDDEN {
        bail!("触发了服务器限流或防盗链");
    }

    if res.status() != reqwest::StatusCode::PARTIAL_CONTENT {
        bail!("服务器不支持 Range 请求");
    }

    let mut file = OpenOptions::new().write(true).open(path)?;
    file.seek(SeekFrom::Start(chunk.start))?;

    let mut stream = res.bytes_stream();
    while let Some(item) = stream.next().await {
        let data = item?;
        file.write_all(&data)?;
        
        let mut t = task.lock().await;
        t.downloaded_bytes += data.len() as u64;
        if t.chunks[chunk.index].end == chunk.end {
            t.chunks[chunk.index].is_finished = true;
        }
    }
    Ok(())
}