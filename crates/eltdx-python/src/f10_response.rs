//! Legacy table objects and extension hooks over shared F10 cell traversal.
use eltdx::f10::python_legacy::{self as shared, Projection};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};

fn builtin<'py>(module: &Bound<'py, PyAny>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    let builtins = module.py().import("builtins")?;
    builtins.getattr("getattr")?.call1((module, name, builtins.getattr(name)?))
}

fn instance(value: &Bound<'_, PyAny>, class: &Bound<'_, PyAny>, module: &Bound<'_, PyAny>) -> PyResult<bool> {
    builtin(module, "isinstance")?.call1((value, class))?.is_truthy()
}

fn is_sequence(value: &Bound<'_, PyAny>, module: &Bound<'_, PyAny>) -> PyResult<bool> {
    if !instance(value, &module.getattr("Sequence")?, module)? { return Ok(false); }
    let excluded = PyTuple::new(module.py(), [builtin(module, "str")?, builtin(module, "bytes")?, builtin(module, "bytearray")?])?;
    Ok(!instance(value, excluded.as_any(), module)?)
}

fn protocol(module: &Bound<'_, PyAny>, message: &str) -> PyErr {
    match module.getattr("ProtocolError").and_then(|kind| kind.call1((message,))) {
        Ok(value) => PyErr::from_value(value), Err(error) => error,
    }
}

fn fallback_empty<'py>(value: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    if value.is_truthy()? { Ok(value) } else { Ok(PyTuple::empty(value.py()).into_any()) }
}

#[pyfunction]
pub fn f10_parse_response<'py>(py: Python<'py>, entry: Bound<'py, PyAny>, request_body: Bound<'py, PyAny>,
    raw: Bound<'py, PyAny>, module: Bound<'py, PyAny>) -> PyResult<Py<PyAny>>
{
    let tables = fallback_empty(raw.call_method1("get", ("ResultSets",))?)?;
    if !is_sequence(&tables, &module)? { return Err(protocol(&module, "TQLEX ResultSets must be an array")); }
    let tables = shared::visit_tables(tables.try_iter()?,
        |table, index| module.getattr("_parse_result_set")?.call1((table, index)), &mut || py.check_signals())?;
    let tables = PyTuple::new(py, tables)?;
    let error = raw.call_method1("get", ("ErrorCode",))?;
    let error = if error.is_none() { error } else { builtin(&module, "int")?.call1((error,))? };
    let constructor = module.getattr("F10Response")?;
    let args = PyDict::new(py);
    args.set_item("entry", entry)?;
    args.set_item("request_body", request_body)?;
    args.set_item("error_code", error)?;
    args.set_item("result_sets", tables)?;
    args.set_item("raw", builtin(&module, "dict")?.call1((raw,))?)?;
    Ok(constructor.call((), Some(&args))?.unbind())
}

#[pyfunction]
pub fn f10_parse_table<'py>(py: Python<'py>, raw: Bound<'py, PyAny>, index: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>) -> PyResult<Py<PyAny>>
{
    if !instance(&raw, &module.getattr("Mapping")?, &module)? { return Err(protocol(&module, "TQLEX ResultSet must be an object")); }
    let columns = module.getattr("_columns")?.call1((&raw,))?;
    let content = fallback_empty(raw.call_method1("get", ("Content",))?)?;
    if !is_sequence(&content, &module)? { return Err(protocol(&module, "TQLEX ResultSet Content must be an array")); }
    let rows = PyList::empty(py);
    let row_cells = PyList::empty(py);
    for row in content.try_iter()? {
        py.check_signals()?;
        let values = module.getattr("_row_values")?.call1((row?,))?;
        let cells = shared::visit_cells(values.try_iter()?, |index, value| {
            let constructor = module.getattr("F10Cell")?;
            let args = PyDict::new(py);
            args.set_item("name", module.getattr("_column_name")?.call1((&columns, index))?)?;
            args.set_item("value", value)?;
            args.set_item("index", index)?;
            constructor.call((), Some(&args))
        }, &mut || py.check_signals())?;
        let cells = PyTuple::new(py, cells)?;
        row_cells.append(&cells)?;
        rows.append(module.getattr("_row_dict")?.call1((cells,))?)?;
    }
    let constructor = module.getattr("F10ResultSet")?;
    let args = PyDict::new(py);
    let key = raw.call_method1("get", ("ResultSetKey",))?;
    let key = if key.is_truthy()? { key } else {
        let index: String = builtin(&module, "format")?.call1((index, ""))?.extract()?;
        format!("table{index}").into_pyobject(py)?.into_any()
    };
    args.set_item("key", key)?;
    args.set_item("columns", builtin(&module, "tuple")?.call1((columns,))?)?;
    args.set_item("rows", PyTuple::new(py, rows.iter())?)?;
    args.set_item("row_cells", PyTuple::new(py, row_cells.iter())?)?;
    args.set_item("raw", builtin(&module, "dict")?.call1((raw,))?)?;
    Ok(constructor.call((), Some(&args))?.unbind())
}

#[pyfunction]
pub fn f10_columns<'py>(py: Python<'py>, raw: Bound<'py, PyAny>, module: Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    let result = PyList::empty(py);
    let name = if raw.contains("ColName")? { "ColName" } else if raw.contains("ColDes")? { "ColDes" }
        else { return Ok(result.into_any().unbind()); };
    let values = fallback_empty(raw.call_method1("get", (name,))?)?;
    if !is_sequence(&values, &module)? { return Err(protocol(&module, if name == "ColName" { "TQLEX ColName must be an array" } else { "TQLEX ColDes must be an array" })); }
    for value in values.try_iter()? {
        py.check_signals()?;
        let value = value?;
        if name == "ColDes" && instance(&value, &module.getattr("Mapping")?, &module)? {
            let string = builtin(&module, "str")?;
            result.append(string.call1((value.call_method1("get", ("Name", ""))?,))?)?;
        } else { result.append(builtin(&module, "str")?.call1((value,))?)?; }
    }
    Ok(result.into_any().unbind())
}

#[pyfunction]
pub fn f10_row_values<'py>(py: Python<'py>, row: Bound<'py, PyAny>, module: Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    if is_sequence(&row, &module)? { Ok(builtin(&module, "list")?.call1((row,))?.unbind()) }
    else { Ok(PyList::new(py, [row])?.into_any().unbind()) }
}

#[pyfunction]
pub fn f10_column_name<'py>(py: Python<'py>, columns: Bound<'py, PyAny>, index: Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    let value = shared::column_name_with(|| index.lt(columns.len()?), || columns.get_item(&index),
        |value| value.is_truthy(), || {
            let index: String = py.import("builtins")?.getattr("format")?.call1((&index, ""))?.extract()?;
            Ok(format!("col_{index}").into_pyobject(py)?.into_any())
        })?;
    Ok(value.unbind())
}

struct PythonProjection<'py> { counts: Bound<'py, PyDict>, output: Bound<'py, PyDict>, module: Bound<'py, PyAny> }
impl<'py> Projection<Bound<'py, PyAny>> for PythonProjection<'py> {
    type Name = Bound<'py, PyAny>;
    type Value = Bound<'py, PyAny>;
    type Error = PyErr;
    fn name(&mut self, cell: &Bound<'py, PyAny>) -> PyResult<Self::Name> { cell.getattr("name") }
    fn count(&mut self, name: Self::Name) -> PyResult<usize> { self.counts.call_method1("get", (name, 0))?.extract() }
    fn store_count(&mut self, name: Self::Name, count: usize) -> PyResult<()> { self.counts.set_item(name, count) }
    fn suffix(&mut self, name: Self::Name, count: usize) -> PyResult<Self::Name> {
        let name: String = builtin(&self.module, "format")?.call1((name, ""))?.extract()?;
        Ok(format!("{name}__{count}").into_pyobject(self.module.py())?.into_any())
    }
    fn value(&mut self, cell: Bound<'py, PyAny>) -> PyResult<Self::Value> { cell.getattr("value") }
    fn insert(&mut self, name: Self::Name, value: Self::Value) -> PyResult<()> { self.output.set_item(name, value) }
}

#[pyfunction]
pub fn f10_row_dict<'py>(py: Python<'py>, cells: Bound<'py, PyAny>, module: Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    let mut projection = PythonProjection { counts: PyDict::new(py), output: PyDict::new(py), module };
    shared::project_cells(cells.try_iter()?, &mut projection, &mut || py.check_signals())?;
    Ok(projection.output.into_any().unbind())
}
