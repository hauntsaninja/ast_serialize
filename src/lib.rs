use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};
use std::path::Path;

mod func_effect_visitor;
pub mod reachability;
mod serialize_ast;
pub mod type_comment;

/// Parse a Python file and serialize its AST to mypy's binary format.
///
/// # Arguments
///
/// * `fnam` - Path to the Python file to parse
/// * `skip_function_bodies` - Optional boolean to skip function bodies without externally visible effects (default: false)
/// * `python_version` - Optional tuple (major, minor) for reachability analysis (default: use sys.version_info)
/// * `platform` - Optional platform string for reachability analysis (default: use sys.platform)
/// * `always_true` - Optional list of names that are always considered true (default: empty)
/// * `always_false` - Optional list of names that are always considered false (default: empty)
///
/// # Returns
///
/// A tuple containing:
/// - bytes: The serialized AST in mypy's format (may be partial if there are syntax errors)
/// - list: A list of syntax errors, where each error is a dict with 'line', 'column', and 'message'
/// - list[tuple[int, list[str]]]: A list of tuples (line_number, error_codes) for `type: ignore` comments
/// - bytes: The serialized imports metadata
///
/// # Errors
///
/// Raises ValueError if the file cannot be read (but not for syntax errors)
#[pyfunction]
#[pyo3(signature = (
    fnam,
    skip_function_bodies=false,
    python_version=None,
    platform=None,
    always_true=None,
    always_false=None
))]
fn parse(
    py: Python,
    fnam: String,
    skip_function_bodies: bool,
    python_version: Option<(u32, u32)>,
    platform: Option<String>,
    always_true: Option<Vec<String>>,
    always_false: Option<Vec<String>>,
) -> PyResult<(Vec<u8>, Vec<PyObject>, Vec<PyObject>, Vec<u8>)> {
    // Get defaults from Python if not provided
    let python_version = python_version.unwrap_or_else(|| {
        let sys = py.import("sys").unwrap();
        let version_info = sys.getattr("version_info").unwrap();
        let major: u32 = version_info.get_item(0).unwrap().extract().unwrap();
        let minor: u32 = version_info.get_item(1).unwrap().extract().unwrap();
        (major, minor)
    });

    let platform = platform.unwrap_or_else(|| {
        let sys = py.import("sys").unwrap();
        sys.getattr("platform").unwrap().extract().unwrap()
    });

    let always_true = always_true.unwrap_or_default();
    let always_false = always_false.unwrap_or_default();

    let path = Path::new(&fnam);
    let (ast_bytes, syntax_errors, type_ignore_lines, import_bytes) =
        serialize_ast::serialize_python_file(
            path,
            skip_function_bodies,
            python_version,
            platform,
            always_true,
            always_false,
        )
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
            PyTuple::new(
                py,
                [
                    line.into_pyobject(py).unwrap().into_any(),
                    error_codes.into_pyobject(py).unwrap().into_any(),
                ],
            )
            .unwrap()
            .into()
        })
        .collect();

    Ok((ast_bytes, py_errors, py_type_ignores, import_bytes))
}

/// A Python module for parsing Python files and serializing to mypy AST format
#[pymodule]
fn ast_serialize(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(parse, m)?)?;
    Ok(())
}
