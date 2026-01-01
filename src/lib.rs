use pyo3::prelude::*;
use std::path::Path;

mod serialize_ast;

/// Parse a Python file and serialize its AST to mypy's binary format.
///
/// # Arguments
///
/// * `fnam` - Path to the Python file to parse
///
/// # Returns
///
/// A bytes object containing the serialized AST in mypy's format
///
/// # Errors
///
/// Raises ValueError if the file cannot be read or parsed
#[pyfunction]
fn parse(fnam: String) -> PyResult<Vec<u8>> {
    let path = Path::new(&fnam);
    serialize_ast::serialize_python_file(path)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
}

/// A Python module for parsing Python files and serializing to mypy AST format
#[pymodule]
fn ast_serialize(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(parse, m)?)?;
    Ok(())
}
