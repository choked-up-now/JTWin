use clap::Parser;
use jituan_core::core::{engine, models::{Task, TaskStatus}, scheduler::Scheduler, storage, updater, mirror, torrent}; // 引入 torrent
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Parser, Debug)]
#[command(name = "汲湍 (jituan-cli)", version, about = "极致轻量多线程下载器 (支持 HTTP 与 P2P)")]
struct Args {
    url: Option<String>, 
    #[arg(short, long)]
    output: Option<String>,
    #[arg(short, long, default_value_t = 16)]
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
        // ... 更新逻辑保持不变 ...
        match updater::check_update(args.alpha).await {
            Ok(Some((new_ver, url))) => {
                updater::download_and_install(&url).await?;
                std::process::exit(0);
            }
            Ok(None) => {
                println!("当前已是最新版本 (v{})。", updater::CURRENT_VERSION);
                return Ok(());
            }
            Err(e) => {
                eprintln!("检查更新失败: {}", e);
                return Ok(());
            }
        }
    }

    let raw_url = match args.url {
        Some(u) => u,
        None => {
            eprintln!("错误：请提供下载链接，或使用 --update 检查更新。");
            return Ok(());
        }
    };

    let save_dir = match args.output {
        Some(path) => PathBuf::from(path),
        None => dirs::download_dir().unwrap_or_else(|| std::env::current_dir().unwrap()),
    };
    let save_dir_str = save_dir.to_string_lossy().to_string();

    // 【核心新增】P2P 链接分流
    if raw_url.starts_with("magnet:") || raw_url.ends_with(".torrent") {
        println!("🌊 汲湍 P2P 引擎启动");
        println!("目标链接: {}", raw_url);
        
        let result = torrent::download_p2p(&raw_url, &save_dir_str, |progress, status| {
            print!("\r[{:.2}%] {}", progress * 100.0, status);
            use std::io::Write; std::io::stdout().flush().unwrap();
        }).await;

        if let Err(e) = result {
            eprintln!("\n❌ P2P 失败: {}", e);
        }
        return Ok(());
    }

    // 原有的 HTTP 逻辑
    let url = if args.no_mirror {
        raw_url.clone()
    } else {
        let best = mirror::get_fastest_mirror(&raw_url).await;
        best
    };

    let allow_alpha = args.alpha;
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        if let Ok(Some((new_ver, _))) = updater::check_update(allow_alpha).await {
            println!("\n\x1b[33m[提示] 发现汲湍新版本 v{}，可使用 `jituan-cli --update` 升级。\x1b[0m", new_ver);
        }
    });

    let file_path = if save_dir.is_dir() {
        save_dir.join("download.bin")
    } else {
        save_dir
    };
    let meta_path = file_path.with_extension("jtdl");

    println!("🌊 汲湍 HTTP 引擎启动");
    // ... 后续 HTTP 下载逻辑保持不变 ...
    Ok(())
}