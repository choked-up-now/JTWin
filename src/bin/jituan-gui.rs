use eframe::egui;
use jituan_core::core::{engine, models::Task, scheduler::Scheduler};
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::Mutex;

struct JituanApp {
    url: String,
    status_text: String,
    progress: Arc<StdMutex<f32>>,
    is_downloading: bool,
}

impl Default for JituanApp {
    fn default() -> Self {
        Self {
            // 换回你最初提供的 AtomGit 链接（已验证支持分片）
            url: "https://atomgit.com/choked-up-now/CiXian/releases/download/v0.12.2/app-arm64-v8a-release.apk".to_string(),
            status_text: "准备就绪".to_string(),
            progress: Arc::new(StdMutex::new(0.0)),
            is_downloading: false,
        }
    }
}

impl eframe::App for JituanApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let current_progress = *self.progress.lock().unwrap();

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("🌊 汲湍 (JiTuan)");
            ui.label("极致轻量、动态并发的桌面下载器");
            ui.separator();

            ui.horizontal(|ui| {
                ui.label("下载链接:");
                ui.add_enabled_ui(!self.is_downloading, |ui| {
                    ui.text_edit_singleline(&mut self.url);
                });
            });

            if ui.button("开始下载").clicked() && !self.is_downloading {
                self.is_downloading = true;
                self.status_text = "正在连接服务器...".to_string();
                *self.progress.lock().unwrap() = 0.0; 
                
                let url = self.url.clone();
                let progress_clone = self.progress.clone();
                let ctx_clone = ctx.clone();

                std::thread::spawn(move || {
                    let rt = tokio::runtime::Runtime::new().unwrap();
                    rt.block_on(async move {
                        let file_path = PathBuf::from("gui_download.apk");
                        
                        // 1. 初始化 Client，加入超时和伪装 UA 防止 403
                        let client = match reqwest::Client::builder()
                            .timeout(std::time::Duration::from_secs(15))
                            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
                            .build() {
                                Ok(c) => c,
                                Err(_) => return,
                            };
                        
                        // 2. 智能探测服务器是否支持分片
                        let res = match client.get(&url).header("Range", "bytes=0-0").send().await {
                            Ok(r) => r,
                            Err(e) => {
                                eprintln!("连接失败: {}", e);
                                *progress_clone.lock().unwrap() = -1.0;
                                ctx_clone.request_repaint();
                                return;
                            }
                        };

                        let (total_size, concurrency) = if res.status() == reqwest::StatusCode::PARTIAL_CONTENT {
                            // 206: 完美支持分片并发
                            let size = res.headers().get("content-range")
                                .and_then(|v| v.to_str().ok()).and_then(|v| v.split('/').nth(1))
                                .and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
                            (size, 16)
                        } else if res.status() == reqwest::StatusCode::OK {
                            // 200: 服务器忽略了 Range，强制返回全量，只能降级单线程
                            let size = res.headers().get("content-length")
                                .and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
                            (size, 1)
                        } else {
                            eprintln!("服务器拒绝连接，状态码: {}", res.status());
                            (0, 1)
                        };

                        if total_size == 0 { 
                            *progress_clone.lock().unwrap() = -1.0;
                            ctx_clone.request_repaint();
                            return; 
                        }

                        // 3. 初始化调度器和下载任务
                        let scheduler = Arc::new(Scheduler::new(32, 16));
                        scheduler.register_task();
                        
                        if let Ok(file) = std::fs::File::create(&file_path) {
                            let _ = file.set_len(total_size);
                        } else {
                            eprintln!("文件创建失败，请检查磁盘权限");
                            *progress_clone.lock().unwrap() = -1.0;
                            ctx_clone.request_repaint();
                            return;
                        }
                        
                        let t = Task::new("gui_task".to_string(), url.clone(), file_path.clone(), total_size, concurrency);
                        let task_arc = Arc::new(Mutex::new(t));
                        
                        // 4. 启动并发下载分片
                        let mut handles = vec![];
                        for i in 0..concurrency {
                            handles.push(tokio::spawn(engine::download_chunk(
                                client.clone(), 
                                task_arc.clone(), 
                                i, 
                                url.clone(), 
                                file_path.to_string_lossy().to_string(), 
                                scheduler.global_limit.clone()
                            )));
                        }
                        
                        // 5. 循环更新 UI 进度
                        loop {
                            let t = task_arc.lock().await;
                            let prog = (t.progress() / 100.0) as f32;
                            drop(t);
                            
                            *progress_clone.lock().unwrap() = prog;
                            ctx_clone.request_repaint(); 
                            
                            if prog >= 1.0 { break; }
                            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                        }
                    });
                });
            }

            // UI 状态渲染
            if current_progress < 0.0 {
                ui.colored_label(egui::Color32::RED, "❌ 下载失败，请检查网络或服务器拒绝访问");
            } else {
                ui.add(egui::ProgressBar::new(current_progress).show_percentage());
                
                if current_progress >= 1.0 {
                    ui.colored_label(egui::Color32::GREEN, "✅ 下载完成！");
                } else {
                    ui.label(&self.status_text);
                }
            }
        });

        if self.is_downloading {
            ctx.request_repaint();
        }
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([450.0, 220.0]),
        ..Default::default()
    };
    
    eframe::run_native(
        "汲湍",
        options,
        Box::new(|cc| {
            // 加载 Windows 系统中文字体
            let mut fonts = egui::FontDefinitions::default();
            if let Ok(font_data) = std::fs::read("C:\\Windows\\Fonts\\msyh.ttc") {
                fonts.font_data.insert("msyh".to_owned(), egui::FontData::from_owned(font_data));
                fonts.families.get_mut(&egui::FontFamily::Proportional).unwrap().insert(0, "msyh".to_owned());
                fonts.families.get_mut(&egui::FontFamily::Monospace).unwrap().insert(0, "msyh".to_owned());
            }
            cc.egui_ctx.set_fonts(fonts);
            Ok(Box::new(JituanApp::default()))
        }),
    )
}