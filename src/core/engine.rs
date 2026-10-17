use crate::core::models::Task;
use anyhow::{Context, Result};
use futures_util::StreamExt;
use reqwest::header::RANGE;
use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};
use std::sync::Arc;
use tokio::sync::{Mutex, Semaphore};

pub async fn download_chunk(
    client: reqwest::Client,
    task: Arc<Mutex<Task>>,
    chunk_index: usize,
    url: String,
    file_path: String,
    semaphore: Arc<Semaphore>,
) -> Result<()> {
    let _permit = semaphore.acquire().await?;

    let (start, end, downloaded) = {
        let t = task.lock().await;
        let c = &t.chunks[chunk_index];
        (c.start, c.end, c.downloaded)
    };

    if start + downloaded > end {
        return Ok(());
    }

    let req_start = start + downloaded;
    let range_header = format!("bytes={}-{}", req_start, end);

    let res = client
        .get(&url)
        .header(RANGE, range_header)
        .send()
        .await
        .context("请求分片失败")?;

    if res.status() != reqwest::StatusCode::PARTIAL_CONTENT {
        anyhow::bail!("服务器不支持 Range 请求，状态码: {}", res.status());
    }

    let mut file = OpenOptions::new()
        .write(true)
        .open(&file_path)
        .context("打开文件失败")?;
    file.seek(SeekFrom::Start(req_start))?;

    let mut stream = res.bytes_stream();
    while let Some(chunk_data) = stream.next().await {
        let data = chunk_data.context("读取网络流失败")?;
        file.write_all(&data)?;

        let mut t = task.lock().await;
        t.chunks[chunk_index].downloaded += data.len() as u64;
        
        if t.chunks[chunk_index].downloaded >= (end - start + 1) {
            t.chunks[chunk_index].is_finished = true;
        }
    }

    Ok(())
}