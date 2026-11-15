//! Per-ticket coordination across Git, filesystem, and document mutations.

use crate::{SdkError, discovery::TIX_DIR};
use std::fs::File;
use std::path::Path;

/// An exclusive advisory guard for a ticket root, released when dropped.
///
/// Locks live in the root's parent, so destroying and recreating a ticket
/// never splits contenders across two lock files. Leave the files in place
/// after releasing the guard. Each ticket has its own lock.
#[must_use = "hold the guard through validation and mutation"]
pub struct TicketMutation {
    _file: File,
}

impl TicketMutation {
    /// Tries to lock `root`. Its parent must already exist.
    ///
    /// Existing tickets addressed through symlinks share the lock at their
    /// physical location. For a new ticket, the lock is keyed by its name in
    /// the physical parent directory.
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::TicketConflict`] if the ticket is already locked,
    /// or an I/O error if the lock cannot be opened or acquired.
    pub fn acquire(root: &Path) -> Result<Self, SdkError> {
        let absolute = std::path::absolute(root)?;
        let resolved = match absolute.canonicalize() {
            Ok(path) => path,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => absolute,
            Err(error) => return Err(error.into()),
        };
        let (Some(parent), Some(name)) = (resolved.parent(), resolved.file_name()) else {
            return Err(SdkError::TicketConflict(format!(
                "{} does not name a ticket directory",
                root.display()
            )));
        };
        let locks = parent.canonicalize()?.join(TIX_DIR).join("ticket-locks");
        std::fs::create_dir_all(&locks)?;
        let file = File::options()
            .write(true)
            .create(true)
            .truncate(false)
            .open(locks.join(name))?;
        match file.try_lock() {
            Ok(()) => Ok(Self { _file: file }),
            Err(std::fs::TryLockError::WouldBlock) => Err(SdkError::TicketConflict(format!(
                "another command is changing the ticket at {} — retry when it finishes",
                root.display()
            ))),
            Err(std::fs::TryLockError::Error(error)) => Err(error.into()),
        }
    }
}
