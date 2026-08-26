//! The read side of the plugin's view of a tix document.

use crate::convert::item_to_py;
use pyo3::prelude::*;
use std::path::PathBuf;
use tix_sdk::document::TixDocument;

/// A parsed tix document — the global config or a ticket's `.tix/ticket.toml`.
///
/// Read-only by construction, and that is the whole point: config has a single
/// writer, the host (`design/spec.md` §6). A plugin that wants a change writes
/// a [`Delta`](crate::host::delta::PyDelta) instead of editing here, so its
/// edits land against a fresh parse after it exits cleanly, with every other
/// section, comment, and byte of formatting preserved.
///
/// Sections come back as plain Python values, mapped exactly as `tomllib`
/// maps them — tables to `dict`, arrays to `list`, TOML datetimes to
/// `datetime` objects.
#[pyclass(name = "Document", module = "pytix.host", frozen)]
pub struct PyDocument {
    inner: TixDocument,
    path: PathBuf,
}

impl PyDocument {
    /// Wraps a document the context has just loaded from `path`.
    pub(crate) fn new(inner: TixDocument, path: PathBuf) -> Self {
        Self { inner, path }
    }
}

#[pymethods]
impl PyDocument {
    /// The file this document was parsed from.
    #[getter]
    fn path(&self) -> PathBuf {
        self.path.clone()
    }

    /// The top-level section `name` as a `dict`, or `None` when the document
    /// has no such section.
    ///
    /// Absent and empty are different answers: a plugin's first run sees
    /// `None`, and a plugin whose table exists but is empty sees `{}`.
    pub fn section<'py>(&self, py: Python<'py>, name: &str) -> PyResult<Option<Bound<'py, PyAny>>> {
        self.inner
            .doc()
            .get(name)
            .map(|item| item_to_py(py, item))
            .transpose()
    }

    /// The whole document as a nested `dict`.
    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        item_to_py(py, self.inner.doc().as_item())
    }

    /// The document's TOML source, byte-identical to the file on disk.
    fn __str__(&self) -> String {
        self.inner.to_string()
    }

    fn __repr__(&self) -> String {
        format!("Document(path={:?})", self.path.display().to_string())
    }
}
