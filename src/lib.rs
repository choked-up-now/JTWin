pub mod core {
    pub mod engine;
    pub mod models;
    pub mod scheduler;
    pub mod storage;
    pub mod updater;
    pub mod mirror;
    
    #[cfg(feature = "p2p")]
    pub mod torrent;
    // 移除了 tracker 和 mse，这部分已经交给 librqbit 处理了
}