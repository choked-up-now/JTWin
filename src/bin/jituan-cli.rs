use clap::Parser;
use jituan_core::core::{engine, models::{Task, TaskStatus}, scheduler::Scheduler, storage, updater};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Parser, Debug)]
#[command(name = "汲湍 (jituan-cli)", version, about = "极致轻量多线程下载器")]
struct Args {
    /// 下载链接 (使用 --update 时可省略)
    url: Option<String>, 
    
    /// 保存文件名 (可选)
    #[arg(short, long)]
    output: Option<String>,
    
    /// 初始并发分片数 (默认 16)
    #[arg(short, long, default_value_t = 16)]
    connections: usize,

    /// 检查并更新到最新版本
    #[arg(short, long)]
    update: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // 1. 如果用户明确要求更新
    if args.update {
        println!("正在检查更新...");
        match updater::check_update().await {
            Ok(Some((new_ver, url))) => {
                println!("发现新版本 v{}，正在下载安装包...", new_ver);
                updater::download_and_install(&url).await?;
                println!("更新程序已启动，当前进程即将退出。");
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

    // 2. 如果没有提供 URL，且没要求更新，提示用法
    let url = match args.url {
        Some(u) => u,
        None => {
            eprintln!("错误：请提供下载链接，或使用 --update 检查更新。");
            return Ok(());
        }
    };

    // 3. 后台自动版本检查（不阻塞主流程）
    let _url_clone = url.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        if let Ok(Some((new_ver, _))) = updater::check_update().await {
            // 用黄色（终端转义字符）打印提示
            println!("\n\x1b[33m[提示] 发现汲湍新版本 v{}，可使用 `jituan-cli --update` 升级。\x1b[0m", new_ver);
        }
    });

    // 4. 正常下载逻辑
    let file_path = PathBuf::from(args.output.unwrap_or_else(|| "download.bin".to_string()));
    let meta_path = file_path.with_extension("jtdl");

    println!("🌊 汲湍下载引擎启动");
    println!("目标链接: {}", url);

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
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split('/').nth(1))
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);
        
        if total_size == 0 { anyhow::bail!("无法获取文件大小"); }
        
        scheduler.register_task();
        let concurrency = scheduler.calculate_quota();
        println!("文件大小: {:.2} MB, 分配并发数: {}", total_size as f64 / 1024.0 / 1024.0, concurrency);

        let file = std::fs::File::create(&file_path)?;
        file.set_len(total_size)?;

        let t = Task::new("cli_task".to_string(), url.clone(), file_path.clone(), total_size, concurrency);
        storage::save_task(&t).await?;
        Arc::new(Mutex::new(t))
    };

    {
        let mut t = task_arc.lock().await;
        t.status = TaskStatus::Downloading;
    }

    let mut handles = vec![];
    let chunk_count = { task_arc.lock().await.chunks.len() };
    
    for i in 0..chunk_count {
        let is_finished = { task_arc.lock().await.chunks[i].is_finished };
        if is_finished { continue; }

        let h = tokio::spawn(engine::download_chunk(
            client.clone(),
            task_arc.clone(),
            i,
            url.clone(),
            file_path.to_string_lossy().to_string(),
            scheduler.global_limit.clone(),
        ));
        handles.push(h);
    }

    // 实时进度监控与存盘
    let task_clone = task_arc.clone();
    let monitor = tokio::spawn(async move {
        loop {
            let t = task_clone.lock().await;
            print!("\r进度: {:.2}% ({:.2}/{:.2} MB)", 
                t.progress(), 
                t.downloaded_bytes() as f64/1024.0/1024.0, 
                t.total_size as f64/1024.0/1024.0
            );
            use std::io::Write; std::io::stdout().flush().unwrap();
            
            if t.downloaded_bytes() >= t.total_size { 
                println!("\n✅ 下载完成！"); 
                break; 
            }
            
            let _ = storage::save_task(&t).await;
            
            drop(t);
            tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
        }
    });

    for h in handles { h.await??; }
    let _ = monitor.await;
    if meta_path.exists() { let _ = tokio::fs::remove_file(meta_path).await; }
    
    Ok(())
}