//! The generic, format-preserving config document layer — stage 1 and 2 of
//! the read path ([configuration](https://tix.armaanv.dev/latest/reference/configuration/)), plus the
//! atomic write path (#67).
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

    /// Navigates to the table at `path`, materializing every missing level
    /// as a real (headered) table and expanding any auto-vivified inline
    /// table found on the way.
    ///
    /// This is the write-path entry point for nested edits. Bare indexing
    /// (`doc["engine"]["configured_repositories"]["alpha"]`) *looks* like the
    /// same thing but is a trap: toml_edit materializes a missing key as a
    /// dotted inline table, so the whole section collapses onto one line
    /// (`engine = { configured_repositories = { alpha.remote = "…" } }`) and
    /// grows unreadable with every entry (#146). Going through here keeps
    /// nested sections rendering as `[engine.configured_repositories.alpha]`.
    ///
    /// Levels created here are marked implicit, so an intermediate that only
    /// holds sub-tables renders no header of its own. An existing inline
    /// table on `path` is expanded in place — that repairs a document a
    /// previous version collapsed, on the next write that touches it.
    ///
    /// Comments survive the repair. One attached to the collapsed line moves
    /// with it and is re-rendered above the section's header (that level
    /// gives up its implicitness so the comment has a header to sit on); the
    /// rest of the document is untouched either way. The single exception is
    /// a comment written *inside* the braces, which TOML 1.0 cannot express
    /// at all — it needs a 1.1 multi-line inline table, and nothing in tix
    /// writes that shape.
    ///
    /// # Errors
    ///
    /// [`SdkError::Message`] when a segment of `path` holds a non-table
    /// (a string, an array) — the caller addressed a key the document
    /// already uses for something else.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let repos = doc.table_at(&["engine", "configured_repositories"])?;
    /// repos.insert("alpha", toml_edit::Item::Table(entry));
    /// ```
    pub fn table_at(&mut self, path: &[&str]) -> Result<&mut toml_edit::Table, SdkError> {
        table_at(self.doc.as_item_mut(), path)
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

/// Navigates `item` to the table at `path`, creating and normalizing levels
/// as [`TixDocument::table_at`] documents. A free function so the delta
/// applier shares one traversal with the CLI's own writes.
///
/// # Errors
///
/// [`SdkError::Message`] when a segment of `path` holds a non-table.
pub(crate) fn table_at<'a>(
    item: &'a mut toml_edit::Item,
    path: &[&str],
) -> Result<&'a mut toml_edit::Table, SdkError> {
    let mut table = as_table(item, "document root")?;
    for (depth, segment) in path.iter().enumerate() {
        // A top-level table repaired from an inline value carries no document
        // position; park it past everything already placed so the repaired
        // section lands at the end of the file instead of ahead of sections
        // that were written as headers all along.
        let tail = (depth == 0).then(|| next_position(table));
        let repaired = table
            .get(segment)
            .is_some_and(|existing| existing.is_value());
        if repaired {
            // The key kept the spacing it had inside the braces, which would
            // otherwise show up in the header as `[engine ]`. Comments
            // attached to the key survive — encode moves a prefix containing
            // a newline out in front of the `[`.
            if let Some(mut key) = table.key_mut(segment) {
                tidy_header(key.leaf_decor_mut());
                tidy_header(key.dotted_decor_mut());
            }
        }
        let commented = table
            .key(segment)
            .is_some_and(|key| carries_comment(key.leaf_decor()));
        let child = as_table(
            table.entry(segment).or_insert(toml_edit::Item::None),
            segment,
        )?;
        if repaired {
            child.set_position(tail.flatten());
            // A comment sits on the key, and encode drops the decor of a
            // header it suppresses. Render this level's header so the
            // comment the user wrote has somewhere to live.
            if commented {
                child.set_implicit(false);
            }
        }
        table = child;
    }
    Ok(table)
}

/// One past the last document position among `table`'s sub-tables, or `None`
/// when it holds none.
fn next_position(table: &toml_edit::Table) -> Option<isize> {
    table
        .iter()
        .filter_map(|(_, item)| item.as_table().and_then(toml_edit::Table::position))
        .max()
        .map(|last| last + 1)
}

/// Materializes `item` as a real table in place: an absent key becomes an
/// implicit table, an inline table is expanded, a real table passes through.
///
/// # Errors
///
/// [`SdkError::Message`] when `item` holds a non-table value.
fn as_table<'a>(
    item: &'a mut toml_edit::Item,
    name: &str,
) -> Result<&'a mut toml_edit::Table, SdkError> {
    match item {
        toml_edit::Item::Table(_) => {}
        toml_edit::Item::None => {
            let mut table = toml_edit::Table::new();
            // Implicit: a level that only holds sub-tables renders no header
            // of its own — `[a.b.c]` alone, not `[a]`, `[a.b]`, `[a.b.c]`.
            // A level that holds values still renders one.
            table.set_implicit(true);
            *item = toml_edit::Item::Table(table);
        }
        toml_edit::Item::Value(toml_edit::Value::InlineTable(inline)) => {
            *item = toml_edit::Item::Table(expand(std::mem::take(inline)));
        }
        _ => {
            return Err(SdkError::Message(format!(
                "'{name}' is not a table — cannot write a nested key under it"
            )));
        }
    }
    Ok(item.as_table_mut().expect("materialized just above"))
}

/// Expands an inline table into a real one, recursing through the *dotted*
/// children (`{ alpha.remote = "…" }`) — the shape auto-vivification leaves
/// behind. A child written as an explicit inline table is a formatting
/// choice and stays one.
fn expand(inline: toml_edit::InlineTable) -> toml_edit::Table {
    let mut table = inline.into_table();
    table.set_implicit(true);
    tidy(table.decor_mut());
    for (mut key, child) in table.iter_mut() {
        tidy(key.leaf_decor_mut());
        tidy(key.dotted_decor_mut());
        match child {
            toml_edit::Item::Value(toml_edit::Value::InlineTable(nested)) if nested.is_dotted() => {
                *child = toml_edit::Item::Table(expand(std::mem::take(nested)));
            }
            toml_edit::Item::Value(value) => tidy(value.decor_mut()),
            _ => {}
        }
    }
    table
}

/// Drops whitespace-only decor, keeps decor carrying a comment.
///
/// The inline form's spacing is noise once the table has a header of its own
/// — left in place it renders as `[engine ]` or a stray-indented key. A
/// comment in the same position is not noise, and a document rewritten by
/// this module must not quietly lose one. Decor this can't read (a span into
/// input that outlived its source) is left alone: keeping stray whitespace
/// beats dropping a comment.
fn tidy(decor: &mut toml_edit::Decor) {
    if !carries_comment(decor) {
        // Cleared, not emptied: an absent decor renders at toml_edit's
        // defaults (`key = "value"`), while an explicit "" would pin the
        // spaces out of existence.
        decor.clear();
    }
}

/// Like [`tidy`], for the key of a level being turned into a table header.
///
/// The suffix is pinned empty rather than cleared — the default key suffix is
/// a space, which inside brackets reads as `[engine ]`. A comment in the
/// prefix is kept and made to start on its own line, since it is about to be
/// re-rendered above a header rather than inline after whatever preceded it.
fn tidy_header(decor: &mut toml_edit::Decor) {
    if !carries_comment(decor) {
        decor.clear();
    } else if let Some(text) = decor.prefix().and_then(toml_edit::RawString::as_str)
        && !text.starts_with('\n')
    {
        decor.set_prefix(format!("\n{text}"));
    }
    decor.set_suffix("");
}

/// Whether `decor` holds a comment. Decor this can't read (a span into input
/// that outlived its source) counts as one: keeping stray whitespace beats
/// dropping something the user wrote.
fn carries_comment(decor: &toml_edit::Decor) -> bool {
    [decor.prefix(), decor.suffix()]
        .into_iter()
        .flatten()
        .any(|raw| match toml_edit::RawString::as_str(raw) {
            Some(text) => text.contains('#'),
            None => true,
        })
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

    // --- nested table writes (#146) ---

    /// A repository entry lands as its own `[engine.…]` section, and a second
    /// one joins it — never the single-line inline form bare indexing
    /// produces.
    #[test]
    fn test_table_at_writes_nested_sections() {
        let mut doc = TixDocument::parse("[cli]\ntickets_directory = \"/t\"\n").unwrap();
        for (alias, remote) in [("alpha", "a.git"), ("beta", "b.git")] {
            let mut entry = toml_edit::Table::new();
            entry["remote"] = toml_edit::value(remote);
            doc.table_at(&["engine", "configured_repositories"])
                .unwrap()
                .insert(alias, toml_edit::Item::Table(entry));
        }
        let out = doc.to_string();

        assert!(
            out.contains("[engine.configured_repositories.alpha]"),
            "{out}"
        );
        assert!(
            out.contains("[engine.configured_repositories.beta]"),
            "{out}"
        );
        // Intermediate levels are implicit: no bare [engine] header, and no
        // inline table anywhere.
        assert!(!out.contains("engine = {"), "{out}");
        assert!(!out.contains("[engine]\n"), "{out}");
    }

    /// A document a previous version collapsed into one inline line is
    /// repaired by the next write that touches it — entries intact, comments
    /// and other sections untouched.
    #[test]
    fn test_table_at_repairs_collapsed_section() {
        let collapsed = r#"engine = { configured_repositories = { alpha.remote = "a.git", alpha.code_path = "/code/alpha" } }
[cli]
tickets_directory = "/t"

# my plugin
[myplugin]
retries = 3
"#;
        let mut doc = TixDocument::parse(collapsed).unwrap();
        let mut entry = toml_edit::Table::new();
        entry["remote"] = toml_edit::value("b.git");
        entry["code_path"] = toml_edit::value("/code/beta");
        doc.table_at(&["engine", "configured_repositories"])
            .unwrap()
            .insert("beta", toml_edit::Item::Table(entry));
        let out = doc.to_string();

        assert!(!out.contains("engine = {"), "{out}");
        assert!(
            out.contains("[engine.configured_repositories.alpha]"),
            "{out}"
        );
        assert!(out.contains("code_path = \"/code/alpha\""), "{out}");
        assert!(
            out.contains("[engine.configured_repositories.beta]"),
            "{out}"
        );
        assert!(out.contains("# my plugin"), "{out}");
        assert!(out.contains("retries = 3"), "{out}");

        // The repaired section still carries the same data.
        let engine: tix_engine::EngineConfig = TixDocument::parse(&out)
            .unwrap()
            .section("engine")
            .unwrap()
            .unwrap();
        assert_eq!(engine.configured_repositories.len(), 2);
        assert_eq!(
            engine.configured_repositories["alpha"].code_path,
            std::path::PathBuf::from("/code/alpha")
        );
    }

    /// Comments ride through the repair: one attached to the collapsed line
    /// is re-rendered above the section it described, and the rest of the
    /// document keeps its own.
    #[test]
    fn test_table_at_repair_keeps_comments() {
        let collapsed = "# global tix config\n\n\
             # where the repos live\n\
             engine = { configured_repositories = { alpha.remote = \"a.git\", \
             alpha.code_path = \"/code/alpha\" } }\n\n\
             [cli]\ntickets_directory = \"/t\"\n\n\
             # my plugin\n[myplugin]\nretries = 3\n";

        let mut doc = TixDocument::parse(collapsed).unwrap();
        doc.table_at(&["engine", "configured_repositories"])
            .unwrap();
        let out = doc.to_string();

        assert!(out.contains("# global tix config"), "{out}");
        assert!(out.contains("# where the repos live"), "{out}");
        assert!(out.contains("# my plugin"), "{out}");
        // The comment needs a header to sit above, so this level renders one.
        assert!(out.contains("[engine]"), "{out}");
        // Spacing comes back to house style, not the inline form's.
        assert!(out.contains("remote = \"a.git\""), "{out}");

        // Repairing a repaired document changes nothing.
        let mut again = TixDocument::parse(&out).unwrap();
        again
            .table_at(&["engine", "configured_repositories"])
            .unwrap();
        assert_eq!(again.to_string(), out);
    }

    /// Addressing a nested key under a non-table is a clear error, not a
    /// clobbered value.
    #[test]
    fn test_table_at_rejects_non_table() {
        let mut doc = TixDocument::parse("engine = \"nonsense\"\n").unwrap();
        let err = doc
            .table_at(&["engine", "configured_repositories"])
            .unwrap_err();
        assert!(err.to_string().contains("not a table"), "{err}");
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
