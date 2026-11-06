use clap::Parser;
use jituan_core::core::{engine, mirror, models::Task, storage, updater};
#[cfg(feature = "p2p")]
use jituan_core::core::torrent;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Parser, Debug)]
#[command(name = "汲湍 (jituan-cli)", version, about = "极致轻量多线程下载器")]
struct Args {
    url: Option<String>,
    #[arg(short, long)]
    output: Option<String>,
    #[arg(short, long, default_value_t = 8)] // 默认初始 8 线程
    connections: usize,
    #[arg(short, long)]
    update: bool,
    #[arg(long)]
    no_mirror: bool,
    #[arg(long)]
    alpha: bool,
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
    let _save_dir_str = save_dir.to_string_lossy().to_string();

    if raw_url.starts_with("magnet:") || raw_url.ends_with(".torrent") {
        #[cfg(feature = "p2p")]
        {
            println!("🌊 汲湍 P2P 引擎启动");
            let result = torrent::download_p2p(&raw_url, &_save_dir_str, |progress, status| {
                print!("\r[{:.2}%] {}", progress * 100.0, status);
                use std::io::Write; std::io::stdout().flush().unwrap();
            }).await;
            if let Err(e) = result { eprintln!("\n❌ P2P 失败: {}", e); }
            println!(); return Ok(());
        }
        #[cfg(not(feature = "p2p"))]
        { eprintln!("❌ 当前为精简版，不支持 P2P"); return Ok(()); }
    }

    // 【新增拦截】
    if let Err(e) = engine::validate_download_url(&raw_url) {
        eprintln!("❌ {}", e);
        return Ok(());
    }

    let url = if args.no_mirror {
        raw_url.clone()
    } else {
        println!("[镜像] 正在智能测速...");
        mirror::get_fastest_mirror(&raw_url).await
    };

    let file_path = if save_dir.is_dir() { save_dir.join("download.bin") } else { save_dir };
    let meta_path = file_path.with_extension("jtdl");

    println!("🌊 汲湍 HTTP 引擎启动");
    println!("目标链接: {}", url);
    
    let client = reqwest::Client::new();
    let task_arc = if meta_path.exists() {
        println!("发现未完成任务...");
        let t = storage::load_task(&meta_path).await?;
        Arc::new(Mutex::new(t))
    } else {
        let res = client.get(&url).header("Range", "bytes=0-0").send().await?;
        let total_size = res.headers().get("content-range")
            .and_then(|v| v.to_str().ok()).and_then(|v| v.split('/').nth(1))
            .and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
        if total_size == 0 { anyhow::bail!("无法获取文件大小"); }
        let file = std::fs::File::create(&file_path)?; file.set_len(total_size)?;
        
        // 注意这里：Task::new 不再需要传入并发数，统一由引擎控制
        let t = Task::new("cli_task".to_string(), url.clone(), file_path.clone(), total_size);
        storage::save_task(&t).await?;
        Arc::new(Mutex::new(t))
    };

    engine::download_task(task_arc.clone(), client.clone(), 4, args.connections.max(4)).await?;

    println!("\n✅ 下载完成！");
    if meta_path.exists() { let _ = tokio::fs::remove_file(meta_path).await; }
    Ok(())
}