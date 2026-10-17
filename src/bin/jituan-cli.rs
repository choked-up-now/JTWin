use clap::Parser;
use jituan_core::core::engine;
use jituan_core::core::models::{Task, TaskStatus};
use jituan_core::core::scheduler::Scheduler;
use jituan_core::core::storage;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Parser, Debug)]
#[command(name = "汲湍 (jituan-cli)", version, about = "极致轻量多线程下载器")]
struct Args {
    /// 下载链接
    url: String,
    /// 保存文件名 (可选)
    #[arg(short, long)]
    output: Option<String>,
    /// 初始并发分片数 (默认 16)
    #[arg(short, long, default_value_t = 16)]
    connections: usize,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let file_path = PathBuf::from(args.output.unwrap_or_else(|| "download.bin".to_string()));
    let meta_path = file_path.with_extension("jtdl");

    println!("🌊 汲湍下载引擎启动");
    println!("目标链接: {}", args.url);

    let scheduler = Arc::new(Scheduler::new(32, args.connections));
    let client = reqwest::Client::new();

    let task_arc = if meta_path.exists() {
        println!("发现未完成任务，正在读取断点 (.jtdl)...");
        let t = storage::load_task(&meta_path).await?;
        println!("已恢复进度: {:.2}%", t.progress());
        Arc::new(Mutex::new(t))
    } else {
        let res = client.get(&args.url).header("Range", "bytes=0-0").send().await?;
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

        let t = Task::new("cli_task".to_string(), args.url.clone(), file_path.clone(), total_size, concurrency);
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
        // 跳过已下载完成的分片
        let is_finished = { task_arc.lock().await.chunks[i].is_finished };
        if is_finished { continue; }

        let h = tokio::spawn(engine::download_chunk(
            client.clone(),
            task_arc.clone(),
            i,
            args.url.clone(),
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
            
            // 每秒自动存盘
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