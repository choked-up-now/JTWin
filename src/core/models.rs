use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Chunk {
    pub index: usize,
    pub start: u64,
    pub end: u64,
    pub downloaded: u64,
    pub is_finished: bool,
}

impl Chunk {
    pub fn new(index: usize, start: u64, end: u64) -> Self {
        Self {
            index,
            start,
            end,
            downloaded: 0,
            is_finished: false,
        }
    }

    #[allow(dead_code)]
    pub fn remaining(&self) -> u64 {
        if self.is_finished {
            0
        } else {
            (self.end - self.start + 1) - self.downloaded
        }
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
}

impl Task {
    pub fn new(id: String, url: String, file_path: PathBuf, total_size: u64, concurrency: usize) -> Self {
        let mut chunks = Vec::new();
        let chunk_size = total_size / concurrency as u64;

        for i in 0..concurrency {
            let start = i as u64 * chunk_size;
            let end = if i == concurrency - 1 {
                total_size - 1
            } else {
                start + chunk_size - 1
            };
            chunks.push(Chunk::new(i, start, end));
        }

        Self {
            id,
            url,
            file_path,
            total_size,
            chunks,
            status: TaskStatus::Pending,
        }
    }

    pub fn downloaded_bytes(&self) -> u64 {
        self.chunks.iter().map(|c| c.downloaded).sum()
    }

    pub fn progress(&self) -> f64 {
        if self.total_size == 0 {
            0.0
        } else {
            (self.downloaded_bytes() as f64 / self.total_size as f64) * 100.0
        }
    }
}