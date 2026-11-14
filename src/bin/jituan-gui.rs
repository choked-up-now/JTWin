#![windows_subsystem = "windows"]

use eframe::egui;
use jituan_core::core::{engine, i18n::I18n, mirror, models::Task, settings, task, updater};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::Mutex;

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
    is_settings_open: bool,
    allow_alpha: bool,
    update_available: Arc<StdMutex<Option<(String, String)>>>,
    is_checking_update: bool,
    has_checked_update: bool,
    i18n: I18n,
    custom_font_path: Option<String>,
}

impl Default for JituanApp {
    fn default() -> Self {
        let i18n = I18n::new();
        let app_settings = settings::load_settings();
        Self {
            url: "https://github.com/choked-up-now/JiTuan-win/releases/download/v0.4.2/JiTuan_v0.4.2_Setup.exe".to_string(),
            save_dir: dirs::download_dir().unwrap_or_else(|| std::env::current_dir().unwrap()).to_string_lossy().to_string(),
            file_name: "JiTuan_v0.4.2_Setup.exe".to_string(),
            status_text: i18n.t("status.ready"),
            progress: Arc::new(StdMutex::new(0.0)),
            is_downloading: false,
            use_mirror: true,
            is_editing_mirrors: false,
            ui_mirrors: Vec::new(),
            is_settings_open: false,
            allow_alpha: false,
            update_available: Arc::new(StdMutex::new(None)),
            is_checking_update: false,
            has_checked_update: false,
            i18n,
            custom_font_path: app_settings.custom_font_path,
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

        // ================== 设置窗口 ==================
        if self.is_settings_open {
            let mut open = true;
            egui::Window::new(format!("⚙️ {}", self.i18n.t("btn.settings")))
                .open(&mut open)
                .resizable(false)
                .collapsible(false)
                .default_width(320.0)
                .show(ctx, |ui| {
                    ui.add_space(5.0);
                    ui.heading(self.i18n.t("settings.preferences"));
                    ui.separator();
                    ui.checkbox(&mut self.use_mirror, self.i18n.t("settings.mirror"));
                    
                    if ui.button("管理镜像规则").clicked() {
                        if mirror::use_internal_editor() {
                            self.ui_mirrors = mirror::get_mirrors();
                            self.is_editing_mirrors = true;
                        } else {
                            let _ = mirror::get_mirrors_raw();
                            let _ = opener::open(mirror::get_config_path());
                        }
                    }

                    if ui.button("管理自动化规则").clicked() {
                        let _ = opener::open(jituan_core::core::action::ActionManager::get_config_path());
                    }

                    ui.add_space(15.0);
                    ui.heading(self.i18n.t("settings.update_channel"));
                    ui.separator();
                    ui.checkbox(&mut self.allow_alpha, self.i18n.t("settings.alpha"));

                    ui.add_space(15.0);
                    ui.heading(self.i18n.t("settings.language"));
                    ui.separator();
                    
                    let mut selected_lang = self.i18n.current_lang.clone();
                    let mut lang_to_set = None; 
                    
                    egui::ComboBox::from_id_source("lang_combo")
                        .selected_text(&selected_lang)
                        .show_ui(ui, |ui| {
                            for lang in &self.i18n.available_langs {
                                if ui.selectable_value(&mut selected_lang, lang.clone(), lang).clicked() {
                                    lang_to_set = Some(lang.clone());
                                }
                            }
                        });

                    if let Some(lang) = lang_to_set {
                        self.i18n.set_language(&lang);
                        self.status_text = self.i18n.t("status.ready");
                        ctx.request_repaint();
                    }

                    // 【核心新增】字体设置
                    ui.add_space(15.0);
                    ui.heading("字体设置 (Font)");
                    ui.separator();
                    ui.label(match &self.custom_font_path {
                        Some(p) => format!("当前字体: {}", p),
                        None => "当前使用系统默认字体".to_string(),
                    });
                    if ui.button("📂 选择自定义字体 (TTF/TTC)").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("Font", &["ttf", "ttc", "otf"])
                            .pick_file()
                        {
                            self.custom_font_path = Some(path.to_string_lossy().to_string());
                            let app_settings = jituan_core::core::settings::AppSettings {
                                custom_font_path: self.custom_font_path.clone(),
                            };
                            let _ = jituan_core::core::settings::save_settings(&app_settings);
                            self.status_text = "字体已更改，重启软件后生效！".to_string();
                        }
                    }
                });
            if !open { self.is_settings_open = false; }
        }

        // ================== 镜像编辑窗口 ==================
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
                                    if ui.button("删除规则").clicked() { to_remove = Some(i); }
                                });
                                ui.horizontal(|ui| {
                                    ui.label("原链接:");
                                    ui.add_sized([450.0, 20.0], egui::TextEdit::singleline(&mut m.target).hint_text("例: https://github.com/*"));
                                });
                                ui.label("镜像站分支 (点击右侧 + 添加):");
                                let mut to_remove_mirror = None;
                                let mut to_add_mirror = false;
                                let mirror_count = m.mirrors.len();
                                for j in 0..mirror_count {
                                    ui.horizontal(|ui| {
                                        ui.add_sized([450.0, 20.0], egui::TextEdit::singleline(&mut m.mirrors[j]).hint_text("例: https://ghproxy.net/https://github.com/*"));
                                        if ui.button("+").clicked() { to_add_mirror = true; }
                                        if mirror_count > 1 && ui.button("-").clicked() { to_remove_mirror = Some(j); }
                                    });
                                }
                                if to_add_mirror { m.mirrors.push(String::new()); }
                                if let Some(j) = to_remove_mirror { m.mirrors.remove(j); }
                            });
                            ui.add_space(5.0);
                        }
                        if let Some(i) = to_remove { self.ui_mirrors.remove(i); }
                    });

                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("添加新规则").clicked() {
                            self.ui_mirrors.push(mirror::MirrorConfig {
                                name: "新规则".to_string(), target: "https://".to_string(), mirrors: vec!["https://".to_string()],
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

        // ================== 主界面 ==================
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading(format!("🌊 {}", self.i18n.t("app.title")));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(format!("⚙️ {}", self.i18n.t("btn.settings"))).clicked() {
                        self.is_settings_open = true;
                    }
                    ui.separator();
                    ui.label(format!("v{}", updater::CURRENT_VERSION));
                    if ui.add_enabled(!self.is_checking_update, egui::Button::new(self.i18n.t("btn.check_update"))).clicked() {
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
                            rt.block_on(async move { let _ = updater::download_and_install(&download_url).await; });
                            ctx_clone.request_repaint();
                        });
                    }
                });
                ui.separator();
            }

            ui.add_space(10.0);
            egui::Grid::new("input_grid").num_columns(3).spacing([10.0, 15.0]).show(ui, |ui| {
                ui.label(self.i18n.t("label.download_url"));
                let url_res = ui.add_sized([360.0, 22.0], egui::TextEdit::singleline(&mut self.url));
                ui.end_row();
                if url_res.changed() {
                    if let Some(filename) = task::extract_filename_from_url(&self.url) { self.file_name = filename; }
                }

                ui.label(self.i18n.t("label.save_to"));
                ui.add_enabled_ui(!self.is_downloading, |ui| {
                    ui.add_sized([320.0, 22.0], egui::TextEdit::singleline(&mut self.save_dir));
                    if ui.button(self.i18n.t("btn.select")).clicked() {
                        let mut dialog = rfd::FileDialog::new();
                        if Path::new(&self.save_dir).exists() { dialog = dialog.set_directory(&self.save_dir); }
                        if let Some(folder) = dialog.pick_folder() { self.save_dir = folder.to_string_lossy().to_string(); }
                    }
                });
                ui.end_row();

                ui.label(self.i18n.t("label.filename"));
                ui.add_enabled_ui(!self.is_downloading, |ui| {
                    ui.add_sized([360.0, 22.0], egui::TextEdit::singleline(&mut self.file_name));
                });
                ui.end_row();
            });

            ui.add_space(15.0);
            ui.horizontal(|ui| {
                let total_width = 240.0;
                ui.add_space((ui.available_width() - total_width) / 2.0);

                ui.add_enabled_ui(!self.is_downloading, |ui| {
                    if ui.add_sized([100.0, 30.0], egui::Button::new(self.i18n.t("btn.start_download"))).clicked() {
                        self.is_downloading = true;
                        self.status_text = self.i18n.t("status.connecting");
                        *self.progress.lock().unwrap() = 0.0; 
                        
                        let raw_url = self.url.clone();
                        
                        if let Err(e) = jituan_core::core::engine::validate_download_url(&raw_url) {
                            self.status_text = format!("❌ {}", e);
                            *self.progress.lock().unwrap() = -1.0;
                            return;
                        }

                        if task::is_p2p_url(&raw_url) {
                            #[cfg(feature = "p2p")]
                            {
                                let dir_str = self.save_dir.clone();
                                let progress_clone = self.progress.clone();
                                let ctx_clone = ctx.clone();
                                std::thread::spawn(move || {
                                    let rt = tokio::runtime::Runtime::new().unwrap();
                                    rt.block_on(async move {
                                        let _ = jituan_core::core::torrent::download_p2p(&raw_url, &dir_str, move |prog, _status| {
                                            *progress_clone.lock().unwrap() = prog;
                                            ctx_clone.request_repaint();
                                        }).await;
                                    });
                                });
                                return;
                            }
                            #[cfg(not(feature = "p2p"))]
                            {
                                self.status_text = "❌ 精简版不支持 P2P 链接".to_string();
                                *self.progress.lock().unwrap() = -1.0;
                                return;
                            }
                        }

                        let use_mirror = self.use_mirror;
                        let file_path = Path::new(&self.save_dir).join(&self.file_name);
                        let file_path_str = file_path.to_string_lossy().to_string();
                        let progress_clone = self.progress.clone();
                        let ctx_clone = ctx.clone();

                        std::thread::spawn(move || {
                            let rt = tokio::runtime::Runtime::new().unwrap();
                            rt.block_on(async move {
                                let final_url = if use_mirror { mirror::get_fastest_mirror(&raw_url).await } else { raw_url };
                                let client = match reqwest::Client::builder().timeout(std::time::Duration::from_secs(15)).user_agent("Mozilla/5.0").build() {
                                    Ok(c) => c, Err(_) => return,
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

                                let total_size = if res.status() == reqwest::StatusCode::PARTIAL_CONTENT {
                                    res.headers().get("content-range").and_then(|v| v.to_str().ok()).and_then(|v| v.split('/').nth(1)).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0)
                                } else if res.status() == reqwest::StatusCode::OK {
                                    res.headers().get("content-length").and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0)
                                } else { 0 };

                                if total_size == 0 { 
                                    *progress_clone.lock().unwrap() = -1.0; ctx_clone.request_repaint(); return; 
                                }

                                let path_buf = PathBuf::from(&file_path_str);
                                if let Ok(file) = std::fs::File::create(&path_buf) { let _ = file.set_len(total_size); } 
                                else { *progress_clone.lock().unwrap() = -1.0; ctx_clone.request_repaint(); return; }
                                
                                let t = Task::new("gui_task".to_string(), final_url.clone(), path_buf.clone(), total_size);
                                let task_arc = Arc::new(Mutex::new(t));

                                let monitor_task = task_arc.clone();
                                let p_clone = progress_clone.clone();
                                let c_clone = ctx_clone.clone();
                                tokio::spawn(async move {
                                    loop {
                                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                                        let t = monitor_task.lock().await;
                                        let prog = (t.downloaded_bytes as f64 / t.total_size as f64) as f32;
                                        let is_done = t.downloaded_bytes >= t.total_size;
                                        drop(t);
                                        *p_clone.lock().unwrap() = prog;
                                        c_clone.request_repaint();
                                        if is_done { break; }
                                    }
                                });

                                let _ = engine::download_task(task_arc, client, 4, 32).await;
                            });
                        });
                    }
                });

                if ui.add_sized([100.0, 30.0], egui::Button::new(self.i18n.t("btn.open_folder"))).clicked() {
                    let _ = opener::open(&self.save_dir);
                }
            });

            ui.add_space(10.0);
            if current_progress < 0.0 {
                ui.colored_label(egui::Color32::RED, self.i18n.t("status.failed"));
            } else if current_progress >= 1.0 {
                ui.horizontal(|ui| {
                    ui.colored_label(egui::Color32::GREEN, self.i18n.t("status.completed"));
                    
                    let file_path = Path::new(&self.save_dir).join(&self.file_name);
                    if file_path.exists() {
                        let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("file").to_uppercase();
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("打开文件夹").clicked() {
                                let _ = opener::open(&self.save_dir);
                            }
                            
                            if ui.button(format!("⚡ 智能执行 .{}", ext)).clicked() {
                                match jituan_core::core::task::handle_post_download(&file_path, true) {
                                    Ok(Some(msg)) => self.status_text = format!("✅ {}", msg),
                                    Ok(None) => self.status_text = "✅ 已调用默认程序".to_string(),
                                    Err(e) => self.status_text = format!("❌ {}", e),
                                }
                            }
                        });
                    }
                });
            } else {
                ui.add(egui::ProgressBar::new(current_progress).show_percentage());
                ui.label(&self.status_text);
            }
        });
        if self.is_downloading { ctx.request_repaint(); }
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([550.0, 400.0]),
        ..Default::default()
    };
    
    eframe::run_native("汲湍", options, Box::new(|cc| {
        let mut fonts = egui::FontDefinitions::default();
        
        // 1. 尝试加载用户自定义字体
        let app_settings = jituan_core::core::settings::load_settings();
        let mut loaded = false;

        if let Some(custom_path) = app_settings.custom_font_path {
            if let Ok(font_data) = std::fs::read(&custom_path) {
                fonts.font_data.insert("custom".to_owned(), egui::FontData::from_owned(font_data));
                fonts.families.get_mut(&egui::FontFamily::Proportional).unwrap().insert(0, "custom".to_owned());
                fonts.families.get_mut(&egui::FontFamily::Monospace).unwrap().insert(0, "custom".to_owned());
                loaded = true;
            }
        }

        // 2. 如果没自定义，加载默认的系统字体
        if !loaded {
            #[cfg(target_os = "windows")]
            let font_path = "C:\\Windows\\Fonts\\msyh.ttc";
            #[cfg(target_os = "macos")]
            let font_path = "/System/Library/Fonts/PingFang.ttc";
            #[cfg(target_os = "linux")]
            let font_path = "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc";

            if let Ok(font_data) = std::fs::read(font_path) {
                fonts.font_data.insert("sys_font".to_owned(), egui::FontData::from_owned(font_data));
                fonts.families.get_mut(&egui::FontFamily::Proportional).unwrap().insert(0, "sys_font".to_owned());
                fonts.families.get_mut(&egui::FontFamily::Monospace).unwrap().insert(0, "sys_font".to_owned());
            }
        }

        cc.egui_ctx.set_fonts(fonts);
        Ok(Box::new(JituanApp::default()))
    }))
}