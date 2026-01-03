use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::path::Path;

mod serialize_ast;
pub mod type_comment;

/// Parse a Python file and serialize its AST to mypy's binary format.
///
/// # Arguments
///
/// * `fnam` - Path to the Python file to parse
///
/// # Returns
///
/// A tuple containing:
/// - bytes: The serialized AST in mypy's format (may be partial if there are syntax errors)
/// - list: A list of syntax errors, where each error is a dict with 'line', 'column', and 'message'
/// - list[tuple[int, list[str]]]: A list of tuples (line_number, error_codes) for `type: ignore` comments
///
/// # Errors
///
/// Raises ValueError if the file cannot be read (but not for syntax errors)
#[pyfunction]
fn parse(py: Python, fnam: String) -> PyResult<(Vec<u8>, Vec<PyObject>, Vec<PyObject>)> {
    let path = Path::new(&fnam);
    let (ast_bytes, syntax_errors, type_ignore_lines) = serialize_ast::serialize_python_file(path)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;

    // Convert syntax errors to Python dicts
    let py_errors: Vec<PyObject> = syntax_errors
        .iter()
        .map(|error| {
            let dict = PyDict::new(py);
            dict.set_item("line", error.line).unwrap();
            dict.set_item("column", error.column).unwrap();
            dict.set_item("message", error.message.clone()).unwrap();
            dict.into()
        })
        .collect();

    // Convert type ignore lines to Python tuples (line, error_codes)
    let py_type_ignores: Vec<PyObject> = type_ignore_lines
        .iter()
        .map(|(line, error_codes)| {
            (*line, error_codes.clone()).into_py(py)
        })
        .collect();

    Ok((ast_bytes, py_errors, py_type_ignores))
}

/// A Python module for parsing Python files and serializing to mypy AST format
#[pymodule]
fn ast_serialize(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(parse, m)?)?;
    Ok(())
}
