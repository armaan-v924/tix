//! The generic, format-preserving config document layer — stage 1 and 2 of
//! the read path (`design/spec.md` §3.3), plus the atomic write path (#67).
//!
//! Both tix documents (global config, `.tix/ticket.toml`) are sets of
//! sections, one table per consumer. **There is no top-level typed struct**
//! for either document: stage 1 parses into a generic `toml_edit` DOM with no
//! types involved; stage 2 extracts typed sections on demand, by whoever owns
//! the type ([`TixDocument::section`]). The same document object serves reads
//! and writes — typed sections are extracted from it, edits applied against
//! it, and unknown sections (plugin tables, comments, formatting) ride
//! through untouched.
//!
//! A typed whole-document round-trip is a **data-loss bug**: deserializing
//! into structs and re-serializing emits only what the structs model,
//! silently dropping every `[<plugin>]` table plus all comments (spec §6.2).
//! Every write in tix goes through this layer instead.
//!
//! Written in `tix-cli` first; promoted here (#96) so plugins parse
//! documents identically.

use crate::error::SdkError;
use serde::de::DeserializeOwned;
use std::fmt;
use std::fs::File;
use std::path::{Path, PathBuf};
use tracing::debug;

/// A parsed tix config document: a format-preserving TOML DOM with typed
/// section extraction.
///
/// # Examples
///
/// ```ignore
/// let doc = TixDocument::load(&config_path)?;
/// let engine: Option<EngineConfig> = doc.section("engine")?;
/// let defaults: Defaults = doc.section_or_default("defaults")?;
/// ```
#[derive(Debug, Clone)]
pub struct TixDocument {
    doc: toml_edit::DocumentMut,
}

impl TixDocument {
    /// An empty document — the starting point when the file does not exist
    /// yet (`tix config init`, first write to a fresh ticket).
    pub fn empty() -> Self {
        Self {
            doc: toml_edit::DocumentMut::new(),
        }
    }

    /// Parses TOML text into the format-preserving DOM. No types involved —
    /// every section is present as an untyped subtree.
    ///
    /// # Errors
    ///
    /// [`SdkError::Message`] with the parse diagnostic on invalid TOML.
    pub fn parse(text: &str) -> Result<Self, SdkError> {
        let doc = text
            .parse::<toml_edit::DocumentMut>()
            .map_err(|e| SdkError::Message(format!("invalid TOML: {e}")))?;
        Ok(Self { doc })
    }

    /// Reads and parses the document at `path`, holding a shared advisory
    /// lock for the duration of the read so a concurrent atomic write cannot
    /// tear it.
    ///
    /// # Errors
    ///
    /// - [`SdkError::from`] if the file cannot be read
    /// - [`SdkError::Message`] on invalid TOML
    pub fn load(path: &Path) -> Result<Self, SdkError> {
        let _lock = SidecarLock::shared(path)?;
        let text = std::fs::read_to_string(path).map_err(SdkError::from)?;
        Self::parse(&text)
    }

    /// Extracts the typed section `name`, by whoever owns the type.
    ///
    /// Returns `Ok(None)` when the section is absent — normal for, e.g., a
    /// plugin's first run. "Absent" and "empty" are meaningfully different
    /// for some consumers; use [`Self::section_or_default`] when they are
    /// not.
    ///
    /// # Errors
    ///
    /// [`SdkError::Message`] when the section exists but does not deserialize
    /// into `T` (including `deny_unknown_fields` violations, which apply to
    /// this section's subtree only).
    pub fn section<T: DeserializeOwned>(&self, name: &str) -> Result<Option<T>, SdkError> {
        let Some(item) = self.doc.get(name) else {
            return Ok(None);
        };
        // toml_edit deserializes whole documents, not bare items, so the
        // section subtree is cloned into a fresh document root first.
        let mut section_doc = toml_edit::DocumentMut::new();
        *section_doc.as_item_mut() = item.clone();
        let value = toml_edit::de::from_document(section_doc)
            .map_err(|e| SdkError::Message(format!("invalid [{name}] section: {e}")))?;
        Ok(Some(value))
    }

    /// Extracts the typed section `name`, or `T::default()` when absent.
    ///
    /// # Errors
    ///
    /// Same as [`Self::section`]: a *present* but invalid section is an
    /// error, never silently defaulted.
    pub fn section_or_default<T: DeserializeOwned + Default>(
        &self,
        name: &str,
    ) -> Result<T, SdkError> {
        Ok(self.section(name)?.unwrap_or_default())
    }

    /// The underlying format-preserving DOM, for reads beyond typed sections.
    pub fn doc(&self) -> &toml_edit::DocumentMut {
        &self.doc
    }

    /// The underlying format-preserving DOM, mutably — the write path for
    /// targeted edits (`tix config set`, delta application). Edits here touch
    /// only the addressed keys; everything else survives byte-identical.
    pub fn doc_mut(&mut self) -> &mut toml_edit::DocumentMut {
        &mut self.doc
    }

    /// Replaces the section `name` with the serialization of `value`.
    ///
    /// This rewrites *that section only* — the owner of a type replacing its
    /// own table. Every other section (plugin tables, comments outside this
    /// subtree) is untouched, which is what makes this safe where a typed
    /// whole-document round-trip is not.
    ///
    /// # Errors
    ///
    /// [`SdkError::Serialization`] if `value` does not serialize to a
    /// TOML table.
    pub fn set_section<T: serde::Serialize>(
        &mut self,
        name: &str,
        value: &T,
    ) -> Result<(), SdkError> {
        let text = toml::to_string(value)?;
        let section: toml_edit::DocumentMut = text
            .parse()
            .map_err(|e| SdkError::Message(format!("serialized [{name}] is not a table: {e}")))?;
        self.doc.insert(name, section.as_item().clone());
        Ok(())
    }

    /// Serializes and atomically replaces the file at `path`: write to a
    /// sibling temp file, then rename into place (atomic on POSIX,
    /// near-atomic on NTFS). Creates parent directories as needed.
    ///
    /// Callers doing read-modify-write cycles should use [`with_write`],
    /// which holds an exclusive lock across the whole cycle; bare `save` is
    /// for whole-file writes that do not depend on prior contents.
    ///
    /// # Errors
    ///
    /// [`SdkError::from`] if the temp write or rename fails.
    pub fn save(&self, path: &Path) -> Result<(), SdkError> {
        // Parent dirs must exist before the sidecar lock can be opened there.
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(SdkError::from)?;
        }
        let _lock = SidecarLock::exclusive(path)?;
        self.save_unlocked(path)
    }

    /// The atomic tmp-write + rename, without locking — [`with_write`] holds
    /// its own exclusive lock across the full load → mutate → rename cycle.
    /// Parent directories must already exist (the lock lives in the same
    /// directory, so callers create them before locking).
    fn save_unlocked(&self, path: &Path) -> Result<(), SdkError> {
        let tmp = sibling_tmp_path(path);
        std::fs::write(&tmp, self.doc.to_string()).map_err(SdkError::from)?;
        std::fs::rename(&tmp, path).map_err(|e| {
            // Leave no droppings on a failed rename.
            let _ = std::fs::remove_file(&tmp);
            SdkError::from(e)
        })?;
        debug!(path = %path.display(), "document saved atomically");
        Ok(())
    }
}

impl fmt::Display for TixDocument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.doc.fmt(f)
    }
}

/// Runs a read-modify-write cycle under an exclusive advisory lock:
/// lock → fresh parse → `mutate` → atomic save → unlock.
///
/// The fresh parse inside the lock is what makes concurrent writers safe:
/// a second writer blocks on the lock, then parses the first writer's result,
/// so concurrency degrades to last-writer-wins at key granularity rather
/// than lost updates (#67; complements fresh-parse-at-apply, spec §6.2).
/// A missing file starts from [`TixDocument::empty`].
///
/// # Errors
///
/// Propagates errors from locking, parsing, `mutate`, and the atomic save.
///
/// # Examples
///
/// ```ignore
/// with_write(&config_path, |doc| {
///     doc.doc_mut()["cli"]["tickets_directory"] = value(path_str);
///     Ok(())
/// })?;
/// ```
pub fn with_write<T>(
    path: &Path,
    mutate: impl FnOnce(&mut TixDocument) -> Result<T, SdkError>,
) -> Result<T, SdkError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(SdkError::from)?;
    }
    let _lock = SidecarLock::exclusive(path)?;
    let mut document = if path.exists() {
        // Fresh parse under the lock, never a caller-held snapshot.
        let text = std::fs::read_to_string(path).map_err(SdkError::from)?;
        TixDocument::parse(&text)?
    } else {
        TixDocument::empty()
    };
    let result = mutate(&mut document)?;
    document.save_unlocked(path)?;
    Ok(result)
}

/// An advisory lock on a sidecar `<file>.lock`, released on drop.
///
/// The lock lives on a sidecar rather than the document itself because the
/// atomic rename replaces the document's inode — a lock on the old inode
/// would guard nothing once a writer renamed over it. The sidecar survives
/// every rename, so all readers and writers contend on one stable file.
struct SidecarLock {
    file: File,
}

impl SidecarLock {
    fn shared(document_path: &Path) -> Result<Self, SdkError> {
        let file = Self::open(document_path)?;
        file.lock_shared().map_err(SdkError::from)?;
        Ok(Self { file })
    }

    fn exclusive(document_path: &Path) -> Result<Self, SdkError> {
        let file = Self::open(document_path)?;
        file.lock().map_err(SdkError::from)?;
        Ok(Self { file })
    }

    fn open(document_path: &Path) -> Result<File, SdkError> {
        File::options()
            .create(true)
            .truncate(false)
            .write(true)
            .open(lock_path(document_path))
            .map_err(SdkError::from)
    }
}

impl Drop for SidecarLock {
    fn drop(&mut self) {
        // Errors on unlock are unreportable from Drop; the OS releases the
        // lock when the fd closes regardless.
        let _ = self.file.unlock();
    }
}

/// `<file>.lock`, next to the document.
fn lock_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".lock");
    path.with_file_name(name)
}

/// A process-unique sibling temp path for the atomic write.
fn sibling_tmp_path(path: &Path) -> PathBuf {
    let mut name = std::ffi::OsString::from(".");
    name.push(path.file_name().unwrap_or_default());
    name.push(format!(".tmp.{}", std::process::id()));
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use tempfile::tempdir;
    use tix_engine::{Defaults, TicketConfig};

    const DOCUMENT: &str = r#"# global tix config
[engine.configured_repositories.my-repo]
remote = "https://github.com/owner/repo.git" # the fork
code_path = "/home/user/code/repo"

[defaults]
branch_prefix = "feature"

# my plugin's settings — tix has no type for this table
[myplugin]
branch = "main"
retries = 3
"#;

    // --- section extraction ---

    /// A typed section deserializes from its subtree.
    #[test]
    fn test_section_extracts_typed() {
        let doc = TixDocument::parse(DOCUMENT).unwrap();
        let defaults: Option<Defaults> = doc.section("defaults").unwrap();
        assert_eq!(defaults.unwrap().branch_prefix.as_deref(), Some("feature"));
    }

    /// An absent section is Ok(None), not an error.
    #[test]
    fn test_absent_section_is_none() {
        let doc = TixDocument::parse(DOCUMENT).unwrap();
        let ticket: Option<TicketConfig> = doc.section("ticket").unwrap();
        assert!(ticket.is_none());
    }

    /// section_or_default distinguishes absent (default) from present-but-invalid (error).
    #[test]
    fn test_section_or_default() {
        let doc = TixDocument::parse(DOCUMENT).unwrap();
        let absent: Defaults = doc.section_or_default("nope").unwrap();
        assert_eq!(absent, Defaults::default());

        let invalid = TixDocument::parse("[defaults]\nbogus_field = 1\n").unwrap();
        assert!(invalid.section_or_default::<Defaults>("defaults").is_err());
    }

    /// deny_unknown_fields applies to the extracted subtree only — foreign
    /// sections never affect extraction of a typed one.
    #[test]
    fn test_unknown_sections_do_not_affect_extraction() {
        let doc = TixDocument::parse(DOCUMENT).unwrap();
        // [myplugin] has no tix type, and its presence breaks nothing.
        assert!(doc.section::<Defaults>("defaults").is_ok());
    }

    /// A struct section round-trips through the same document object used
    /// for edits.
    #[derive(Deserialize, Default, PartialEq, Debug)]
    #[serde(deny_unknown_fields)]
    struct PluginSection {
        branch: String,
        retries: i64,
    }

    /// Consumers without engine types can still extract their own sections.
    #[test]
    fn test_plugin_shaped_section() {
        let doc = TixDocument::parse(DOCUMENT).unwrap();
        let mine: PluginSection = doc.section("myplugin").unwrap().unwrap();
        assert_eq!(
            mine,
            PluginSection {
                branch: "main".to_string(),
                retries: 3
            }
        );
    }

    // --- format preservation ---

    /// A targeted edit leaves every untouched byte identical: comments,
    /// formatting, and the [myplugin] table survive.
    #[test]
    fn test_targeted_edit_preserves_everything_else() {
        let mut doc = TixDocument::parse(DOCUMENT).unwrap();
        doc.doc_mut()["defaults"]["branch_prefix"] = toml_edit::value("bugfix");
        let out = doc.to_string();

        assert!(out.contains("# global tix config"));
        assert!(out.contains("# the fork"));
        assert!(out.contains("# my plugin's settings — tix has no type for this table"));
        assert!(out.contains("branch_prefix = \"bugfix\""));
        assert!(out.contains("retries = 3"));
        // Everything except the edited line is untouched.
        assert_eq!(out.replace("bugfix", "feature"), DOCUMENT);
    }

    // --- atomic writes ---

    /// save() round-trips through disk; load() reads it back.
    #[test]
    fn test_save_load_round_trip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nested/config.toml");
        let doc = TixDocument::parse(DOCUMENT).unwrap();
        doc.save(&path).unwrap();

        let loaded = TixDocument::load(&path).unwrap();
        assert_eq!(loaded.to_string(), DOCUMENT);
    }

    /// with_write starts from empty when the file is missing, and from a
    /// fresh parse when it exists — sequential cycles compose.
    #[test]
    fn test_with_write_cycles() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");

        with_write(&path, |doc| {
            doc.doc_mut()["cli"]["tickets_directory"] = toml_edit::value("/tickets");
            Ok(())
        })
        .unwrap();
        with_write(&path, |doc| {
            doc.doc_mut()["defaults"]["branch_prefix"] = toml_edit::value("feature");
            Ok(())
        })
        .unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("tickets_directory = \"/tickets\""));
        assert!(text.contains("branch_prefix = \"feature\""));
    }

    /// A failing mutate closure writes nothing.
    #[test]
    fn test_with_write_failure_writes_nothing() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, DOCUMENT).unwrap();

        let result: Result<(), SdkError> =
            with_write(&path, |_doc| Err(SdkError::Message("nope".to_string())));
        assert!(result.is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), DOCUMENT);
    }

    /// Concurrent read-modify-write cycles on one file lose no keys: the
    /// exclusive lock serializes the cycles (#67's done-when).
    #[test]
    fn test_concurrent_with_write_loses_nothing() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");

        let handles: Vec<_> = (0..8)
            .map(|i| {
                let path = path.clone();
                std::thread::spawn(move || {
                    with_write(&path, |doc| {
                        doc.doc_mut()["section"][format!("key{i}")] = toml_edit::value(i as i64);
                        Ok(())
                    })
                    .unwrap();
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }

        let text = std::fs::read_to_string(&path).unwrap();
        for i in 0..8 {
            assert!(
                text.contains(&format!("key{i} = {i}")),
                "missing key{i} in:\n{text}"
            );
        }
    }
}
