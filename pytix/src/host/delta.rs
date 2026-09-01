//! The write side of the plugin's view of a tix document: diff-back deltas.

use crate::convert::py_to_json;
use crate::error::{message_error, sdk_error};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::path::PathBuf;
use tix_sdk::delta::{Delta, DeltaOp, DeltaTarget};

/// A config delta: the ordered set of changes a plugin asks the host to make.
///
/// Config has one writer — the host — so a plugin never edits a document in
/// place. It records what it wants at dotted key paths, writes the result to
/// the path the host passed in `--tix-delta`, and exits; the host applies the
/// ops against a fresh parse afterwards
/// ([contract](https://tix.armaanv.dev/latest/plugins/specification/#3-config-access)). Writing no file
/// means no changes, so a plugin with nothing to say simply never builds one.
///
/// Ops are applied in order and overlapping keys are last-writer-wins.
///
/// Values are ordinary Python objects. `datetime`, `date`, and `time`
/// instances are tagged for the wire automatically — JSON has no datetime, and
/// the caller should not have to know that.
///
/// # Example
///
/// ```python
/// delta = pytix.host.Delta("ticket")
/// delta.set("myplugin.branch", "main")
/// delta.set("myplugin.last_run", datetime.now(timezone.utc))
/// context.write_delta(delta)
/// ```
#[pyclass(name = "Delta", module = "pytix.host")]
pub struct PyDelta {
    inner: Delta,
}

#[pymethods]
impl PyDelta {
    /// An empty delta against `target`, either `"global"` (the global config)
    /// or `"ticket"` (the ticket document).
    ///
    /// Raises `TixError` for any other target: there are exactly two
    /// documents, so a typo here is a mistake, not an extension point.
    #[new]
    fn new(target: &str) -> PyResult<Self> {
        let target = match target {
            "global" => DeltaTarget::Global,
            "ticket" => DeltaTarget::Ticket,
            other => {
                return Err(message_error(format!(
                    "unknown delta target {other:?} — expected \"global\" or \"ticket\""
                )));
            }
        };
        Ok(Self {
            inner: Delta::new(target),
        })
    }

    /// The document this delta targets: `"global"` or `"ticket"`.
    #[getter]
    fn target(&self) -> &'static str {
        match self.inner.target {
            DeltaTarget::Global => "global",
            DeltaTarget::Ticket => "ticket",
        }
    }

    /// The ops recorded so far, in order, as `{"set": path, "value": value}`
    /// dicts — the wire form, for tests and debugging.
    fn ops<'py>(&self, py: Python<'py>) -> PyResult<Vec<Bound<'py, PyDict>>> {
        self.inner
            .ops
            .iter()
            .map(|op| {
                let dict = PyDict::new(py);
                dict.set_item("set", &op.set)?;
                dict.set_item("value", json_to_py(py, &op.value)?)?;
                Ok(dict)
            })
            .collect()
    }

    /// Records `value` at the dotted key `path`, e.g. `myplugin.branch`.
    ///
    /// Raises `TixError` if `value` has no TOML representation — `None` most
    /// commonly, since TOML has no null and "absent" is spelled by not setting
    /// the key at all.
    fn set(&mut self, path: &str, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.ops.push(DeltaOp {
            set: path.to_string(),
            value: py_to_json(value, path)?,
        });
        Ok(())
    }

    /// Serializes the delta to its JSON wire form.
    fn to_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.inner)
            .map_err(|e| message_error(format!("could not serialize delta: {e}")))
    }

    /// Writes the delta to `path`.
    ///
    /// Plugins normally call `HostContext.write_delta` instead, which supplies
    /// the path the host passed in `--tix-delta`; this is for hand-running a
    /// plugin against an arbitrary file while debugging.
    fn write_to(&self, path: PathBuf) -> PyResult<()> {
        self.inner.write_to(&path).map_err(sdk_error)
    }

    fn __repr__(&self) -> String {
        format!(
            "Delta(target={:?}, ops={})",
            self.target(),
            self.inner.ops.len()
        )
    }
}

impl PyDelta {
    /// The wrapped delta, for the context's `write_delta`.
    pub(crate) fn inner(&self) -> &Delta {
        &self.inner
    }
}

/// Renders a recorded op value back as a Python object.
///
/// Only [`PyDelta::ops`] needs this — the delta's own storage is JSON, and
/// showing a caller its ops should show what it put in.
fn json_to_py<'py>(py: Python<'py>, value: &serde_json::Value) -> PyResult<Bound<'py, PyAny>> {
    use pyo3::types::PyList;
    use serde_json::Value as Json;
    Ok(match value {
        Json::Null => py.None().into_bound(py),
        Json::Bool(b) => b.into_pyobject(py)?.to_owned().into_any(),
        Json::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.into_pyobject(py)?.into_any()
            } else {
                n.as_f64().unwrap_or(f64::NAN).into_pyobject(py)?.into_any()
            }
        }
        Json::String(s) => s.into_pyobject(py)?.into_any(),
        Json::Array(items) => {
            let items: Vec<Bound<'py, PyAny>> = items
                .iter()
                .map(|item| json_to_py(py, item))
                .collect::<PyResult<_>>()?;
            PyList::new(py, items)?.into_any()
        }
        Json::Object(map) => {
            let dict = PyDict::new(py);
            for (key, item) in map {
                dict.set_item(key, json_to_py(py, item)?)?;
            }
            dict.into_any()
        }
    })
}
