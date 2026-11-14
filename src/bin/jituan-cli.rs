use clap::Parser;
use jituan_core::core::{
    engine, mirror, models::{Task, TaskStatus}, scheduler::Scheduler, storage, task, updater,
};

#[cfg(feature = "p2p")]
use jituan_core::core::torrent;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};
use tokio::sync::Mutex;

#[derive(Parser, Debug)]
#[command(name = "汲湍 (jituan-cli)", version, about = "极致轻量多线程下载器")]
struct Args {
    url: Option<String>,
    #[arg(short, long)]
    output: Option<String>,
    #[arg(short, long, default_value_t = 8)]
    connections: usize,
    #[arg(short, long)]
    update: bool,
    #[arg(long)]
    no_mirror: bool,
    #[arg(long)]
    alpha: bool,
    /// 下载完成后根据 actions.json 规则自动执行后续操作（如自动解压）
    #[arg(long)]
    action: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    if args.update {
        println!("正在检查更新...");
        match updater::check_update(args.alpha).await {
            Ok(Some((new_ver, url))) => {
                println!("发现新版本 v{}，正在下载安装包...", new_ver);
                updater::download_and_install(&url).await?;
                std::process::exit(0);
            }
            Ok(None) => { println!("当前已是最新版本 (v{})。", updater::CURRENT_VERSION); return Ok(()); }
            Err(e) => { eprintln!("检查更新失败: {}", e); return Ok(()); }
        }
    }

    let raw_url = match args.url {
        Some(u) => u,
        None => { eprintln!("错误：请提供下载链接"); return Ok(()); }
    };

    let save_dir = match args.output {
        Some(path) => PathBuf::from(path.trim_matches('"')),
        None => dirs::download_dir().unwrap_or_else(|| std::env::current_dir().unwrap()),
    };
    let save_dir_str = save_dir.to_string_lossy().to_string();

    // ==========================================
    // 分流 1：P2P 下载
    // ==========================================
    if task::is_p2p_url(&raw_url) {
        #[cfg(feature = "p2p")]
        {
            println!("🌊 汲湍 P2P 引擎启动");
            let last_pct = Arc::new(AtomicI32::new(-1));
            let last_pct_clone = last_pct.clone();
            
            let result = torrent::download_p2p(&raw_url, &save_dir_str, move |progress, status| {
                let pct = (progress * 100.0) as i32;
                let last = last_pct_clone.load(Ordering::Relaxed);
                if pct > last || pct == 0 {
                    println!("[{}%] {}", pct, status);
                    last_pct_clone.store(pct, Ordering::Relaxed);
                }
            }).await;

            if let Err(e) = result {
                eprintln!("\n❌ P2P 失败: {}", e);
            } else if args.action {
                if let Some(file_name) = task::extract_filename_from_url(&raw_url) {
                    let file_path = save_dir.join(file_name);
                    if let Ok(Some(msg)) = task::handle_post_download(&file_path, true) {
                        println!("[系统] {}", msg);
                    }
                }
            }
            println!(); 
            return Ok(());
        }
        #[cfg(not(feature = "p2p"))]
        { eprintln!("❌ 当前为精简版，不支持 P2P"); return Ok(()); }
    }

    // ==========================================
    // 分流 2：HTTP 下载
    // ==========================================
    if let Err(e) = engine::validate_download_url(&raw_url) {
        eprintln!("❌ {}", e);
        return Ok(());
    }

    let url = if args.no_mirror { raw_url.clone() } else { mirror::get_fastest_mirror(&raw_url).await };

    let file_path = if save_dir.is_dir() {
        save_dir.join(task::extract_filename_from_url(&url).unwrap_or_else(|| "download.bin".to_string()))
    } else {
        save_dir
    };
    let meta_path = file_path.with_extension("jtdl");

    println!("🌊 汲湍 HTTP 引擎启动");
    println!("目标链接: {}", url);
    println!("保存至: {:?}", file_path);

    let scheduler = Arc::new(Scheduler::new(32, args.connections));
    let client = reqwest::Client::new();

    let task_arc = if meta_path.exists() {
        println!("发现未完成任务，正在读取断点 (.jtdl)...");
        let t = storage::load_task(&meta_path).await?;
        println!("已恢复进度: {:.2}%", t.progress());
        Arc::new(Mutex::new(t))
    } else {
        let res = client.get(&url).header("Range", "bytes=0-0").send().await?;
        let total_size = res.headers().get("content-range")
            .and_then(|v| v.to_str().ok()).and_then(|v| v.split('/').nth(1))
            .and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
        
        if total_size == 0 { anyhow::bail!("无法获取文件大小"); }
        
        scheduler.register_task();
        let concurrency = scheduler.calculate_quota();
        println!("文件大小: {:.2} MB, 分配并发数: {}", total_size as f64 / 1024.0 / 1024.0, concurrency);

        let file = std::fs::File::create(&file_path)?;
        file.set_len(total_size)?;

        let t = Task::new("cli_task".to_string(), url.clone(), file_path.clone(), total_size);
        storage::save_task(&t).await?;
        Arc::new(Mutex::new(t))
    };

    {
        let mut t = task_arc.lock().await;
        t.status = TaskStatus::Downloading;
    }

    let task_clone = task_arc.clone();
    let monitor = tokio::spawn(async move {
        loop {
            let t = task_clone.lock().await;
            print!("\r进度: {:.2}% ({:.2}/{:.2} MB)", 
                t.progress(), 
                t.downloaded_bytes as f64/1024.0/1024.0, 
                t.total_size as f64/1024.0/1024.0
            );
            use std::io::Write; std::io::stdout().flush().unwrap();
            if t.downloaded_bytes >= t.total_size { 
                println!("\n✅ 下载完成！"); 
                break; 
            }
            let _ = storage::save_task(&t).await;
            drop(t);
            tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
        }
    });

    engine::download_task(task_arc, client, 4, args.connections.max(4)).await?;

    let _ = monitor.await;
    if meta_path.exists() { let _ = tokio::fs::remove_file(meta_path).await; }

    // 动作执行
    if let Ok(Some(msg)) = task::handle_post_download(&file_path, args.action) {
        println!("[系统] {}", msg);
    }
    
    Ok(())
}