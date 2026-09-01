//! Value conversion across the three representations a plugin sees.
//!
//! Config travels in two directions and two formats, and the asymmetry is
//! deliberate ([contract](https://tix.armaanv.dev/latest/plugins/specification/#3-config-access)): inbound config is TOML,
//! because it is a
//! real file a human edits, and the outbound delta is JSON, because Python has
//! a TOML *reader* in the stdlib but no writer. So this module holds two
//! one-way conversions rather than one round trip:
//!
//! - [`item_to_py`] — TOML → Python, matching `tomllib`'s own type mapping so
//!   a section read through the SDK is indistinguishable from one read with
//!   `tomllib.load`.
//! - [`py_to_json`] — Python → JSON, tagging the one TOML type JSON cannot
//!   express as `{"$datetime": "…"}` so the caller never has to.
//!
//! Datetimes are built and inspected by calling into the `datetime` module
//! rather than through `pyo3::types::PyDateTime`: the C datetime API is absent
//! from the limited API, and these bindings are `abi3`.

use crate::error::message_error;
use pyo3::prelude::*;
use pyo3::types::{PyAnyMethods, PyDict, PyList};
use toml_edit::{Datetime, Item, Offset, Value};

/// Converts a document item — a section, a nested table, a leaf — into its
/// Python equivalent.
///
/// `Item::None` maps to `None`: it is toml_edit's absent-key sentinel, which
/// only surfaces for a key that was never there.
pub fn item_to_py<'py>(py: Python<'py>, item: &Item) -> PyResult<Bound<'py, PyAny>> {
    match item {
        Item::None => Ok(py.None().into_bound(py)),
        Item::Value(value) => value_to_py(py, value),
        Item::Table(table) => {
            let dict = PyDict::new(py);
            for (key, child) in table.iter() {
                dict.set_item(key, item_to_py(py, child)?)?;
            }
            Ok(dict.into_any())
        }
        Item::ArrayOfTables(tables) => {
            let items: Vec<Bound<'py, PyAny>> = tables
                .iter()
                .map(|table| item_to_py(py, &Item::Table(table.clone())))
                .collect::<PyResult<_>>()?;
            Ok(PyList::new(py, items)?.into_any())
        }
    }
}

/// Converts a TOML leaf value into its Python equivalent.
fn value_to_py<'py>(py: Python<'py>, value: &Value) -> PyResult<Bound<'py, PyAny>> {
    match value {
        Value::String(s) => Ok(s.value().into_pyobject(py)?.into_any()),
        Value::Integer(i) => Ok(i.value().into_pyobject(py)?.into_any()),
        Value::Float(f) => Ok(f.value().into_pyobject(py)?.into_any()),
        Value::Boolean(b) => Ok(b.value().into_pyobject(py)?.to_owned().into_any()),
        Value::Datetime(d) => datetime_to_py(py, d.value()),
        Value::Array(array) => {
            let items: Vec<Bound<'py, PyAny>> = array
                .iter()
                .map(|item| value_to_py(py, item))
                .collect::<PyResult<_>>()?;
            Ok(PyList::new(py, items)?.into_any())
        }
        Value::InlineTable(table) => {
            let dict = PyDict::new(py);
            for (key, child) in table.iter() {
                dict.set_item(key, value_to_py(py, child)?)?;
            }
            Ok(dict.into_any())
        }
    }
}

/// Builds the `datetime` object matching a TOML datetime's precision:
/// `datetime` when both halves are present, `date` or `time` when only one is.
///
/// TOML records nanoseconds and Python stores microseconds, so sub-microsecond
/// precision is truncated — the same loss `tomllib` takes.
fn datetime_to_py<'py>(py: Python<'py>, datetime: &Datetime) -> PyResult<Bound<'py, PyAny>> {
    let module = py.import("datetime")?;
    let tzinfo = match datetime.offset {
        None => py.None().into_bound(py),
        Some(Offset::Z) => module.getattr("timezone")?.getattr("utc")?,
        Some(Offset::Custom { minutes }) => {
            let delta = module
                .getattr("timedelta")?
                .call1((0, i64::from(minutes) * 60))?;
            module.getattr("timezone")?.call1((delta,))?
        }
    };

    match (datetime.date, datetime.time) {
        (Some(date), Some(time)) => module.getattr("datetime")?.call1((
            date.year,
            date.month,
            date.day,
            time.hour,
            time.minute,
            time.second.unwrap_or(0),
            time.nanosecond.unwrap_or(0) / 1_000,
            tzinfo,
        )),
        (Some(date), None) => module
            .getattr("date")?
            .call1((date.year, date.month, date.day)),
        (None, Some(time)) => module.getattr("time")?.call1((
            time.hour,
            time.minute,
            time.second.unwrap_or(0),
            time.nanosecond.unwrap_or(0) / 1_000,
            tzinfo,
        )),
        // The TOML grammar admits no datetime with neither half.
        (None, None) => Err(message_error("TOML datetime carries neither date nor time")),
    }
}

/// Converts a Python value into the JSON a delta op carries.
///
/// The mapping rides JSON's text form, which is what the host reads it back
/// through: `1` stays an integer and `1.0` a float. `datetime`, `date`, and
/// `time` instances become the tagged `{"$datetime": "…"}` form automatically —
/// the Rust SDK makes callers reach for a separate `set_datetime`, but Python
/// has real datetime types to recognize, so there is nothing to opt into.
///
/// `path` names the delta op being built, so a rejected value points at the key
/// that carried it.
pub fn py_to_json(value: &Bound<'_, PyAny>, path: &str) -> PyResult<serde_json::Value> {
    let py = value.py();

    if value.is_none() {
        return Err(message_error(format!(
            "None is not representable in TOML (delta op '{path}')"
        )));
    }
    // bool before int: Python's bool *is* an int, and `True` must not land in
    // the document as `1`.
    if let Ok(flag) = value.cast::<pyo3::types::PyBool>() {
        return Ok(serde_json::Value::Bool(flag.is_true()));
    }
    if let Ok(datetime) = datetime_to_json(value, path)? {
        return Ok(datetime);
    }
    if let Ok(integer) = value.extract::<i64>() {
        return Ok(serde_json::Value::from(integer));
    }
    if let Ok(float) = value.extract::<f64>() {
        return serde_json::Number::from_f64(float)
            .map(serde_json::Value::Number)
            .ok_or_else(|| {
                message_error(format!(
                    "{float} has no TOML representation (delta op '{path}')"
                ))
            });
    }
    if let Ok(text) = value.extract::<String>() {
        return Ok(serde_json::Value::String(text));
    }
    if let Ok(dict) = value.cast::<PyDict>() {
        let mut map = serde_json::Map::with_capacity(dict.len());
        for (key, item) in dict.iter() {
            let key = key.extract::<String>().map_err(|_| {
                message_error(format!("table keys must be strings (delta op '{path}')"))
            })?;
            map.insert(key, py_to_json(&item, path)?);
        }
        return Ok(serde_json::Value::Object(map));
    }
    // Deliberately after str and dict: both are iterable, and neither is an
    // array.
    if let Ok(iterator) = value.try_iter() {
        let mut items = Vec::new();
        for item in iterator {
            items.push(py_to_json(&item?, path)?);
        }
        return Ok(serde_json::Value::Array(items));
    }

    let type_name = value
        .get_type()
        .name()
        .map(|name| name.to_string())
        .unwrap_or_else(|_| "?".to_string());
    let _ = py;
    Err(message_error(format!(
        "{type_name} has no TOML representation (delta op '{path}')"
    )))
}

/// Recognizes the three `datetime` module types and renders them as the tagged
/// form. The outer `Result` is a real failure; the inner one is "not a
/// datetime", which is the common case and not worth an exception.
#[allow(clippy::result_large_err)]
fn datetime_to_json(
    value: &Bound<'_, PyAny>,
    path: &str,
) -> PyResult<Result<serde_json::Value, ()>> {
    let module = value.py().import("datetime")?;
    // `datetime` first: it subclasses `date`, so the wider check would swallow
    // it and drop the time half.
    for name in ["datetime", "date", "time"] {
        if value.is_instance(&module.getattr(name)?)? {
            let text: String = value.call_method0("isoformat")?.extract().map_err(|_| {
                message_error(format!(
                    "{name}.isoformat() did not return a string (delta op '{path}')"
                ))
            })?;
            return Ok(Ok(serde_json::json!({ "$datetime": text })));
        }
    }
    Ok(Err(()))
}
