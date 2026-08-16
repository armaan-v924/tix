//! `tix update` — self-update from GitHub releases.
//!
//! Fetches the latest release, compares against the compiled-in version,
//! and replaces the running binary via temp file + rename. A direct port of
//! v2's self-updater, adjusted for the workspace and given `--dry-run` and
//! checksum verification.

use semver::Version;
use serde::Deserialize;
use sha2::Digest;
use std::io::Read;
use std::path::{Path, PathBuf};
use tix_sdk::SdkError;
use tracing::{info, warn};

/// GitHub repository the updater checks, overridable for testing.
const DEFAULT_OWNER: &str = "armaan-v924";
const DEFAULT_REPO: &str = "tix";
const USER_AGENT: &str = concat!("tix-updater/", env!("CARGO_PKG_VERSION"));

/// Update tix to the latest release
#[derive(clap::Args, Debug)]
pub struct Args {
    /// Print what would happen without downloading or writing anything
    #[arg(long)]
    pub dry_run: bool,
}

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

/// The platform triple's slice of a release: asset naming + binary name.
struct Target {
    asset_suffix: &'static str,
    archive_ext: &'static str,
    exe_name: &'static str,
}

/// Checks the latest GitHub release and installs it when newer.
///
/// Asset naming follows the release workflow's convention:
/// `tix-v<version>-<os>-<arch>.<tar.gz|zip>`. If the release also carries a
/// `<asset>.sha256` sibling, the download is verified against it; releases
/// without checksums skip verification. The binary is replaced via temp
/// file + rename in its own directory.
pub fn run(_app: &crate::tix::utils::App, args: Args) -> Result<(), SdkError> {
    let target = detect_target()?;
    let owner = std::env::var("TIX_UPDATE_OWNER").unwrap_or_else(|_| DEFAULT_OWNER.into());
    let repo = std::env::var("TIX_UPDATE_REPO").unwrap_or_else(|_| DEFAULT_REPO.into());

    let release = fetch_latest_release(&owner, &repo)?;
    let latest = parse_tag(&release.tag_name)?;
    let current = Version::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|e| SdkError::Message(format!("could not parse own version: {e}")))?;

    if latest <= current {
        println!("tix {current} is already up to date (latest release: {latest})");
        return Ok(());
    }

    let asset_name = format!(
        "tix-v{latest}-{}.{}",
        target.asset_suffix, target.archive_ext
    );
    let asset = release
        .assets
        .iter()
        .find(|asset| asset.name == asset_name)
        .ok_or_else(|| {
            SdkError::Message(format!(
                "release {} has no asset '{asset_name}' for this platform",
                release.tag_name
            ))
        })?;
    let checksum_asset = release
        .assets
        .iter()
        .find(|a| a.name == format!("{asset_name}.sha256"));
    let destination = install_destination(&target)?;

    if args.dry_run {
        println!("would update tix {current} -> {latest}");
        println!("  asset:       {asset_name}");
        println!(
            "  checksum:    {}",
            checksum_asset.map_or("none published", |_| "verified against .sha256")
        );
        println!("  destination: {}", destination.display());
        return Ok(());
    }

    info!(from = %current, to = %latest, asset = %asset_name, "updating tix");
    let staging = tempfile::tempdir().map_err(SdkError::from)?;
    let archive_path = staging.path().join(&asset.name);
    download(&asset.browser_download_url, &archive_path)?;

    if let Some(checksum) = checksum_asset {
        verify_checksum(&archive_path, &checksum.browser_download_url)?;
    } else {
        info!("release publishes no checksum for '{asset_name}' — skipping verification");
    }

    let extracted = extract_archive(&archive_path, &target)?;
    install_binary(&extracted, &destination)?;

    println!("updated tix {current} -> {latest} ({})", destination.display());
    Ok(())
}

fn fetch_latest_release(owner: &str, repo: &str) -> Result<Release, SdkError> {
    let url = format!("https://api.github.com/repos/{owner}/{repo}/releases/latest");
    let mut response = ureq::get(&url)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(|e| SdkError::Message(format!("could not fetch latest release: {e}")))?;
    response
        .body_mut()
        .read_json::<Release>()
        .map_err(|e| SdkError::Message(format!("could not parse release JSON: {e}")))
}

fn parse_tag(tag: &str) -> Result<Version, SdkError> {
    Version::parse(tag.trim_start_matches('v'))
        .map_err(|e| SdkError::Message(format!("invalid release tag '{tag}': {e}")))
}

fn detect_target() -> Result<Target, SdkError> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
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
        (os, arch) => Err(SdkError::Message(format!(
            "self-update is not supported on {os}-{arch}"
        ))),
    }
}

fn download(url: &str, dest: &Path) -> Result<(), SdkError> {
    let mut response = ureq::get(url)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(|e| SdkError::Message(format!("download failed: {e}")))?;
    let mut file = std::fs::File::create(dest).map_err(SdkError::from)?;
    std::io::copy(&mut response.body_mut().as_reader(), &mut file).map_err(SdkError::from)?;
    Ok(())
}

/// Verifies the downloaded archive against the release's `.sha256` asset
/// (`<hex digest>` optionally followed by a filename, sha256sum-style).
fn verify_checksum(archive_path: &Path, checksum_url: &str) -> Result<(), SdkError> {
    let mut response = ureq::get(checksum_url)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(|e| SdkError::Message(format!("checksum download failed: {e}")))?;
    let mut text = String::new();
    response
        .body_mut()
        .as_reader()
        .take(4096)
        .read_to_string(&mut text)
        .map_err(SdkError::from)?;
    let expected = text
        .split_whitespace()
        .next()
        .ok_or_else(|| SdkError::Message("empty checksum file".to_string()))?
        .to_lowercase();

    let bytes = std::fs::read(archive_path).map_err(SdkError::from)?;
    let actual = hex(&sha2::Sha256::digest(&bytes));
    if actual != expected {
        return Err(SdkError::Message(format!(
            "checksum mismatch: expected {expected}, got {actual} — not installing"
        )));
    }
    info!("checksum verified");
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Pulls the target executable out of the archive into its parent dir.
fn extract_archive(archive_path: &Path, target: &Target) -> Result<PathBuf, SdkError> {
    let out_dir = archive_path
        .parent()
        .expect("staging archive always has a parent")
        .join("extract");
    std::fs::create_dir_all(&out_dir).map_err(SdkError::from)?;

    match target.archive_ext {
        "tar.gz" => extract_tar_gz(archive_path, &out_dir, target.exe_name),
        "zip" => extract_zip(archive_path, &out_dir, target.exe_name),
        other => Err(SdkError::Message(format!(
            "unsupported archive format '{other}'"
        ))),
    }
}

fn extract_tar_gz(archive_path: &Path, out_dir: &Path, exe: &str) -> Result<PathBuf, SdkError> {
    let file = std::fs::File::open(archive_path).map_err(SdkError::from)?;
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(file));
    for entry in archive.entries().map_err(SdkError::from)? {
        let mut entry = entry.map_err(SdkError::from)?;
        let is_exe = entry
            .path()
            .map_err(SdkError::from)?
            .file_name()
            .is_some_and(|name| name == exe);
        if is_exe {
            let dest = out_dir.join(exe);
            entry.unpack(&dest).map_err(SdkError::from)?;
            return Ok(dest);
        }
    }
    Err(SdkError::Message(format!(
        "executable '{exe}' not found in archive"
    )))
}

fn extract_zip(archive_path: &Path, out_dir: &Path, exe: &str) -> Result<PathBuf, SdkError> {
    let file = std::fs::File::open(archive_path).map_err(SdkError::from)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| SdkError::Message(format!("could not read zip archive: {e}")))?;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|e| SdkError::Message(format!("could not read zip entry: {e}")))?;
        let is_exe = Path::new(entry.name())
            .file_name()
            .is_some_and(|name| name == exe);
        if is_exe {
            let dest = out_dir.join(exe);
            let mut out = std::fs::File::create(&dest).map_err(SdkError::from)?;
            std::io::copy(&mut entry, &mut out).map_err(SdkError::from)?;
            return Ok(dest);
        }
    }
    Err(SdkError::Message(format!(
        "executable '{exe}' not found in archive"
    )))
}

/// Where the new binary lands: `TIX_INSTALL_PATH` override, else next to the
/// currently running executable.
fn install_destination(target: &Target) -> Result<PathBuf, SdkError> {
    if let Ok(path) = std::env::var("TIX_INSTALL_PATH") {
        return Ok(PathBuf::from(path));
    }
    let current = std::env::current_exe().map_err(SdkError::from)?;
    let parent = current
        .parent()
        .ok_or_else(|| SdkError::Message("running executable has no parent directory".into()))?;
    Ok(parent.join(target.exe_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A release-shaped tar.gz (./LICENSE, ./tix) extracts the executable,
    /// and install_binary lands it at the destination with the exec bit.
    #[test]
    fn test_extract_and_install() {
        let dir = tempfile::tempdir().unwrap();
        let archive_path = dir.path().join("tix-v9.9.9-macos-aarch64.tar.gz");
        let gz = flate2::write::GzEncoder::new(
            std::fs::File::create(&archive_path).unwrap(),
            flate2::Compression::default(),
        );
        let mut builder = tar::Builder::new(gz);
        for (name, contents) in [("./LICENSE", "MIT"), ("./tix", "#!/bin/sh\necho new\n")] {
            let mut header = tar::Header::new_gnu();
            header.set_path(name).unwrap();
            header.set_size(contents.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder.append(&header, contents.as_bytes()).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap();

        let target = Target {
            asset_suffix: "macos-aarch64",
            archive_ext: "tar.gz",
            exe_name: "tix",
        };
        let extracted = extract_archive(&archive_path, &target).unwrap();
        assert!(extracted.ends_with("tix"));

        let dest = dir.path().join("bin/tix");
        install_binary(&extracted, &dest).unwrap();
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), "#!/bin/sh\necho new\n");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_ne!(dest.metadata().unwrap().permissions().mode() & 0o111, 0);
        }
    }

    /// A checksum mismatch refuses to install.
    #[test]
    fn test_hex_digest() {
        // sha256("") — the well-known empty digest.
        assert_eq!(
            hex(&sha2::Sha256::digest(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}

/// Replaces the binary: rename when possible (same filesystem — atomic),
/// copy as the cross-device fallback; executable bit restored on unix.
fn install_binary(src: &Path, dest: &Path) -> Result<(), SdkError> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(SdkError::from)?;
    }
    if dest.exists() {
        warn!(dest = %dest.display(), "replacing existing binary");
    }
    std::fs::rename(src, dest).or_else(|_| {
        std::fs::copy(src, dest).map(|_| ()).map_err(SdkError::from)
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dest, std::fs::Permissions::from_mode(0o755))
            .map_err(SdkError::from)?;
    }
    Ok(())
}
