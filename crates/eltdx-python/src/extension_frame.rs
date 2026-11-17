//! EX/MAC framing policy with explicit Python buffer/codec/except adapters.
//! No response association or production ACK assumption is inferred here.
use crate::f10_request::builtin;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};

fn display<'py>(py: Python<'py>, value: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    "{:}".into_pyobject(py)?.call_method1("format", (value,))
}
fn length<'py>(
    module: &Bound<'py, PyAny>,
    body: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    builtin(module, "len")?.call1((body,))
}
fn exact<'py>(value: Bound<'py, PyAny>) -> PyResult<Vec<Bound<'py, PyAny>>> {
    let mut iter = value.try_iter()?;
    let mut values = Vec::new();
    for index in 0..5 {
        values.push(iter.next().transpose()?.ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!(
                "not enough values to unpack (expected 5, got {index})"
            ))
        })?);
    }
    if iter.next().transpose()?.is_some() {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "too many values to unpack (expected 5)",
        ));
    }
    Ok(values)
}

#[pyfunction]
pub fn mac_frame_request<'py>(
    py: Python<'py>,
    command: Bound<'py, PyAny>,
    body: Bound<'py, PyAny>,
    flag: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let op = py.import("operator")?;
    let prefix = module
        .getattr("struct")?
        .getattr("pack")?
        .call1(("<H", command))?;
    let inner = op.getattr("add")?.call1((prefix, body))?;
    let pack = module.getattr("struct")?.getattr("pack")?;
    let fmt = module.getattr("_MAC_HEADER_FMT")?;
    let customize = module.getattr("_MAC_CUSTOMIZE")?;
    let version = module.getattr("_MAC_VERSION")?;
    let first_len = length(&module, &inner)?;
    let second_len = length(&module, &inner)?;
    let header = pack.call1((fmt, flag, customize, version, first_len, second_len))?;
    Ok(op.getattr("add")?.call1((header, inner))?.unbind())
}

#[pyfunction]
pub fn ex_frame_header_prepare<'py>(
    py: Python<'py>,
    body: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<()> {
    if length(&module, &body)?.lt(module.getattr("HEADER_SIZE")?)? {
        let constructor = module.getattr("ProtocolError")?;
        let expected = display(py, module.getattr("HEADER_SIZE")?)?;
        let actual = display(py, length(&module, &body)?)?;
        let message = "响应帧头长度不足: 需要 {}，实际 {}"
            .into_pyobject(py)?
            .call_method1("format", (expected, actual))?;
        return Err(PyErr::from_value(constructor.call1((message,))?));
    }
    Ok(())
}

#[pyfunction]
pub fn extension_frame_header_values<'py>(
    py: Python<'py>,
    kind: &str,
    body: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let values = if kind == "mac" {
        let method = module.getattr("unpack_from")?;
        exact(method.call1((
            module.getattr("_RESP_HEADER_FMT")?,
            body,
            0,
            "mac frame header",
        ))?)?
    } else {
        let method = module.getattr("struct")?.getattr("unpack_from")?;
        exact(method.call1((module.getattr("_HEADER_FMT")?, body, 0))?)?
    };
    Ok(PyTuple::new(py, values)?.into_any().unbind())
}

#[pyfunction]
pub fn extension_frame_header_model<'py>(
    py: Python<'py>,
    kind: &str,
    values: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let values = values.cast::<PyTuple>()?;
    let constructor = module.getattr(if kind == "mac" {
        "MacFrameHeader"
    } else {
        "FrameHeader"
    })?;
    if kind == "mac" {
        return Ok(constructor.call1(values)?.unbind());
    }
    let kwargs = PyDict::new(py);
    for (index, name) in ["magic", "seq_id", "method", "zipsize", "unzipsize"]
        .into_iter()
        .enumerate()
    {
        kwargs.set_item(name, values.get_item(index)?)?;
    }
    Ok(constructor.call((), Some(&kwargs))?.unbind())
}

#[pyfunction]
pub fn extension_frame_error<'py>(
    py: Python<'py>,
    kind: &str,
    error: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let constructor = module.getattr("ProtocolError")?;
    let template = match kind {
        "header" => "响应帧头解析失败: {:}",
        "ex_limit" => "响应 body 解压超限: {:}",
        "mac_limit" => "mac frame body 解压超限: {:}",
        "ex_zlib" => "响应 body zlib 解压失败: {:}",
        "mac_zlib" => "mac frame body zlib 解压失败: {:}",
        _ => {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "unknown private frame exception",
            ))
        }
    };
    let message = template
        .into_pyobject(py)?
        .call_method1("format", (error,))?;
    Ok(constructor.call1((message,))?.unbind())
}

#[pyfunction]
pub fn extension_body_input<'py>(
    py: Python<'py>,
    kind: &str,
    header: Bound<'py, PyAny>,
    body: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<bool> {
    if length(&module, &body)?.ne(header.getattr("zipsize")?)? {
        let constructor = module.getattr("ProtocolError")?;
        let expected = display(py, header.getattr("zipsize")?)?;
        let actual = display(py, length(&module, &body)?)?;
        let template = if kind == "mac" {
            "mac frame body 长度不符: header={}, actual={}"
        } else {
            "响应 body 长度不符: header={}, actual={}"
        };
        let message = template
            .into_pyobject(py)?
            .call_method1("format", (expected, actual))?;
        return Err(PyErr::from_value(constructor.call1((message,))?));
    }
    Ok(!header
        .getattr("zipsize")?
        .eq(header.getattr("unzipsize")?)?)
}

#[pyfunction]
pub fn extension_body_output<'py>(
    py: Python<'py>,
    kind: &str,
    header: Bound<'py, PyAny>,
    body: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    if length(&module, &body)?.ne(header.getattr("unzipsize")?)? {
        let constructor = module.getattr("ProtocolError")?;
        let expected = display(py, header.getattr("unzipsize")?)?;
        let actual = display(py, length(&module, &body)?)?;
        let template = if kind == "mac" {
            "mac frame body 解压长度不符: header={}, actual={}"
        } else {
            "响应 body 解压长度不符: header={}, actual={}"
        };
        let message = template
            .into_pyobject(py)?
            .call_method1("format", (expected, actual))?;
        return Err(PyErr::from_value(constructor.call1((message,))?));
    }
    Ok(body.unbind())
}

#[pyfunction]
pub fn ex_frame_encode<'py>(
    _py: Python<'py>,
    sequence: Bound<'py, PyAny>,
    method: Bound<'py, PyAny>,
    size: Bound<'py, PyAny>,
    uncompressed: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let uncompressed = if uncompressed.is_none() {
        size.clone()
    } else {
        uncompressed
    };
    let pack = module.getattr("struct")?.getattr("pack")?;
    Ok(pack
        .call1((
            module.getattr("_HEADER_FMT")?,
            module.getattr("_MAGIC")?,
            sequence,
            method,
            size,
            uncompressed,
        ))?
        .unbind())
}

#[pyfunction]
pub fn extension_decompress_limited<'py>(
    py: Python<'py>,
    data: Bound<'py, PyAny>,
    maximum: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let decompressor = module.getattr("zlib")?.call_method0("decompressobj")?;
    let output = decompressor.call_method1("decompress", (data, &maximum))?;
    if decompressor.getattr("unconsumed_tail")?.is_truthy()?
        || decompressor.getattr("unused_data")?.is_truthy()?
    {
        let constructor = module.getattr("DecompressLimitError")?;
        let message = "解压输出超过上限 {:} 字节，已中止（声明长度不匹配或响应异常）"
            .into_pyobject(py)?
            .call_method1("format", (maximum,))?;
        return Err(PyErr::from_value(constructor.call1((message,))?));
    }
    if !decompressor.getattr("eof")?.is_truthy()? {
        return Err(PyErr::from_value(
            module
                .getattr("zlib")?
                .getattr("error")?
                .call1(("Error -5 while decompressing data: incomplete or truncated stream",))?,
        ));
    }
    Ok(output.unbind())
}
