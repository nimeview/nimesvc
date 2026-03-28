use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug)]
struct TargetInfo {
    asset: String,
}

pub(crate) fn update_to_latest(repo: &str) -> Result<()> {
    let current = env!("CARGO_PKG_VERSION");
    let release = fetch_latest_release(repo)?;
    let latest = release.tag_name.trim_start_matches('v');
    if latest == current {
        println!("Already up to date ({}).", current);
        return Ok(());
    }

    let target = detect_target()?;
    let asset = release
        .assets
        .iter()
        .find(|a| a.name == target.asset)
        .ok_or_else(|| {
            anyhow!(
                "No asset '{}' found in release {}",
                target.asset,
                release.tag_name
            )
        })?;

    let tmp_dir = std::env::temp_dir().join("nimesvc_update");
    fs::create_dir_all(&tmp_dir).with_context(|| "Failed to create temp dir")?;
    let download_path = tmp_dir.join(&asset.name);

    download_file(&asset.browser_download_url, &download_path)?;

    let install_dir = default_install_dir()?;
    fs::create_dir_all(&install_dir)
        .with_context(|| format!("Failed to create '{}'", install_dir.display()))?;
    let dest_path = install_dir.join(binary_name());

    fs::copy(&download_path, &dest_path)
        .with_context(|| format!("Failed to write '{}'", dest_path.display()))?;
    set_executable(&dest_path)?;

    println!("Updated to {} at {}", release.tag_name, dest_path.display());
    Ok(())
}

fn fetch_latest_release(repo: &str) -> Result<GithubRelease> {
    let url = format!("https://api.github.com/repos/{}/releases/latest", repo);
    let agent = ureq::AgentBuilder::new()
        .user_agent("nimesvc-updater")
        .build();
    let resp = agent
        .get(&url)
        .call()
        .map_err(|e| anyhow!("Failed to fetch release: {e}"))?;
    let release: GithubRelease = resp
        .into_json()
        .map_err(|e| anyhow!("Invalid response: {e}"))?;
    Ok(release)
}

fn detect_target() -> Result<TargetInfo> {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let asset = match (os, arch) {
        ("macos", "aarch64") => "nimesvc-macos-arm64",
        ("macos", "x86_64") => "nimesvc-macos-x64",
        ("linux", "x86_64") => "nimesvc-linux-x64",
        ("linux", "aarch64") => "nimesvc-linux-arm64",
        ("windows", "x86_64") => "nimesvc-windows-x64.exe",
        _ => return Err(anyhow!("Unsupported target: {} {}", os, arch)),
    };
    Ok(TargetInfo {
        asset: asset.to_string(),
    })
}

fn download_file(url: &str, dest: &Path) -> Result<()> {
    let resp = ureq::get(url)
        .call()
        .map_err(|e| anyhow!("Failed to download: {e}"))?;
    let mut reader = resp.into_reader();
    let mut file = fs::File::create(dest).with_context(|| "Failed to create download file")?;
    std::io::copy(&mut reader, &mut file).with_context(|| "Failed to write download file")?;
    Ok(())
}

fn default_install_dir() -> Result<PathBuf> {
    if cfg!(windows) {
        let home = std::env::var("USERPROFILE").map_err(|_| anyhow!("Missing USERPROFILE"))?;
        Ok(PathBuf::from(home).join(".nimesvc").join("bin"))
    } else {
        let home = std::env::var("HOME").map_err(|_| anyhow!("Missing HOME"))?;
        Ok(PathBuf::from(home).join(".nimesvc").join("bin"))
    }
}

fn binary_name() -> String {
    if cfg!(windows) {
        "nimesvc.exe".to_string()
    } else {
        "nimesvc".to_string()
    }
}

fn set_executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms)?;
    }
    #[cfg(windows)]
    {
        let _ = path;
    }
    Ok(())
}
