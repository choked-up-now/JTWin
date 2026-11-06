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
// 【已修改】仓库名为 JTWin
const UPDATE_API_URL: &str = "https://api.atomgit.com/api/v5/repos/choked-up-now/JTWin/releases";

pub async fn check_update(allow_alpha: bool) -> Result<Option<(String, String)>> {
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

    let releases: Vec<AtomGitRelease> = resp.json().await?;
    
    let current_ver = Version::parse(CURRENT_VERSION)?;
    let mut best_ver = current_ver.clone();
    let mut best_download = None;

    for release in releases {
        let tag_str = release.tag_name.trim_start_matches('v');
        if let Ok(ver) = Version::parse(tag_str) {
            if !allow_alpha && !ver.pre.is_empty() {
                continue;
            }

            if ver > best_ver {
                for asset in &release.assets {
                    if asset.name.ends_with(".exe") {
                        best_ver = ver.clone();
                        best_download = Some((tag_str.to_string(), asset.browser_download_url.clone()));
                        break;
                    }
                }
            }
        }
    }
    
    Ok(best_download)
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
        
        cmd.args([
            "/SILENT", 
            "/CLOSEAPPLICATIONS", 
            "/SUPPRESSMSGBOXES",
            "/MERGETASKS=\"!desktopicon\""
        ]);
        
        cmd.stdout(std::process::Stdio::null());
        cmd.stderr(std::process::Stdio::null());
        cmd.stdin(std::process::Stdio::null());

        cmd.creation_flags(0x08000000 | 0x00000008); 
        let _ = cmd.spawn();
    }

    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    
    std::process::exit(0);
}