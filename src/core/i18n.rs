use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

pub struct I18n {
    pub current_lang: String,
    pub available_langs: Vec<String>,
    translations: HashMap<String, String>,
    lang_dir: PathBuf,
}

impl I18n {
    pub fn new() -> Self {
        let mut config_dir = dirs::config_dir().unwrap_or_else(|| std::env::current_dir().unwrap());
        config_dir.push("JiTuan");
        if !config_dir.exists() {
            let _ = fs::create_dir_all(&config_dir);
        }

        let mut lang_dir = config_dir.clone();
        lang_dir.push("languages");
        if !lang_dir.exists() {
            let _ = fs::create_dir_all(&lang_dir);
        }

        let mut i18n = Self {
            current_lang: "zh-CN".to_string(),
            available_langs: Vec::new(),
            translations: HashMap::new(),
            lang_dir,
        };

        i18n.ensure_default_files();
        i18n.scan_languages();
        i18n.detect_system_language();
        i18n.load_current_language();

        i18n
    }

    fn ensure_default_files(&self) {
        let zh_path = self.lang_dir.join("zh-CN.json");
        if !zh_path.exists() {
            let zh_content = r#"{
                "app.title": "汲湍 (JiTuan)",
                "app.subtitle": "极致轻量、动态并发的桌面下载器",
                "btn.settings": "设置",
                "btn.check_update": "手动检查",
                "label.download_url": "下载地址",
                "label.save_to": "保存到",
                "label.filename": "文件名",
                "btn.start_download": "开始下载",
                "btn.open_folder": "打开文件夹",
                "btn.select": "选择",
                "status.ready": "准备就绪",
                "status.connecting": "正在连接服务器...",
                "status.downloading": "正在下载",
                "status.completed": "✅ 下载完成！",
                "status.failed": "❌ 下载失败",
                "settings.preferences": "下载偏好",
                "settings.mirror": "启用镜像智能加速",
                "settings.update_channel": "更新通道",
                "settings.alpha": "接收预览版 (Alpha/Beta)",
                "settings.language": "界面语言"
            }"#;
            let _ = fs::write(zh_path, zh_content);
        }

        let en_path = self.lang_dir.join("en-US.json");
        if !en_path.exists() {
            let en_content = r#"{
                "app.title": "JiTuan",
                "app.subtitle": "Ultra-lightweight, dynamic concurrent downloader",
                "btn.settings": "Settings",
                "btn.check_update": "Check Update",
                "label.download_url": "Download URL",
                "label.save_to": "Save to",
                "label.filename": "Filename",
                "btn.start_download": "Start Download",
                "btn.open_folder": "Open Folder",
                "btn.select": "Select",
                "status.ready": "Ready",
                "status.connecting": "Connecting...",
                "status.downloading": "Downloading",
                "status.completed": "✅ Download Completed!",
                "status.failed": "❌ Download Failed",
                "settings.preferences": "Preferences",
                "settings.mirror": "Enable Smart Mirror",
                "settings.update_channel": "Update Channel",
                "settings.alpha": "Receive Alpha/Beta",
                "settings.language": "Interface Language"
            }"#;
            let _ = fs::write(en_path, en_content);
        }
    }

    fn scan_languages(&mut self) {
        if let Ok(entries) = fs::read_dir(&self.lang_dir) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.ends_with(".json") {
                        let lang = name.replace(".json", "");
                        self.available_langs.push(lang);
                    }
                }
            }
        }
    }

    fn detect_system_language(&mut self) {
        if let Some(sys_lang) = sys_locale::get_locale() {
            if self.available_langs.contains(&sys_lang) {
                self.current_lang = sys_lang;
                return;
            }
            let prefix = sys_lang.split('-').next().unwrap_or("");
            for lang in &self.available_langs {
                if lang.starts_with(prefix) {
                    self.current_lang = lang.clone();
                    return;
                }
            }
        }
        if self.available_langs.contains(&"zh-CN".to_string()) {
            self.current_lang = "zh-CN".to_string();
        }
    }

    fn load_current_language(&mut self) {
        self.translations.clear();
        let path = self.lang_dir.join(format!("{}.json", self.current_lang));
        if let Ok(content) = fs::read_to_string(path) {
            if let Ok(dict) = serde_json::from_str::<HashMap<String, String>>(&content) {
                self.translations = dict;
            }
        }
    }

    pub fn t(&self, key: &str) -> String {
        self.translations
            .get(key)
            .cloned()
            .unwrap_or_else(|| key.to_string())
    }
    
    pub fn set_language(&mut self, lang: &str) {
        if self.available_langs.contains(&lang.to_string()) {
            self.current_lang = lang.to_string();
            self.load_current_language();
        }
    }
}