use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MirrorConfig {
    pub name: String,
    pub target: String,
    pub mirrors: Vec<String>,
}

fn get_config_dir() -> PathBuf {
    let mut path = dirs::config_dir().unwrap_or_else(|| std::env::current_dir().unwrap());
    path.push("JiTuan");
    if !path.exists() {
        let _ = fs::create_dir_all(&path);
    }
    path
}

pub fn get_config_path() -> PathBuf {
    let mut path = get_config_dir();
    path.push("mirrors.json");
    path
}

pub fn use_internal_editor() -> bool {
    let mut path = get_config_dir();
    path.push("use_internal_editor.flag");
    path.exists()
}

pub fn get_mirrors_raw() -> String {
    let path = get_config_path();
    if path.exists() {
        fs::read_to_string(&path).unwrap_or_default()
    } else {
        let default_rules = include_str!("../../mirrors.json");
        let _ = fs::write(&path, default_rules);
        default_rules.to_string()
    }
}

pub fn get_mirrors() -> Vec<MirrorConfig> {
    let content = get_mirrors_raw();
    serde_json::from_str::<Vec<MirrorConfig>>(&content).unwrap_or_default()
}

pub fn save_mirrors(content: &str) -> Result<(), std::io::Error> {
    let path = get_config_path();
    fs::write(path, content)
}

pub async fn get_fastest_mirror(url: &str) -> String {
    let rules = get_mirrors();
    let mut candidates = vec![url.to_string()];

    for rule in rules {
        let pattern = rule.target.replace("*", "(.*)");
        if let Ok(re) = Regex::new(&format!("^{}$", pattern)) {
            if let Some(caps) = re.captures(url) {
                let dynamic_part = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                
                for m in &rule.mirrors {
                    let mirror_url = m.replace("*", dynamic_part);
                    if !candidates.contains(&mirror_url) {
                        candidates.push(mirror_url);
                    }
                }
                break;
            }
        }
    }

    if candidates.len() == 1 {
        return candidates[0].clone();
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
        .build()
        .unwrap_or_default();

    let mut handles = Vec::new();
    for cand in &candidates {
        let client = client.clone();
        let cand = cand.clone();
        handles.push(tokio::spawn(async move {
            let start = std::time::Instant::now();
            let _ = client.head(&cand).send().await;
            (cand, start.elapsed())
        }));
    }

    for handle in handles {
        if let Ok((cand, _duration)) = handle.await {
            return cand;
        }
    }

    candidates[0].clone()
}