//! `tix update` — self-update from GitHub releases.
//!
//! Lists the published releases, picks the newest one in the running
//! binary's major, and replaces the running binary via temp file + rename.
//! A newer major is reported but only installed with consent: under spec
//! §2.3 a major means an on-disk migration, so crossing one silently would
//! leave a binary that cannot read the user's files until `tix migrate`.

use semver::Version;
use serde::Deserialize;
use sha2::Digest;
use std::io::{IsTerminal, Read};
use std::path::{Path, PathBuf};
use tix_sdk::SdkError;
use tracing::{debug, info, warn};

/// GitHub repository the updater checks, overridable for testing.
const DEFAULT_OWNER: &str = "armaan-v924";
const DEFAULT_REPO: &str = "tix";
const USER_AGENT: &str = concat!("tix-updater/", env!("CARGO_PKG_VERSION"));
/// GitHub's maximum page size for the releases listing.
const PER_PAGE: usize = 100;

/// Update tix to the newest release in its major version
#[derive(clap::Args, Debug)]
pub struct Args {
    /// Print what would happen without downloading or writing anything
    #[arg(long)]
    pub dry_run: bool,

    /// Install a newer major version if one exists. A new major needs
    /// `tix migrate` before it can use your config and tickets
    #[arg(long)]
    pub major: bool,
}

#[derive(Debug, Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    assets: Vec<ReleaseAsset>,
}

/// The platform triple's slice of a release: asset naming + binary name.
struct Target {
    asset_suffix: &'static str,
    archive_ext: &'static str,
    exe_name: &'static str,
}

/// What the release list offers a binary at `current`.
#[derive(Debug)]
struct Selection<'a> {
    /// Newest release in the running major, if newer than `current`.
    same_major: Option<(Version, &'a Release)>,
    /// Newest release in any later major.
    newer_major: Option<(Version, &'a Release)>,
}

/// Picks candidates from a release list. Drafts and prereleases are skipped,
/// as `releases/latest` skips them, and so are tags that are not semver.
fn select<'a>(releases: &'a [Release], current: &Version) -> Selection<'a> {
    let mut selection = Selection {
        same_major: None,
        newer_major: None,
    };
    for release in releases.iter().filter(|r| !r.draft && !r.prerelease) {
        let Ok(version) = parse_tag(&release.tag_name) else {
            debug!(tag = %release.tag_name, "skipping release with a non-semver tag");
            continue;
        };
        let slot = if version.major == current.major && version > *current {
            &mut selection.same_major
        } else if version.major > current.major {
            &mut selection.newer_major
        } else {
            continue;
        };
        if slot.as_ref().is_none_or(|(best, _)| version > *best) {
            *slot = Some((version, release));
        }
    }
    selection
}

/// The answer to "cross into the newer major?".
#[derive(Debug, PartialEq, Eq)]
enum Consent {
    Cross,
    Stay,
    /// Nobody could be asked and no flag answered up front.
    Refused,
}

/// The one decision point for crossing a major. The flag answers up front;
/// otherwise only a terminal can be asked. Anything else is refused, never
/// treated as a quiet "no": [`run`] turns that into a non-zero exit.
fn consent(
    flag: bool,
    interactive: bool,
    ask: impl FnOnce() -> Result<bool, SdkError>,
) -> Result<Consent, SdkError> {
    Ok(match (flag, interactive) {
        (true, _) => Consent::Cross,
        (false, true) if ask()? => Consent::Cross,
        (false, true) => Consent::Stay,
        (false, false) => Consent::Refused,
    })
}

/// Lists releases, installs the newest in the running major, and asks
/// before crossing into a newer one.
///
/// Asset naming follows the release workflow's convention:
/// `tix-v<version>-<os>-<arch>.<tar.gz|zip>`, and every archive ships a
/// `<asset>.sha256` sibling that the download is verified against. A release
/// missing that sibling aborts the update rather than installing unverified:
/// the workflow always publishes one, so its absence means either a
/// half-uploaded release or an attacker withholding the file to steer the
/// updater onto an unchecked path. The binary is replaced via temp file +
/// rename in its own directory.
///
/// Without a terminal and without `--major`, a newer major is refused: the
/// in-major update (if any) still installs, then the command fails so a
/// script sees that a migration-bearing release is waiting.
pub fn run(_app: &crate::tix::utils::App, args: Args) -> Result<(), SdkError> {
    let target = detect_target()?;
    let owner = std::env::var("TIX_UPDATE_OWNER").unwrap_or_else(|_| DEFAULT_OWNER.into());
    let repo = std::env::var("TIX_UPDATE_REPO").unwrap_or_else(|_| DEFAULT_REPO.into());

    let releases = fetch_releases(&owner, &repo)?;
    let current = Version::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|e| SdkError::Message(format!("could not parse own version: {e}")))?;
    let Selection {
        same_major,
        newer_major,
    } = select(&releases, &current);

    if args.dry_run {
        if let Some((version, release)) = &newer_major {
            let action = if args.major {
                "would install"
            } else {
                "would skip (pass --major to install)"
            };
            println!(
                "{action} tix {version}: a new major version, needs `tix migrate` ({})",
                release.html_url
            );
        }
        let chosen = if args.major && newer_major.is_some() {
            &newer_major
        } else {
            &same_major
        };
        match chosen {
            Some((version, release)) => {
                print_plan(&target, &current, version, release)?;
            }
            None => println!("tix {current} is the newest {}.x release", current.major),
        }
        return Ok(());
    }

    let mut refused = None;
    let chosen = match newer_major {
        Some((version, release)) => {
            let ask = || {
                crate::tix::utils::confirm(&format!(
                    "tix {version} is a new major version and needs `tix migrate` before \
                     it can use your config and tickets ({}). Install it",
                    release.html_url
                ))
            };
            match consent(args.major, std::io::stdin().is_terminal(), ask)? {
                Consent::Cross => Some((version, release)),
                Consent::Stay => same_major,
                Consent::Refused => {
                    refused = Some((version, release));
                    same_major
                }
            }
        }
        None => same_major,
    };

    match chosen {
        Some((version, release)) => install(&target, &current, &version, release)?,
        None => info!("tix {current} is the newest {}.x release", current.major),
    }

    if let Some((version, release)) = refused {
        return Err(SdkError::Message(format!(
            "tix {version} is a new major version and was not installed: it needs \
             `tix migrate` before it can use your config and tickets ({}). Rerun \
             `tix cli update --major` to install it",
            release.html_url
        )));
    }
    Ok(())
}

/// The asset and checksum names `release` must publish for this platform.
fn assets<'a>(
    target: &Target,
    version: &Version,
    release: &'a Release,
) -> Result<(&'a ReleaseAsset, &'a ReleaseAsset), SdkError> {
    let asset_name = format!(
        "tix-v{version}-{}.{}",
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
    let checksum_name = format!("{asset_name}.sha256");
    let checksum_asset = release
        .assets
        .iter()
        .find(|asset| asset.name == checksum_name)
        .ok_or_else(|| {
            SdkError::Message(format!(
                "release {} publishes no '{checksum_name}' — refusing to install \
                 an unverified binary",
                release.tag_name
            ))
        })?;
    Ok((asset, checksum_asset))
}

fn print_plan(
    target: &Target,
    current: &Version,
    version: &Version,
    release: &Release,
) -> Result<(), SdkError> {
    let (asset, checksum) = assets(target, version, release)?;
    let destination = install_destination(target)?;
    println!("would update tix {current} -> {version}");
    println!("  asset:       {}", asset.name);
    println!("  checksum:    {}", checksum.name);
    println!("  destination: {}", destination.display());
    Ok(())
}

fn install(
    target: &Target,
    current: &Version,
    version: &Version,
    release: &Release,
) -> Result<(), SdkError> {
    let (asset, checksum) = assets(target, version, release)?;
    let destination = install_destination(target)?;

    info!("updating tix {current} -> {version} ({})", asset.name);
    let staging = tempfile::tempdir().map_err(SdkError::from)?;
    let archive_path = staging.path().join(&asset.name);
    download(&asset.browser_download_url, &archive_path)?;

    verify_checksum(&archive_path, &checksum.browser_download_url)?;

    let extracted = extract_archive(&archive_path, target)?;
    install_binary(&extracted, &destination)?;

    info!(
        "updated tix {current} -> {version} ({})",
        destination.display()
    );
    Ok(())
}

/// Every release, newest first as GitHub returns them, across all pages.
fn fetch_releases(owner: &str, repo: &str) -> Result<Vec<Release>, SdkError> {
    let mut releases = Vec::new();
    for page in 1.. {
        let url = format!(
            "https://api.github.com/repos/{owner}/{repo}/releases?per_page={PER_PAGE}&page={page}"
        );
        let batch = ureq::get(&url)
            .header("User-Agent", USER_AGENT)
            .header("Accept", "application/vnd.github+json")
            .call()
            .map_err(|e| SdkError::Message(format!("could not list releases: {e}")))?
            .body_mut()
            .read_json::<Vec<Release>>()
            .map_err(|e| SdkError::Message(format!("could not parse release JSON: {e}")))?;
        let last = batch.len() < PER_PAGE;
        releases.extend(batch);
        if last {
            break;
        }
    }
    Ok(releases)
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

/// Verifies the downloaded archive against the release's `.sha256` asset.
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
    verify_digest(archive_path, &parse_expected_digest(&text)?)
}

/// Pulls the digest out of a sha256sum-style checksum file: `<hex digest>`
/// optionally followed by the filename it covers.
///
/// The shape is checked here rather than deferred to the comparison, so a body
/// that is not a checksum at all — a CDN error page, a truncated upload —
/// reports what it actually is instead of a mismatch against gibberish.
fn parse_expected_digest(text: &str) -> Result<String, SdkError> {
    let token = text
        .split_whitespace()
        .next()
        .ok_or_else(|| SdkError::Message("empty checksum file".to_string()))?;
    let digest = token.to_lowercase();
    if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(SdkError::Message(format!(
            "malformed checksum file: expected a 64-character hex digest, got '{}'",
            token.chars().take(64).collect::<String>()
        )));
    }
    Ok(digest)
}

/// Hashes the archive on disk and refuses anything but an exact match.
fn verify_digest(archive_path: &Path, expected: &str) -> Result<(), SdkError> {
    let bytes = std::fs::read(archive_path).map_err(SdkError::from)?;
    let actual = hex(&sha2::Sha256::digest(&bytes));
    if actual != expected {
        return Err(SdkError::Message(format!(
            "checksum mismatch: expected {expected}, got {actual} — not installing"
        )));
    }
    debug!("checksum verified");
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Pulls the target executable out of the archive into its parent dir.
///
/// Entry paths are only matched on their file name, never joined onto the
/// output directory, so a `../` name cannot write outside it. The match
/// must be a regular file: unpacking a symlink named like the executable
/// would leave a link that `install_binary` then follows.
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
            if !entry.header().entry_type().is_file() {
                return Err(not_a_regular_file(exe));
            }
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
        let name = entry
            .name()
            .map_err(|e| SdkError::Message(format!("could not read zip entry name: {e}")))?;
        let is_exe = Path::new(name.as_ref())
            .file_name()
            .is_some_and(|name| name == exe);
        if is_exe {
            if !entry.is_file() {
                return Err(not_a_regular_file(exe));
            }
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

fn not_a_regular_file(exe: &str) -> SdkError {
    SdkError::Message(format!(
        "'{exe}' in the archive is not a regular file; refusing to install it"
    ))
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

/// Replaces the binary: rename when possible (same filesystem — atomic),
/// copy as the cross-device fallback; executable bit restored on unix.
fn install_binary(src: &Path, dest: &Path) -> Result<(), SdkError> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(SdkError::from)?;
    }
    if dest.exists() {
        warn!(dest = %dest.display(), "replacing existing binary");
    }
    std::fs::rename(src, dest)
        .or_else(|_| std::fs::copy(src, dest).map(|_| ()).map_err(SdkError::from))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dest, std::fs::Permissions::from_mode(0o755))
            .map_err(SdkError::from)?;
    }
    Ok(())
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
        assert_eq!(
            std::fs::read_to_string(&dest).unwrap(),
            "#!/bin/sh\necho new\n"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_ne!(dest.metadata().unwrap().permissions().mode() & 0o111, 0);
        }
    }

    const ZIP_TARGET: Target = Target {
        asset_suffix: "windows-x86_64",
        archive_ext: "zip",
        exe_name: "tix.exe",
    };
    const TAR_TARGET: Target = Target {
        asset_suffix: "linux-x86_64",
        archive_ext: "tar.gz",
        exe_name: "tix",
    };

    /// Writes a zip of regular files at `path`.
    fn write_zip(path: &Path, files: &[(&str, &str)]) {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        for (name, contents) in files {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            std::io::Write::write_all(&mut zip, contents.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }

    /// Appends one tar entry, writing the name straight into the header so
    /// a test can use names `Header::set_path` would refuse, like `../x`.
    fn append_tar(
        builder: &mut tar::Builder<impl std::io::Write>,
        name: &str,
        kind: tar::EntryType,
        link: Option<&str>,
        contents: &str,
    ) {
        let mut header = tar::Header::new_gnu();
        header.as_gnu_mut().unwrap().name[..name.len()].copy_from_slice(name.as_bytes());
        header.set_entry_type(kind);
        if let Some(link) = link {
            header.set_link_name(link).unwrap();
        }
        header.set_size(contents.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        builder.append(&header, contents.as_bytes()).unwrap();
    }

    fn write_tar_gz(
        path: &Path,
        fill: impl FnOnce(&mut tar::Builder<flate2::write::GzEncoder<std::fs::File>>),
    ) {
        let gz = flate2::write::GzEncoder::new(
            std::fs::File::create(path).unwrap(),
            flate2::Compression::default(),
        );
        let mut builder = tar::Builder::new(gz);
        fill(&mut builder);
        builder.into_inner().unwrap().finish().unwrap();
    }

    /// A release-shaped Windows zip (LICENSE, tix.exe at the root) yields
    /// the executable with its contents intact.
    #[test]
    fn test_extract_zip() {
        let dir = tempfile::tempdir().unwrap();
        let archive_path = dir.path().join("tix-v9.9.9-windows-x86_64.zip");
        write_zip(&archive_path, &[("LICENSE", "MIT"), ("tix.exe", "MZ new")]);

        let extracted = extract_archive(&archive_path, &ZIP_TARGET).unwrap();
        assert_eq!(extracted, dir.path().join("extract/tix.exe"));
        assert_eq!(std::fs::read_to_string(&extracted).unwrap(), "MZ new");
    }

    /// An entry whose name climbs out of the archive is matched on its file
    /// name only, and lands in the extraction directory like any other.
    /// The archive sits two levels down so a regression would write inside
    /// the tempdir, where the test can see it, rather than above it.
    #[test]
    fn test_extract_confines_traversing_entry_names() {
        let dir = tempfile::tempdir().unwrap();
        let staging = dir.path().join("a/b");
        std::fs::create_dir_all(&staging).unwrap();

        let zip_path = staging.join("tix.zip");
        write_zip(&zip_path, &[("../../escape/tix.exe", "MZ new")]);
        let extracted = extract_archive(&zip_path, &ZIP_TARGET).unwrap();
        assert_eq!(extracted, staging.join("extract/tix.exe"));

        let tar_path = staging.join("tix.tar.gz");
        write_tar_gz(&tar_path, |b| {
            append_tar(b, "../../escape/tix", tar::EntryType::Regular, None, "new")
        });
        let extracted = extract_archive(&tar_path, &TAR_TARGET).unwrap();
        assert_eq!(extracted, staging.join("extract/tix"));

        for outside in ["escape", "a/escape", "a/b/escape"] {
            assert!(
                !dir.path().join(outside).exists(),
                "an entry escaped to {outside}"
            );
        }
    }

    /// A symlink named like the executable is refused in both formats,
    /// and nothing is left at the destination for the installer to follow.
    #[test]
    fn test_extract_refuses_symlinked_executable() {
        let dir = tempfile::tempdir().unwrap();

        let zip_path = dir.path().join("tix.zip");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
        zip.add_symlink(
            "tix.exe",
            "/etc/passwd",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.finish().unwrap();

        let tar_path = dir.path().join("tix.tar.gz");
        write_tar_gz(&tar_path, |b| {
            append_tar(b, "tix", tar::EntryType::Symlink, Some("/etc/passwd"), "")
        });

        for (archive, target) in [(&zip_path, &ZIP_TARGET), (&tar_path, &TAR_TARGET)] {
            let err = extract_archive(archive, target).expect_err("symlink is refused");
            assert!(
                err.to_string().contains("not a regular file"),
                "unexpected error: {err}"
            );
            let dest = dir.path().join("extract").join(target.exe_name);
            assert!(
                dest.symlink_metadata().is_err(),
                "{} was created",
                dest.display()
            );
        }
    }

    fn release(tag: &str, draft: bool, prerelease: bool) -> Release {
        Release {
            tag_name: tag.to_string(),
            html_url: format!("https://example.test/{tag}"),
            draft,
            prerelease,
            assets: Vec::new(),
        }
    }

    /// The issue's mixed list, newest first as GitHub returns it.
    fn mixed_releases() -> Vec<Release> {
        vec![
            release("v4.1.0", false, true),
            release("v4.0.0", false, false),
            release("v3.4.0", true, false),
            release("v3.3.1", false, false),
            release("v3.3.0", false, false),
            release("not-a-version", false, false),
        ]
    }

    fn picked(slot: &Option<(Version, &Release)>) -> Option<String> {
        slot.as_ref().map(|(version, _)| version.to_string())
    }

    /// From 3.3.0: the in-major pick is 3.3.1 (the 3.4.0 draft is skipped),
    /// and 4.0.0 is reported as the newer major (the 4.1.0 prerelease is
    /// skipped).
    #[test]
    fn test_select_from_older_patch() {
        let releases = mixed_releases();
        let selection = select(&releases, &Version::new(3, 3, 0));
        assert_eq!(picked(&selection.same_major).as_deref(), Some("3.3.1"));
        assert_eq!(picked(&selection.newer_major).as_deref(), Some("4.0.0"));
    }

    /// From 3.3.1: nothing newer in the major, 4.0.0 still reported.
    #[test]
    fn test_select_from_newest_patch() {
        let releases = mixed_releases();
        let selection = select(&releases, &Version::new(3, 3, 1));
        assert_eq!(picked(&selection.same_major), None);
        assert_eq!(picked(&selection.newer_major).as_deref(), Some("4.0.0"));
    }

    /// Order in the listing does not decide the pick, and a binary already
    /// on the newest major sees no newer one.
    #[test]
    fn test_select_ignores_listing_order() {
        let mut releases = mixed_releases();
        releases.reverse();
        let selection = select(&releases, &Version::new(3, 3, 0));
        assert_eq!(picked(&selection.same_major).as_deref(), Some("3.3.1"));
        let selection = select(&releases, &Version::new(4, 0, 0));
        assert_eq!(picked(&selection.same_major), None);
        assert_eq!(picked(&selection.newer_major), None);
    }

    /// Without a terminal and without the flag, crossing is refused — not
    /// answered "no" — and nothing is asked. The flag crosses without
    /// asking; a terminal gets asked.
    #[test]
    fn test_consent() {
        let never = || -> Result<bool, SdkError> { panic!("must not prompt") };
        assert_eq!(consent(false, false, never).unwrap(), Consent::Refused);
        assert_eq!(consent(true, false, never).unwrap(), Consent::Cross);
        assert_eq!(consent(true, true, never).unwrap(), Consent::Cross);
        assert_eq!(consent(false, true, || Ok(true)).unwrap(), Consent::Cross);
        assert_eq!(consent(false, true, || Ok(false)).unwrap(), Consent::Stay);
    }

    /// The hex encoder agrees with the reference digest.
    #[test]
    fn test_hex_digest() {
        // sha256("") — the well-known empty digest.
        assert_eq!(
            hex(&sha2::Sha256::digest(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    /// The digest parser accepts what the release workflow writes — the
    /// `sha256sum`/`shasum -a 256` line and PowerShell's uppercase hex — and
    /// rejects a body that is not a checksum file.
    #[test]
    fn test_parse_expected_digest() {
        let digest = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        // GNU coreutils / shasum: two spaces, then the filename.
        assert_eq!(
            parse_expected_digest(&format!("{digest}  tix-v9.9.9-linux-x86_64.tar.gz\n")).unwrap(),
            digest
        );
        // Get-FileHash hex, and a bare digest with no filename.
        assert_eq!(
            parse_expected_digest(&digest.to_uppercase()).unwrap(),
            digest
        );

        for bogus in ["", "   \n", "<!DOCTYPE html>", "deadbeef  archive.tar.gz"] {
            assert!(
                parse_expected_digest(bogus).is_err(),
                "expected '{bogus}' to be rejected"
            );
        }
    }

    /// End to end over HTTP: the updater fetches a real `.sha256` body, and
    /// the same archive with one byte flipped is refused.
    #[test]
    fn test_verify_checksum_over_http() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("tix-v9.9.9-linux-x86_64.tar.gz");
        std::fs::write(&archive, b"pretend this is a release archive").unwrap();
        let digest = hex(&sha2::Sha256::digest(std::fs::read(&archive).unwrap()));
        let body = format!("{digest}  tix-v9.9.9-linux-x86_64.tar.gz\n");

        let url = serve_once(&body);
        verify_checksum(&archive, &url).expect("untampered archive verifies");

        // A download corrupted in flight — or swapped for someone else's
        // binary — no longer matches the digest the release published.
        std::fs::write(&archive, b"pretend this is a release archiveX").unwrap();
        let url = serve_once(&body);
        let err = verify_checksum(&archive, &url).expect_err("corrupted archive is refused");
        assert!(
            err.to_string().contains("checksum mismatch"),
            "unexpected error: {err}"
        );
    }

    /// Serves `body` to exactly one request and returns its URL. A whole HTTP
    /// mock crate would be a heavier dependency than the two lines of protocol
    /// this needs.
    fn serve_once(body: &str) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/checksum", listener.local_addr().unwrap());
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/plain\r\n\r\n{body}",
            body.len()
        );
        std::thread::spawn(move || {
            use std::io::Write;
            let (mut stream, _) = listener.accept().unwrap();
            // Drain the request line/headers so the client is not writing into
            // a socket we have already closed.
            let mut request = [0u8; 1024];
            let _ = std::io::Read::read(&mut stream, &mut request);
            stream.write_all(response.as_bytes()).unwrap();
            stream.flush().unwrap();
        });
        url
    }
}
