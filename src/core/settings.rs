use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct AppSettings {
    pub custom_font_path: Option<String>,
}

pub fn get_settings_path() -> PathBuf {
    let mut dir = dirs::config_dir().unwrap_or_else(|| std::env::current_dir().unwrap());
    dir.push("JiTuan");
    if !dir.exists() {
        let _ = fs::create_dir_all(&dir);
    }
    dir.join("settings.json")
}

pub fn load_settings() -> AppSettings {
    let path = get_settings_path();
    if let Ok(content) = fs::read_to_string(path) {
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        AppSettings::default()
    }
}

pub fn save_settings(settings: &AppSettings) -> Result<()> {
    let path = get_settings_path();
    let json = serde_json::to_string_pretty(settings)?;
    fs::write(path, json)?;
    Ok(())
}