//! Legacy block codecs over shared layouts; filesystem calls stay injectable.
use eltdx::offline::block::python_legacy as blocks;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PyTuple};

fn format(value: &Bound<'_, PyAny>) -> PyResult<String> {
    value.py().import("builtins")?.getattr("format")?.call1((value, ""))?.extract()
}

fn failure(module: &Bound<'_, PyAny>, message: String) -> PyErr {
    match module.getattr("ResourceFormatError").and_then(|kind| kind.call1((message,))) {
        Ok(value) => PyErr::from_value(value), Err(error) => error,
    }
}

fn decode<'py>(py: Python<'py>, bytes: &[u8], encoding: &str) -> PyResult<Bound<'py, PyAny>> {
    let kwargs = PyDict::new(py);
    kwargs.set_item("errors", "replace")?;
    PyBytes::new(py, bytes).call_method("decode", (encoding,), Some(&kwargs))
}

#[pyfunction]
pub fn offline_block_category(filename: Bound<'_, PyAny>) -> PyResult<u8> {
    Ok(blocks::category(&filename.call_method0("lower")?.extract::<String>()?))
}

#[pyfunction]
pub fn offline_block_safe_leaf(value: &str) -> bool { blocks::safe_leaf(value) }

#[pyfunction]
pub fn offline_block<'py>(py: Python<'py>, data: Bound<'py, PyBytes>, path: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>) -> PyResult<Py<PyAny>>
{
    let bytes = data.as_bytes();
    let Some(count) = blocks::dat_count(bytes) else {
        return Err(failure(&module, format!("板块文件头不完整: {}", format(&path)?)));
    };
    let category = module.getattr("_infer_category")?.call1((path.getattr("name")?,))?;
    let result = PyList::empty(py);
    let mut pos = 386;
    for index in 0..count {
        py.check_signals()?;
        let Some(row) = blocks::dat_record(bytes, pos) else {
            return Err(failure(&module, format!("板块文件记录被截断: 第 {}/{} 条需要 2813 字节，实际剩余 {} 字节（{}）",
                index + 1, count, bytes.len() - pos, format(&path)?)));
        };
        let name = decode(py, row.name, "gbk")?.call_method1("strip", ("\0",))?;
        let codes = PyList::empty(py);
        for bytes in row.codes() {
            py.check_signals()?;
            let code = decode(py, bytes, "ascii")?.call_method1("strip", ("\0",))?;
            if code.is_truthy()? { codes.append(code)?; }
        }
        let constructor = module.getattr("BlockInfo")?;
        let args = PyDict::new(py);
        args.set_item("name", name)?;
        args.set_item("category", &category)?;
        args.set_item("count", row.count)?;
        args.set_item("codes", PyTuple::new(py, codes.iter())?)?;
        result.append(constructor.call((), Some(&args))?)?;
        pos += 2813;
    }
    Ok(result.into_any().unbind())
}

#[pyfunction]
pub fn offline_customer_blocks<'py>(py: Python<'py>, data: Bound<'py, PyBytes>, directory: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>) -> PyResult<Py<PyAny>>
{
    let result = PyList::empty(py);
    for (name_bytes, leaf_bytes) in blocks::config_records(data.as_bytes()) {
        py.check_signals()?;
        let name = decode(py, name_bytes, "gbk")?.call_method1("split", ("\0",))?.get_item(0)?;
        let leaf = decode(py, leaf_bytes, "gbk")?.call_method1("split", ("\0",))?.get_item(0)?;
        if !leaf.is_truthy()? { continue; }
        if !module.getattr("_is_safe_block_type")?.call1((&leaf,))?.is_truthy()? {
            return Err(failure(&module, format!("blocknew.cfg 中的板块文件名非法（疑似路径穿越）: {}", leaf.repr()?.to_str()?)));
        }
        let filename = format!("{}.blk", format(&leaf)?);
        let path = py.import("operator")?.getattr("truediv")?.call1((&directory, filename))?;
        let resolved = path.call_method0("resolve")?;
        let root = directory.call_method0("resolve")?;
        if !resolved.call_method1("is_relative_to", (root,))?.is_truthy()? {
            return Err(failure(&module, format!("板块文件解析越界: {}（必须位于 {} 内）", format(&path)?, format(&directory)?)));
        }
        if !path.call_method0("is_file")?.is_truthy()? || !name.is_truthy()? { continue; }
        let kwargs = PyDict::new(py);
        kwargs.set_item("encoding", "utf-8")?;
        kwargs.set_item("errors", "replace")?;
        let text = path.call_method("read_text", (), Some(&kwargs))?;
        let codes = PyList::empty(py);
        if text.is_exact_instance_of::<pyo3::types::PyString>() {
            let text = text.extract::<String>()?;
            blocks::visit_codes(&text, |code| codes.append(code), &mut || py.check_signals())?;
        } else {
            // Custom read_text/line objects retain their original strip calls.
            for line in text.call_method0("splitlines")?.try_iter()? {
                py.check_signals()?;
                let line = line?;
                if line.call_method0("strip")?.is_truthy()? && line.call_method0("strip")?.len()? > 1 {
                    codes.append(line.call_method0("strip")?.get_item(pyo3::types::PySlice::new(py, 1, isize::MAX, 1))?)?;
                }
            }
        }
        let constructor = module.getattr("CustomerBlock")?;
        let args = PyDict::new(py);
        args.set_item("blockname", name)?;
        args.set_item("block_type", leaf)?;
        args.set_item("codes", PyTuple::new(py, codes.iter())?)?;
        result.append(constructor.call((), Some(&args))?)?;
    }
    Ok(result.into_any().unbind())
}
