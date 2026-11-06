use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Chunk {
    pub index: usize,
    pub start: u64,
    pub end: u64,
    pub is_finished: bool,
}

impl Chunk {
    pub fn new(index: usize, start: u64, end: u64) -> Self {
        Self { index, start, end, is_finished: false }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum TaskStatus {
    Pending,
    Downloading,
    Paused,
    Completed,
    Failed,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Task {
    pub id: String,
    pub url: String,
    pub file_path: PathBuf,
    pub total_size: u64,
    pub chunks: Vec<Chunk>,
    pub status: TaskStatus,
    pub downloaded_bytes: u64, // 记录任务总下载量，用于动态测速
}

impl Task {
    pub fn new(id: String, url: String, file_path: PathBuf, total_size: u64) -> Self {
        let mut chunks = Vec::new();
        // 采用 4MB 的微块切分，替代原来的按线程数均分
        let micro_chunk_size = 4 * 1024 * 1024; 
        let mut current_start = 0;
        let mut index = 0;

        while current_start < total_size {
            let end = (current_start + micro_chunk_size - 1).min(total_size - 1);
            chunks.push(Chunk::new(index, current_start, end));
            current_start = end + 1;
            index += 1;
        }

        Self {
            id,
            url,
            file_path,
            total_size,
            chunks,
            status: TaskStatus::Pending,
            downloaded_bytes: 0,
        }
    }

    pub fn progress(&self) -> f64 {
        if self.total_size == 0 {
            0.0
        } else {
            (self.downloaded_bytes as f64 / self.total_size as f64) * 100.0
        }
    }
}