//! The `simucraft._native` extension module. Keep it thin: the Python API lives
//! in `simucraft/`, the logic in the Rust crates.

use pyo3::prelude::*;

/// Check one project file. Returns diagnostics as JSON (empty list = valid).
#[pyfunction]
fn check_file(path: &str) -> PyResult<String> {
    let path = std::path::Path::new(path);
    let kind = simucraft_format::Kind::from_path(path)
        .ok_or_else(|| pyo3::exceptions::PyValueError::new_err(format!("not a SimuCraft file: {}", path.display())))?;
    let text = std::fs::read_to_string(path)?;
    let diagnostics = simucraft_format::check(kind, &text);
    Ok(serde_json::to_string(&diagnostics).expect("diagnostics serialize"))
}

/// An in-process model service. `handle` takes and returns one JSON-RPC message as text.
#[pyclass]
struct Service(simucraft_service::Service);

#[pymethods]
impl Service {
    #[new]
    fn new() -> Self {
        Self(Default::default())
    }

    /// Run one JSON-RPC request and return the response.
    fn handle(&mut self, request: &str) -> String {
        self.0.handle_json(request)
    }
}

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add_class::<Service>()?;
    m.add_function(wrap_pyfunction!(check_file, m)?)
}
