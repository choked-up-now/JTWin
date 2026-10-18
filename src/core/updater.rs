use anyhow::Result;
use semver::Version;
use serde::Deserialize;
use std::io::Write;

#[derive(Deserialize, Debug)]
struct AtomGitRelease {
    tag_name: String,
    assets: Vec<AtomGitAsset>,
}

#[derive(Deserialize, Debug)]
struct AtomGitAsset {
    name: String,
    browser_download_url: String,
}

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
// AtomGit API 地址
const UPDATE_API_URL: &str = "https://api.atomgit.com/api/v5/repos/choked-up-now/JiTuan/releases";

pub async fn check_update() -> Result<Option<(String, String)>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
        
    let resp = client.get(UPDATE_API_URL)
        .header("User-Agent", "JiTuan-Updater")
        .header("Accept", "application/json")
        .send().await?;

    if !resp.status().is_success() {
        return Ok(None);
    }

    // 解析列表，取最新
    let releases: Vec<AtomGitRelease> = resp.json().await?;
    
    if let Some(latest) = releases.first() {
        let latest_ver_str = latest.tag_name.trim_start_matches('v');
        
        let current_ver = Version::parse(CURRENT_VERSION)?;
        if let Ok(latest_ver) = Version::parse(latest_ver_str) {
            if latest_ver > current_ver {
                for asset in &latest.assets {
                    if asset.name.ends_with(".exe") {
                        return Ok(Some((latest_ver_str.to_string(), asset.browser_download_url.clone())));
                    }
                }
            }
        }
    }
    
    Ok(None)
}

pub async fn download_and_install(url: &str) -> Result<()> {
    let client = reqwest::Client::new();
    let res = client.get(url).send().await?;
    
    let bytes = res.bytes().await?;
    let mut temp = tempfile::Builder::new().prefix("JiTuan_Update_").suffix(".exe").tempfile()?;
    temp.write_all(&bytes)?;
    
    let path = temp.into_temp_path();

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let mut cmd = std::process::Command::new(&path);
        
        // 静默安装，关闭应用，且【不创建桌面快捷方式】
        cmd.args([
            "/SILENT", 
            "/CLOSEAPPLICATIONS", 
            "/SUPPRESSMSGBOXES",
            "/MERGETASKS=\"!desktopicon\""
        ]);
        
        // 屏蔽所有输出，防止黑框
        cmd.stdout(std::process::Stdio::null());
        cmd.stderr(std::process::Stdio::null());
        cmd.stdin(std::process::Stdio::null());

        // CREATE_NO_WINDOW | DETACHED_PROCESS
        cmd.creation_flags(0x08000000 | 0x00000008); 
        let _ = cmd.spawn();
    }

    // 给安装程序留出接管时间
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    
    // 退出当前进程
    std::process::exit(0);
}