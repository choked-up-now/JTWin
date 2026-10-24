#![windows_subsystem = "windows"]

use eframe::egui;
use jituan_core::core::{engine, models::Task, scheduler::Scheduler, updater, mirror};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::Mutex;

fn extract_filename_from_url(url: &str) -> Option<String> {
    let path = url.split('?').next()?;
    let parts: Vec<&str> = path.split('/').collect();
    let last = parts.last()?;
    if !last.is_empty() && last.contains('.') {
        Some(last.to_string())
    } else {
        None
    }
}

struct JituanApp {
    url: String,
    save_dir: String,
    file_name: String,
    status_text: String,
    progress: Arc<StdMutex<f32>>,
    is_downloading: bool,
    use_mirror: bool,
    is_editing_mirrors: bool,
    ui_mirrors: Vec<mirror::MirrorConfig>,
    allow_alpha: bool,
    update_available: Arc<StdMutex<Option<(String, String)>>>,
    is_checking_update: bool,
    has_checked_update: bool,
}

impl Default for JituanApp {
    fn default() -> Self {
        Self {
            url: "https://github.com/choked-up-now/JiTuan-win/releases/download/v0.2.6/JiTuan_v0.2.6_Setup.exe".to_string(),
            save_dir: dirs::download_dir().unwrap_or_else(|| std::env::current_dir().unwrap()).to_string_lossy().to_string(),
            file_name: "JiTuan_v0.2.6_Setup.exe".to_string(),
            status_text: "准备就绪".to_string(),
            progress: Arc::new(StdMutex::new(0.0)),
            is_downloading: false,
            use_mirror: true,
            is_editing_mirrors: false,
            ui_mirrors: Vec::new(),
            allow_alpha: false,
            update_available: Arc::new(StdMutex::new(None)),
            is_checking_update: false,
            has_checked_update: false,
        }
    }
}

impl eframe::App for JituanApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let current_progress = *self.progress.lock().unwrap();
        let update_opt = self.update_available.lock().unwrap().clone();

        if !self.has_checked_update {
            self.has_checked_update = true;
            let ctx_clone = ctx.clone();
            let update_state = self.update_available.clone();
            let allow_alpha = self.allow_alpha;
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async move {
                    if let Ok(Some((new_ver, url))) = updater::check_update(allow_alpha).await {
                        *update_state.lock().unwrap() = Some((new_ver, url));
                        ctx_clone.request_repaint();
                    }
                });
            });
        }

        if self.is_editing_mirrors {
            let mut open = true;
            egui::Window::new("智能镜像规则管理")
                .open(&mut open)
                .resizable(true)
                .default_size([600.0, 500.0])
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label("说明：不固定位置请用星号 * 占位。下载时会并发测速并选择最快的源。");
                    ui.separator();
                    
                    egui::ScrollArea::vertical().max_height(380.0).show(ui, |ui| {
                        let mut to_remove = None;
                        for (i, m) in self.ui_mirrors.iter_mut().enumerate() {
                            ui.group(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(format!("序号: {}", i + 1));
                                    ui.label("名称:");
                                    ui.text_edit_singleline(&mut m.name);
                                    if ui.button("删除规则").clicked() {
                                        to_remove = Some(i);
                                    }
                                });
                                
                                ui.horizontal(|ui| {
                                    ui.label("原链接:");
                                    ui.add_sized([450.0, 20.0], egui::TextEdit::singleline(&mut m.target).hint_text("例: https://github.com/*"));
                                });

                                ui.label("镜像站分支 (点击右侧 + 添加):");
                                let mut to_remove_mirror = None;
                                let mut to_add_mirror = false;
                                let mirror_count = m.mirrors.len();
                                
                                // 使用索引遍历，避免 Borrow Checker 报错
                                for j in 0..mirror_count {
                                    ui.horizontal(|ui| {
                                        ui.add_sized([450.0, 20.0], egui::TextEdit::singleline(&mut m.mirrors[j]).hint_text("例: https://ghproxy.net/https://github.com/*"));
                                        if ui.button("+").clicked() {
                                            to_add_mirror = true;
                                        }
                                        if mirror_count > 1 && ui.button("-").clicked() {
                                            to_remove_mirror = Some(j);
                                        }
                                    });
                                }
                                
                                // 循环结束后再统一执行增删
                                if to_add_mirror {
                                    m.mirrors.push(String::new());
                                }
                                if let Some(j) = to_remove_mirror {
                                    m.mirrors.remove(j);
                                }
                            });
                            ui.add_space(5.0);
                        }
                        if let Some(i) = to_remove {
                            self.ui_mirrors.remove(i);
                        }
                    });

                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("添加新规则").clicked() {
                            self.ui_mirrors.push(mirror::MirrorConfig {
                                name: "新规则".to_string(),
                                target: "https://".to_string(),
                                mirrors: vec!["https://".to_string()],
                            });
                        }
                        
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("保存并生效").clicked() {
                                if let Ok(json) = serde_json::to_string_pretty(&self.ui_mirrors) {
                                    if mirror::save_mirrors(&json).is_ok() {
                                        self.is_editing_mirrors = false;
                                        self.status_text = "镜像规则已保存".to_string();
                                    }
                                }
                            }
                        });
                    });
                });
            if !open { self.is_editing_mirrors = false; }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("🌊 汲湍 (JiTuan)");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format!("v{}", updater::CURRENT_VERSION));
                    
                    if ui.add_enabled(!self.is_checking_update, egui::Button::new("手动检查")).clicked() {
                        self.is_checking_update = true;
                        let ctx_clone = ctx.clone();
                        let update_state = self.update_available.clone();
                        let allow_alpha = self.allow_alpha;
                        std::thread::spawn(move || {
                            let rt = tokio::runtime::Runtime::new().unwrap();
                            rt.block_on(async move {
                                if let Ok(Some((new_ver, url))) = updater::check_update(allow_alpha).await {
                                    *update_state.lock().unwrap() = Some((new_ver, url));
                                }
                                ctx_clone.request_repaint();
                            });
                        });
                    }
                    
                    ui.checkbox(&mut self.allow_alpha, "接收预览版(Alpha)");
                });
            });
            
            ui.separator();

            if let Some((new_ver, download_url)) = update_opt {
                ui.horizontal(|ui| {
                    ui.colored_label(egui::Color32::YELLOW, format!("🎉 发现新版本 v{}", new_ver));
                    if ui.button("立即更新并重启").clicked() {
                        self.status_text = "正在下载更新包...".to_string();
                        let ctx_clone = ctx.clone();
                        std::thread::spawn(move || {
                            let rt = tokio::runtime::Runtime::new().unwrap();
                            rt.block_on(async move {
                                let _ = updater::download_and_install(&download_url).await;
                            });
                            ctx_clone.request_repaint();
                        });
                    }
                });
                ui.separator();
            }

            ui.add_space(10.0);
            egui::Grid::new("input_grid")
                .num_columns(3)
                .spacing([10.0, 15.0])
                .show(ui, |ui| {
                    ui.label("下载地址");
                    let url_res = ui.add_sized([360.0, 22.0], egui::TextEdit::singleline(&mut self.url));
                    ui.end_row();
                    if url_res.changed() {
                        if let Some(filename) = extract_filename_from_url(&self.url) {
                            self.file_name = filename;
                        }
                    }

                    ui.label("保存到");
                    ui.add_enabled_ui(!self.is_downloading, |ui| {
                        ui.add_sized([320.0, 22.0], egui::TextEdit::singleline(&mut self.save_dir));
                        if ui.button("选择").clicked() {
                            let mut dialog = rfd::FileDialog::new();
                            if Path::new(&self.save_dir).exists() {
                                dialog = dialog.set_directory(&self.save_dir);
                            }
                            if let Some(folder) = dialog.pick_folder() {
                                self.save_dir = folder.to_string_lossy().to_string();
                            }
                        }
                    });
                    ui.end_row();

                    ui.label("文件名");
                    ui.add_enabled_ui(!self.is_downloading, |ui| {
                        ui.add_sized([360.0, 22.0], egui::TextEdit::singleline(&mut self.file_name));
                    });
                    ui.end_row();
                });

            ui.horizontal(|ui| {
                ui.checkbox(&mut self.use_mirror, "启用镜像智能加速");
                
                if ui.button("管理镜像规则").clicked() {
                    if mirror::use_internal_editor() {
                        self.ui_mirrors = mirror::get_mirrors();
                        self.is_editing_mirrors = true;
                    } else {
                        let _ = mirror::get_mirrors_raw();
                        let _ = opener::open(mirror::get_config_path());
                    }
                }
            });

            ui.add_space(15.0);
            ui.horizontal(|ui| {
                let total_width = 240.0;
                let available_width = ui.available_width();
                ui.add_space((available_width - total_width) / 2.0);

                ui.add_enabled_ui(!self.is_downloading, |ui| {
                    if ui.add_sized([100.0, 30.0], egui::Button::new("开始下载")).clicked() {
                        self.is_downloading = true;
                        self.status_text = "正在智能测速选择最优镜像...".to_string();
                        *self.progress.lock().unwrap() = 0.0; 
                        
                        let raw_url = self.url.clone();
                        let use_mirror = self.use_mirror;
                        let file_path = Path::new(&self.save_dir).join(&self.file_name);
                        let file_path_str = file_path.to_string_lossy().to_string();
                        
                        let progress_clone = self.progress.clone();
                        let ctx_clone = ctx.clone();

                        std::thread::spawn(move || {
                            let rt = tokio::runtime::Runtime::new().unwrap();
                            rt.block_on(async move {
                                let final_url = if use_mirror {
                                    mirror::get_fastest_mirror(&raw_url).await
                                } else {
                                    raw_url
                                };

                                let client = match reqwest::Client::builder()
                                    .timeout(std::time::Duration::from_secs(15))
                                    .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
                                    .build() {
                                        Ok(c) => c,
                                        Err(_) => return,
                                    };
                                
                                let res = match client.get(&final_url).header("Range", "bytes=0-0").send().await {
                                    Ok(r) => r,
                                    Err(e) => {
                                        eprintln!("连接失败: {}", e);
                                        *progress_clone.lock().unwrap() = -1.0;
                                        ctx_clone.request_repaint();
                                        return;
                                    }
                                };

                                let (total_size, concurrency) = if res.status() == reqwest::StatusCode::PARTIAL_CONTENT {
                                    let size = res.headers().get("content-range")
                                        .and_then(|v| v.to_str().ok()).and_then(|v| v.split('/').nth(1))
                                        .and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
                                    (size, 16)
                                } else if res.status() == reqwest::StatusCode::OK {
                                    let size = res.headers().get("content-length")
                                        .and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
                                    (size, 1)
                                } else {
                                    (0, 1)
                                };

                                if total_size == 0 { 
                                    *progress_clone.lock().unwrap() = -1.0;
                                    ctx_clone.request_repaint();
                                    return; 
                                }

                                let scheduler = Arc::new(Scheduler::new(32, 16));
                                scheduler.register_task();
                                
                                let path_buf = PathBuf::from(&file_path_str);
                                if let Ok(file) = std::fs::File::create(&path_buf) {
                                    let _ = file.set_len(total_size);
                                } else {
                                    *progress_clone.lock().unwrap() = -1.0;
                                    ctx_clone.request_repaint();
                                    return;
                                }
                                
                                let t = Task::new("gui_task".to_string(), final_url.clone(), path_buf.clone(), total_size, concurrency);
                                let task_arc = Arc::new(Mutex::new(t));
                                
                                let mut handles = vec![];
                                for i in 0..concurrency {
                                    handles.push(tokio::spawn(engine::download_chunk(
                                        client.clone(), 
                                        task_arc.clone(), 
                                        i, 
                                        final_url.clone(), 
                                        file_path_str.clone(), 
                                        scheduler.global_limit.clone()
                                    )));
                                }
                                
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
                });

                if ui.add_sized([100.0, 30.0], egui::Button::new("打开文件夹")).clicked() {
                    let _ = opener::open(&self.save_dir);
                }
            });

            ui.add_space(10.0);
            if current_progress < 0.0 {
                ui.colored_label(egui::Color32::RED, "❌ 下载失败，请检查网络或配置");
            } else if current_progress >= 1.0 {
                ui.colored_label(egui::Color32::GREEN, "✅ 下载完成！");
            } else {
                ui.add(egui::ProgressBar::new(current_progress).show_percentage());
                ui.label(&self.status_text);
            }
        });

        if self.is_downloading {
            ctx.request_repaint();
        }
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([550.0, 400.0]),
        ..Default::default()
    };
    
    eframe::run_native(
        "汲湍",
        options,
        Box::new(|cc| {
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