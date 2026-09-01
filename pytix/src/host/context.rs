//! The plugin's handle on the invoking host.

use crate::engine::ticket::PyTicketConfig;
use crate::error::{message_error, sdk_error};
use crate::host::delta::PyDelta;
use crate::host::document::PyDocument;
use pyo3::exceptions::PySystemExit;
use pyo3::prelude::*;
use std::path::PathBuf;
use tix_engine::TicketConfig;
use tix_sdk::document::TixDocument;
use tix_sdk::host::{HostContext, PROTOCOL_MISMATCH_EXIT};
use tix_sdk::state;

/// The settled values the host forwarded, plus the user's own arguments.
///
/// A plugin's first act is to build one of these. It strips every `--tix-*`
/// flag out of the argument list, answers the bare `print-cli-help` handshake,
/// and checks the protocol — all before the plugin has looked at a single
/// argument of its own. The host resolved flag precedence before forwarding,
/// so plugins must not reimplement it
/// ([contract](https://tix.armaanv.dev/latest/plugins/specification/#1-the-exec-contract)).
///
/// Paths point at the real files, not staged copies, so a plugin can be
/// hand-run against arbitrary paths while debugging.
///
/// # Example
///
/// ```python
/// context = pytix.host.HostContext.from_env_or_exit("what my plugin does")
/// settings = context.config_section("myplugin") or {}
/// ```
#[pyclass(name = "HostContext", module = "pytix.host", frozen)]
pub struct PyHostContext {
    inner: HostContext,
}

impl PyHostContext {
    /// Parses `sys.argv[1:]`, optionally answering the `print-cli-help`
    /// handshake with `description`.
    ///
    /// `sys.argv` rather than the process's own argv, which is what the Rust
    /// SDK reads: a Python plugin may be reached as `python -m myplugin` or
    /// `python myplugin.py` as readily as through its `console_scripts` entry
    /// point, and only `sys.argv` names the plugin's arguments in all three.
    fn parse_argv(py: Python<'_>, description: Option<&str>) -> PyResult<Self> {
        let argv: Vec<String> = py.import("sys")?.getattr("argv")?.extract()?;
        let args = argv.into_iter().skip(1);
        let parsed = match description {
            Some(description) => HostContext::from_args_with_description(args, description),
            None => HostContext::from_args(args),
        };
        Ok(Self {
            inner: parsed.map_err(sdk_error)?,
        })
    }

    /// The ticket document's path, which is fixed relative to the ticket root.
    fn ticket_document_path(&self) -> PyResult<PathBuf> {
        Ok(self
            .inner
            .require_ticket()
            .map_err(sdk_error)?
            .join(".tix")
            .join("ticket.toml"))
    }

    /// Loads and parses a document under a shared lock, so a concurrent
    /// host write cannot tear the read.
    fn load(path: PathBuf) -> PyResult<PyDocument> {
        let document = TixDocument::load(&path).map_err(sdk_error)?;
        Ok(PyDocument::new(document, path))
    }
}

#[pymethods]
impl PyHostContext {
    /// Parses `sys.argv[1:]`.
    ///
    /// Pass `description` to answer the `print-cli-help` handshake: when the
    /// host invokes the plugin with that single argument and nothing else, the
    /// description is printed and the process exits 0.
    ///
    /// Raises `TixError` on a protocol mismatch, or when `--tix-config` is
    /// absent — the one flag every host invocation carries, so its absence
    /// means the binary was run directly rather than through `tix <name>`.
    #[staticmethod]
    #[pyo3(signature = (description = None))]
    fn from_env(py: Python<'_>, description: Option<&str>) -> PyResult<Self> {
        Self::parse_argv(py, description)
    }

    /// [`from_env`](Self::from_env) over an explicit argument list, for tests
    /// and for plugins that manage their own argv.
    #[staticmethod]
    #[pyo3(signature = (args, description = None))]
    fn from_args(args: Vec<String>, description: Option<&str>) -> PyResult<Self> {
        let parsed = match description {
            Some(description) => HostContext::from_args_with_description(args, description),
            None => HostContext::from_args(args),
        };
        Ok(Self {
            inner: parsed.map_err(sdk_error)?,
        })
    }

    /// [`from_env`](Self::from_env), turning failures into the contract's
    /// process exits instead of exceptions — the convenience entry point for a
    /// plugin's `main`.
    ///
    /// A protocol mismatch exits `PROTOCOL_MISMATCH_EXIT`, which the host
    /// reads as "this plugin needs rebuilding" rather than as a plugin
    /// failure; anything else prints to stderr and exits 1. Both raise
    /// `SystemExit`, so `finally` blocks and context managers still run.
    #[staticmethod]
    fn from_env_or_exit(py: Python<'_>, description: &str) -> PyResult<Self> {
        match Self::parse_argv(py, Some(description)) {
            Ok(context) => Ok(context),
            Err(error) => {
                let message = error.value(py).to_string();
                eprintln!("error: {message}");
                // The SDK reports a mismatch as a rebuild situation; the
                // marker is what distinguishes it from an ordinary failure.
                let code = if message.contains("— rebuild") {
                    PROTOCOL_MISMATCH_EXIT
                } else {
                    1
                };
                Err(PySystemExit::new_err(code))
            }
        }
    }

    /// The global config file.
    #[getter]
    fn config_path(&self) -> PathBuf {
        self.inner.config_path.clone()
    }

    /// The ticket directory, or `None` when the host ran outside a ticket.
    ///
    /// The absence is load-bearing rather than exceptional: `tix ticket setup`
    /// creates tickets, so it necessarily runs without one.
    #[getter]
    fn ticket_root(&self) -> Option<PathBuf> {
        self.inner.ticket_root.clone()
    }

    /// The host-created file this plugin's delta should be written to.
    #[getter]
    fn delta_path(&self) -> Option<PathBuf> {
        self.inner.delta_path.clone()
    }

    /// The alias of the repository worktree the cwd is inside, if any.
    #[getter]
    fn repo(&self) -> Option<String> {
        self.inner.repo.clone()
    }

    /// The path of that worktree.
    #[getter]
    fn repo_dir(&self) -> Option<PathBuf> {
        self.inner.repo_dir.clone()
    }

    /// The host's resolved log level.
    #[getter]
    fn log_level(&self) -> Option<String> {
        self.inner.log_level.clone()
    }

    /// The host's resolved output format: `json`, `toml`, or `default`.
    #[getter]
    fn output(&self) -> Option<String> {
        self.inner.output.clone()
    }

    /// The host's resolved color decision for this process.
    #[getter]
    fn color(&self) -> Option<bool> {
        self.inner.color
    }

    /// Everything that was not a `--tix-*` flag, in order — the plugin's own
    /// arguments, untouched. Feed this to `argparse`.
    #[getter]
    fn user_args(&self) -> Vec<String> {
        self.inner.user_args.clone()
    }

    /// The ticket directory, for plugins that require ticket context.
    ///
    /// Raises `TixError` with a user-facing "run inside a ticket" message when
    /// the host forwarded none.
    fn require_ticket(&self) -> PyResult<PathBuf> {
        self.inner.require_ticket().cloned().map_err(sdk_error)
    }

    /// Parses the global config document.
    ///
    /// Raises `TixError` if the file is missing or is not valid TOML.
    fn config_document(&self) -> PyResult<PyDocument> {
        Self::load(self.inner.config_path.clone())
    }

    /// Parses the ticket document at `<ticket_root>/.tix/ticket.toml`.
    ///
    /// Raises `TixError` when the host forwarded no ticket, or when the
    /// document is missing or invalid.
    fn ticket_document(&self) -> PyResult<PyDocument> {
        Self::load(self.ticket_document_path()?)
    }

    /// The section `name` of the global config, or `None` when absent.
    ///
    /// Re-reads the file on every call, deliberately: the host may have
    /// rewritten it since, and a plugin holding a stale parse is exactly the
    /// bug diff-back exists to prevent. Hold the
    /// [`Document`](PyDocument) yourself if you want a fixed snapshot.
    fn config_section<'py>(
        &self,
        py: Python<'py>,
        name: &str,
    ) -> PyResult<Option<Bound<'py, PyAny>>> {
        self.config_document()?.section(py, name)
    }

    /// The section `name` of the ticket document, or `None` when absent.
    ///
    /// See [`config_section`](Self::config_section) on re-reading.
    fn ticket_section<'py>(
        &self,
        py: Python<'py>,
        name: &str,
    ) -> PyResult<Option<Bound<'py, PyAny>>> {
        self.ticket_document()?.section(py, name)
    }

    /// The typed `[ticket]` section of the ticket document.
    ///
    /// The bridge between the two namespaces: the host does the IO, the engine
    /// type describes the result. Resolve the returned config against the
    /// ticket root to get a live `pytix.Ticket`.
    ///
    /// Raises `TixError` when the document has no `[ticket]` section or it
    /// does not match the schema.
    fn ticket_config(&self) -> PyResult<PyTicketConfig> {
        let path = self.ticket_document_path()?;
        let document = TixDocument::load(&path).map_err(sdk_error)?;
        let section: Option<TicketConfig> = document.section("ticket").map_err(sdk_error)?;
        section
            .map(PyTicketConfig::from_inner)
            .ok_or_else(|| message_error(format!("no [ticket] section in {}", path.display())))
    }

    /// This plugin's per-ticket state directory,
    /// `<ticket_root>/.tix/plugins/<plugin>/`, created on call.
    ///
    /// State is not config: caches and derived data of any shape, part of no
    /// document, no delta, and no protocol
    /// ([config vs state](https://tix.armaanv.dev/latest/plugins/specification/#4-plugin-state-vs-plugin-config)).
    ///
    /// Raises `TixError` without ticket context, or if creation fails.
    fn state_dir(&self, plugin: &str) -> PyResult<PathBuf> {
        let root = self.inner.require_ticket().map_err(sdk_error)?;
        state::ticket_state_dir(root, plugin).map_err(sdk_error)
    }

    /// Writes `delta` to the path the host passed in `--tix-delta`.
    ///
    /// The host applies it after this process exits cleanly. Writing nothing
    /// means changing nothing, so a plugin with no config changes never calls
    /// this.
    ///
    /// Raises `TixError` if the host forwarded no delta path — which happens
    /// only when the plugin was hand-run rather than invoked through `tix`.
    fn write_delta(&self, delta: &PyDelta) -> PyResult<()> {
        let path = self.inner.delta_path.as_deref().ok_or_else(|| {
            message_error(
                "the host passed no --tix-delta path; run this through `tix <name>` \
                 (or use Delta.write_to for hand-run debugging)",
            )
        })?;
        delta.inner().write_to(path).map_err(sdk_error)
    }

    fn __repr__(&self) -> String {
        format!(
            "HostContext(config_path={:?}, ticket_root={:?})",
            self.inner.config_path.display().to_string(),
            self.inner
                .ticket_root
                .as_ref()
                .map(|path| path.display().to_string())
        )
    }
}
