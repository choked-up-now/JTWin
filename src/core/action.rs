use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Deserialize, Clone)]
pub struct ActionRule {
    pub ext: String,
    pub action: String,
    pub app_path: Option<String>,
    pub args: Option<Vec<String>>,
}

pub struct ActionManager {
    rules: Vec<ActionRule>,
}

impl ActionManager {
    pub fn get_config_path() -> PathBuf {
        let mut config_dir = dirs::config_dir().unwrap_or_else(|| std::env::current_dir().unwrap());
        config_dir.push("JiTuan");
        if !config_dir.exists() { let _ = std::fs::create_dir_all(&config_dir); }
        let path = config_dir.join("actions.json");
        
        if !path.exists() {
            let default_json = r#"[
    {
        "ext": "zip",
        "action": "execute",
        "app_path": "C:\\Program Files\\7-Zip\\7z.exe",
        "args": ["x", "{file}", "-o{dir}", "-y"]
    },
    {
        "ext": "rar",
        "action": "open_with",
        "app_path": "C:\\Program Files\\WinRAR\\WinRAR.exe"
    },
    {
        "ext": "mp4",
        "action": "default"
    }
]"#;
            let _ = std::fs::write(&path, default_json);
        }
        path
    }

    pub fn new() -> Self {
        let path = Self::get_config_path();
        let rules = if let Ok(content) = std::fs::read_to_string(&path) {
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            Vec::new()
        };
        Self { rules }
    }

    pub fn trigger_action(&self, file_path: &Path) -> Result<Option<String>, String> {
        let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        
        if let Some(rule) = self.rules.iter().find(|r| r.ext == ext) {
            match rule.action.as_str() {
                "default" => {
                    let _ = opener::open(file_path);
                    return Ok(Some(format!("已调用系统默认程序打开 .{}", ext)));
                }
                "open_with" | "execute" => {
                    if let Some(app) = &rule.app_path {
                        if !Path::new(app).exists() {
                            return Err(format!("找不到程序: {}，请检查 actions.json 配置", app));
                        }
                        let file_str = file_path.to_string_lossy().to_string();
                        let dir_str = file_path.parent().unwrap_or(Path::new(".")).to_string_lossy().to_string();
                        
                        let mut cmd = Command::new(app);
                        if let Some(args) = &rule.args {
                            for arg in args {
                                let processed = arg.replace("{file}", &file_str).replace("{dir}", &dir_str);
                                cmd.arg(processed);
                            }
                        } else {
                            cmd.arg(&file_str);
                        }
                        match cmd.spawn() {
                            Ok(_) => return Ok(Some(format!("已成功触发外部动作 .{}", ext))),
                            Err(e) => return Err(format!("启动失败: {}", e)),
                        }
                    }
                }
                _ => {}
            }
        }
        let _ = opener::open(file_path);
        Ok(None)
    }
}