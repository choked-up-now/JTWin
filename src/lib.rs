pub mod core {
    pub mod engine;
    pub mod models;
    pub mod scheduler;
    pub mod storage;
    pub mod updater;
    pub mod mirror;
    pub mod i18n;
    pub mod action;
    pub mod task;
    pub mod settings; // 新增
    
    #[cfg(feature = "p2p")]
    pub mod torrent;
}