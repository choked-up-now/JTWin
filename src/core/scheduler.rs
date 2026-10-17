use std::sync::Arc;
use tokio::sync::Semaphore;
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct Scheduler {
    pub global_limit: Arc<Semaphore>,
    pub active_tasks: AtomicUsize,
    pub max_per_task: usize,
}

impl Scheduler {
    pub fn new(global_max: usize, max_per_task: usize) -> Self {
        Self {
            global_limit: Arc::new(Semaphore::new(global_max)),
            active_tasks: AtomicUsize::new(0),
            max_per_task,
        }
    }

    pub fn register_task(&self) {
        self.active_tasks.fetch_add(1, Ordering::SeqCst);
    }

    pub fn unregister_task(&self) {
        self.active_tasks.fetch_sub(1, Ordering::SeqCst);
    }

    pub fn calculate_quota(&self) -> usize {
        let active = self.active_tasks.load(Ordering::SeqCst).max(1);
        let total_permits = 32; // 全局最大并发数
        let quota = (total_permits / active).clamp(1, self.max_per_task);
        quota
    }
}