use crate::core::models::Task;
use anyhow::{Context, Result};
use std::path::Path;

pub async fn save_task(task: &Task) -> Result<()> {
    // 采用 .jtdl 专属后缀
    let meta_path = task.file_path.with_extension("jtdl"); 
    let json = serde_json::to_string_pretty(task)
        .context("序列化任务失败")?;
    tokio::fs::write(&meta_path, json)
        .await
        .context("写入 .jtdl 文件失败")?;
    Ok(())
}

pub async fn load_task(meta_path: &Path) -> Result<Task> {
    let json = tokio::fs::read_to_string(meta_path)
        .await
        .context("读取 .jtdl 文件失败")?;
    let task: Task = serde_json::from_str(&json)
        .context("解析 .jtdl 文件失败")?;
    Ok(task)
}