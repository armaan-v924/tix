//! Python bindings for tix.
//!
//! One extension module, two namespaces:
//!
//! - `pytix.*` binds `tix-engine` — repositories, tickets, worktrees:
//!   general-purpose scripting against tix's domain operations, with the
//!   engine's contract intact — resolved paths in, no ambient IO, no layout
//!   policy.
//! - `pytix.host` binds `tix-sdk` — the plugin's handle on the invoking host:
//!   `--tix-*` flag parsing, the protocol check, document reads, the delta
//!   channel, and state directories.
//!
//! Both are compiled into every wheel unconditionally. There are no Cargo
//! feature gates and no pip extras, because a wheel is built once and extras
//! cannot vary compiled content — `import pytix.host` either always works or
//! is a lie.
//!
//! # Layering
//!
//! The split is not cosmetic. `tix-engine` refuses to know where anything
//! lives; `tix-sdk` is the layer that decides. Collapsing them into one flat
//! Python namespace would erase the distinction that keeps the engine
//! testable and the layout policy in one place, so the module tree mirrors
//! the crate graph exactly.

mod convert;
mod engine;
mod error;
mod host;

use pyo3::prelude::*;

/// Python bindings for tix: `pytix` for the engine, `pytix.host` for the
/// plugin's handle on the invoking host.
#[pymodule]
mod pytix {
    use pyo3::prelude::*;

    #[pymodule_export]
    use crate::error::TixError;

    #[pymodule_export]
    use crate::engine::repository::{PyRepository, PyRepositoryConfig};

    #[pymodule_export]
    use crate::engine::ticket::{PyTicket, PyTicketConfig};

    #[pymodule_export]
    use crate::engine::worktree::{PyWorktree, PyWorktreeConfig};

    /// The plugin's handle on the invoking host: forwarded flags, document
    /// reads, state directories, and the config delta channel.
    #[pymodule]
    mod host {
        // The nested module resolves `#[pymodule_export]` in its own
        // scope; rustc then sees the import as unused because the
        // macro consumed every use of it.
        #[allow(unused_imports)]
        use pyo3::prelude::*;

        #[pymodule_export]
        use crate::host::cache_dir;

        #[pymodule_export]
        use crate::host::context::PyHostContext;

        #[pymodule_export]
        use crate::host::delta::PyDelta;

        #[pymodule_export]
        use crate::host::document::PyDocument;

        /// The invocation-contract version this build speaks.
        ///
        /// Monotonic and independent of the crate version. Bumped only when an
        /// existing flag or document is removed, renamed, or changes meaning —
        /// never for additions, since unknown `--tix-*` flags are ignored by
        /// contract and flag presence doubles as capability detection.
        #[pymodule_export]
        const PROTOCOL: u64 = tix_sdk::host::PROTOCOL;

        /// The exit code reserved for a protocol mismatch.
        ///
        /// The established tool-layer-error slot, excluded from the range the
        /// host propagates, so "rebuild this plugin" never reads as "the
        /// plugin failed".
        #[pymodule_export]
        const PROTOCOL_MISMATCH_EXIT: i32 = tix_sdk::host::PROTOCOL_MISMATCH_EXIT;
    }

    /// Registers `pytix.host` in `sys.modules`.
    ///
    /// A submodule of an extension module is an attribute of its parent, not
    /// a package entry, so `import pytix.host` would fail without this. The
    /// import machinery re-checks `sys.modules` after importing the parent —
    /// exactly the hook this uses.
    #[pymodule_init]
    fn init(module: &Bound<'_, PyModule>) -> PyResult<()> {
        let py = module.py();
        py.import("sys")?
            .getattr("modules")?
            .set_item("pytix.host", module.getattr("host")?)
    }
}
