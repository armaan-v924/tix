//! Self-update command: checks GitHub releases for a newer version and installs it.

use crate::core::defaults;
use anyhow::{anyhow, bail, Context, Result};
use log::{info, warn};
use semver::Version;
use serde::Deserialize;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use tempfile::tempdir;

#[derive(Debug, Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<ReleaseAsset>,
}

struct Target {
    asset_suffix: &'static str,
    archive_ext: &'static str,
    exe_name: &'static str,
}

/// Run the update command.
pub fn run() -> Result<()> {
    let target = detect_target()?;
    let owner =
        env::var("TIX_UPDATE_OWNER").unwrap_or_else(|_| defaults::DEFAULT_RELEASE_OWNER.into());
    let repo =
        env::var("TIX_UPDATE_REPO").unwrap_or_else(|_| defaults::DEFAULT_RELEASE_REPO.into());

    let release = fetch_latest_release(&owner, &repo)?;
    let latest_version = parse_tag(&release.tag_name)?;
    let current_version = Version::parse(env!("CARGO_PKG_VERSION"))
        .context("Could not parse current package version")?;

    if latest_version <= current_version {
        info!(
            "tix {} is up to date (latest: {})",
            current_version, latest_version
        );
        return Ok(());
    }

    let asset_name = format!(
        "tix-v{}-{}.{}",
        latest_version, target.asset_suffix, target.archive_ext
    );
    let asset = release
        .assets
        .iter()
        .find(|a| a.name == asset_name)
        .ok_or_else(|| anyhow!("Release does not contain asset '{}'", asset_name))?;

    info!(
        "Updating tix from {} -> {} using asset '{}'",
        current_version, latest_version, asset_name
    );

    let tmp = tempdir().context("Failed to create temp directory for update")?;
    let archive_path = tmp.path().join(&asset.name);
    download_asset(&asset.browser_download_url, &archive_path)?;

    let extracted_path = extract_archive(&archive_path, &target)?;
    let destination = install_destination(&target)?;
    install_binary(&extracted_path, &destination)?;

    info!("Installed tix {} to {:?}", latest_version, destination);
    Ok(())
}

fn fetch_latest_release(owner: &str, repo: &str) -> Result<Release> {
    let url = format!("https://api.github.com/repos/{owner}/{repo}/releases/latest");
    let resp = ureq::get(&url)
        .set("User-Agent", defaults::UPDATE_USER_AGENT)
        .call()
        .map_err(|e| anyhow!("Failed to request latest release: {e}"))?;
    resp.into_json::<Release>()
        .map_err(|e| anyhow!("Failed to parse release JSON: {e}"))
}

fn parse_tag(tag: &str) -> Result<Version> {
    let trimmed = tag.trim_start_matches('v');
    Version::parse(trimmed).with_context(|| format!("Invalid release tag '{tag}'"))
}

fn detect_target() -> Result<Target> {
    let (os, arch) = (env::consts::OS, env::consts::ARCH);
    match (os, arch) {
        ("linux", "x86_64") => Ok(Target {
            asset_suffix: "linux-x86_64",
            archive_ext: "tar.gz",
            exe_name: "tix",
        }),
        ("macos", "aarch64") => Ok(Target {
            asset_suffix: "macos-aarch64",
            archive_ext: "tar.gz",
            exe_name: "tix",
        }),
        ("windows", "x86_64") => Ok(Target {
            asset_suffix: "windows-x86_64",
            archive_ext: "zip",
            exe_name: "tix.exe",
        }),
        _ => bail!("Unsupported platform for self-update: {os}-{arch}"),
    }
}

fn download_asset(url: &str, dest: &Path) -> Result<()> {
    let mut reader = ureq::get(url)
        .set("User-Agent", defaults::UPDATE_USER_AGENT)
        .call()
        .map_err(|e| anyhow!("Failed to download asset: {e}"))?
        .into_reader();
    let mut file = fs::File::create(dest).context("Failed to create download file")?;
    io::copy(&mut reader, &mut file).context("Failed to write downloaded asset")?;
    Ok(())
}

fn extract_archive(archive_path: &Path, target: &Target) -> Result<PathBuf> {
    let out_dir = archive_path
        .parent()
        .ok_or_else(|| anyhow!("Could not resolve parent dir for archive"))?
        .join("extract");
    fs::create_dir_all(&out_dir)?;

    match target.archive_ext {
        "tar.gz" => extract_tar_gz(archive_path, &out_dir, target.exe_name),
        "zip" => extract_zip(archive_path, &out_dir, target.exe_name),
        other => bail!("Unsupported archive format '{other}'"),
    }
}

fn extract_tar_gz(archive_path: &Path, out_dir: &Path, target_exe: &str) -> Result<PathBuf> {
    let file = fs::File::open(archive_path).context("Failed to open tar.gz asset")?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(gz);
    let mut found = None;

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        if path
            .file_name()
            .map(|n| n == OsStr::new(target_exe))
            .unwrap_or(false)
        {
            let dest = out_dir.join(target_exe);
            entry.unpack(&dest)?;
            found = Some(dest);
            break;
        }
    }

    found.ok_or_else(|| anyhow!("Executable '{}' not found in archive", target_exe))
}

fn extract_zip(archive_path: &Path, out_dir: &Path, target_exe: &str) -> Result<PathBuf> {
    let file = fs::File::open(archive_path).context("Failed to open zip asset")?;
    let mut archive = zip::ZipArchive::new(file).context("Failed to read zip archive")?;
    let mut found = None;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let name = Path::new(file.name());
        if name
            .file_name()
            .map(|n| n == OsStr::new(target_exe))
            .unwrap_or(false)
        {
            let dest = out_dir.join(target_exe);
            let mut out = fs::File::create(&dest)?;
            io::copy(&mut file, &mut out)?;
            found = Some(dest);
            break;
        }
    }

    found.ok_or_else(|| anyhow!("Executable '{}' not found in archive", target_exe))
}

fn install_destination(target: &Target) -> Result<PathBuf> {
    if let Ok(path) = env::var("TIX_INSTALL_PATH") {
        return Ok(PathBuf::from(path));
    }
    let current_exe =
        env::current_exe().context("Could not determine current executable location")?;
    let parent = current_exe
        .parent()
        .ok_or_else(|| anyhow!("Executable has no parent directory"))?;
    Ok(parent.join(target.exe_name))
}

fn install_binary(src: &Path, dest: &Path) -> Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }

    if dest.exists() {
        warn!("Replacing existing binary at {:?}", dest);
    }

    fs::rename(src, dest).or_else(|_| {
        fs::copy(src, dest)
            .map(|_| ())
            .context("Failed to copy binary into place")
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o755);
        fs::set_permissions(dest, perms).context("Failed to set executable permissions")?;
    }

    Ok(())
}
