use pyo3::prelude::*;

#[pymodule]
fn pytix(_m: &Bound<'_, PyModule>) -> PyResult<()> {
    Ok(())
}
