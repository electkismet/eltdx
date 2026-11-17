//! Python EX configuration policy. Path/stream/exception objects remain explicit
//! adapters; parsing, selection, validation and save preparation live here.
use crate::f10_request::builtin;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyIterator, PyList, PySlice};

fn formatted<'py>(
    py: Python<'py>,
    value: &Bound<'py, PyAny>,
    spec: &str,
) -> PyResult<Bound<'py, PyAny>> {
    format!("{{:{spec}}}")
        .into_pyobject(py)?
        .call_method1("format", (value,))
}
fn format<'py>(
    py: Python<'py>,
    pattern: &str,
    values: &[Bound<'py, PyAny>],
) -> PyResult<Bound<'py, PyAny>> {
    pattern
        .into_pyobject(py)?
        .call_method1("format", pyo3::types::PyTuple::new(py, values)?)
}
fn error(module: &Bound<'_, PyAny>, message: Bound<'_, PyAny>) -> PyResult<PyErr> {
    Ok(PyErr::from_value(
        builtin(module, "ValueError")?.call1((message,))?,
    ))
}
fn is_instance(module: &Bound<'_, PyAny>, value: &Bound<'_, PyAny>, name: &str) -> PyResult<bool> {
    let method = builtin(module, "isinstance")?;
    method.call1((value, builtin(module, name)?))?.is_truthy()
}

#[pyclass]
struct HostSpaces {
    source: Py<PyIterator>,
}
#[pymethods]
impl HostSpaces {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }
    fn __next__(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        let mut source = self.source.bind(py).clone();
        let Some(item) = source.next() else {
            return Ok(None);
        };
        let result: PyResult<Py<PyAny>> = (|| Ok(item?.call_method0("isspace")?.unbind()))();
        match result {
            Err(error) if error.is_instance_of::<pyo3::exceptions::PyStopIteration>(py) => {
                let outer =
                    pyo3::exceptions::PyRuntimeError::new_err("generator raised StopIteration");
                outer.set_cause(py, Some(error));
                Err(outer)
            }
            result => result.map(Some),
        }
    }
}
#[pyfunction]
pub fn ex_host_normalize<'py>(
    py: Python<'py>,
    value: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    if !is_instance(&module, &value, "str")? {
        return Ok(py.None());
    }
    let text = value.call_method0("strip")?;
    if !text.is_truthy()? {
        return Ok(py.None());
    }
    let any = builtin(&module, "any")?;
    let spaces = Py::new(
        py,
        HostSpaces {
            source: text.try_iter()?.unbind(),
        },
    )?;
    let generator = module.getattr("_native_host_values")?.call1((spaces,))?;
    if any.call1((generator,))?.is_truthy()? {
        return Ok(py.None());
    }
    if text.call_method1("startswith", ("[",))?.is_truthy()? {
        let end = text.call_method1("find", ("]",))?;
        if end.eq(-1)? {
            return Ok(py.None());
        }
        let start = py.import("operator")?.getattr("add")?.call1((&end, 1))?;
        let suffix = text.get_item(py.get_type::<PySlice>().call1((start, py.None()))?)?;
        if suffix.call_method0("strip")?.is_truthy()? {
            return Ok(py.None());
        }
        return Ok(text.unbind());
    }
    if text.contains(":")?
        || text.contains("/")?
        || builtin(&module, "len")?.call1((&text,))?.gt(253)?
    {
        return Ok(py.None());
    }
    Ok(text.unbind())
}
#[pyfunction]
pub fn ex_hosts_parse<'py>(
    py: Python<'py>,
    values: Bound<'py, PyAny>,
    source: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let hosts = PyList::empty(py);
    for raw in values.try_iter()? {
        let raw = raw?;
        py.check_signals()?;
        let host = module.getattr("normalize_ex_host")?.call1((&raw,))?;
        if host.is_none() {
            let constructor = builtin(&module, "ValueError")?;
            let source = formatted(py, &source, "")?;
            let raw = raw.repr()?.into_any();
            let port = formatted(py, &module.getattr("DEFAULT_EX_PORT")?, "")?;
            let message = format(py,"{} 中的主机项无效: {}；只接受裸主机名/IP（例如 '203.0.113.10'），端口由 ExTdxClient(port=) 指定，默认 {}",&[source,raw,port])?;
            return Err(PyErr::from_value(constructor.call1((message,))?));
        }
        if !hosts.contains(&host)? {
            hosts.append(host)?;
        }
    }
    Ok(hosts.into_any().unbind())
}
#[pyfunction]
pub fn ex_hosts_env<'py>(
    py: Python<'py>,
    env: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let env = if env.is_none() {
        module.getattr("os")?.getattr("environ")?
    } else {
        env
    };
    let method = env.getattr("get")?;
    let text = method.call1((module.getattr("EX_HOSTS_ENV")?,))?;
    if !text.is_truthy()? || !text.call_method0("strip")?.is_truthy()? {
        return Ok(PyList::empty(py).into_any().unbind());
    }
    let split = module.getattr("_HOST_SPLIT")?.getattr("split")?;
    let values = split.call1((text.call_method0("strip")?,))?;
    let parts = PyList::empty(py);
    for part in values.try_iter()? {
        let part = part?;
        if part.is_truthy()? {
            parts.append(part)?;
        }
    }
    let kwargs = PyDict::new(py);
    kwargs.set_item("source", module.getattr("EX_HOSTS_ENV")?)?;
    Ok(module
        .getattr("parse_ex_hosts")?
        .call((parts,), Some(&kwargs))?
        .unbind())
}
#[pyfunction]
pub fn ex_hosts_path<'py>(py: Python<'py>, module: Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    let directory = module.getattr("data_dir")?.call0()?;
    Ok(py
        .import("operator")?
        .getattr("truediv")?
        .call1((directory, module.getattr("EX_SERVER_FILE")?))?
        .unbind())
}
#[pyfunction]
pub fn ex_hosts_target<'py>(
    path: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let target = if path.is_none() {
        module.getattr("ex_server_file_path")?.call0()?
    } else {
        module.getattr("Path")?.call1((path,))?
    };
    Ok(target.unbind())
}
#[pyfunction]
pub fn ex_hosts_file_data<'py>(
    py: Python<'py>,
    data: Bound<'py, PyAny>,
    target: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    if !is_instance(&module, &data, "dict")? {
        let constructor = builtin(&module, "ValueError")?;
        let message = format(
            py,
            "扩展行情主机配置顶层必须是对象（{}）",
            &[formatted(py, &target, "")?],
        )?;
        return Err(PyErr::from_value(constructor.call1((message,))?));
    }
    let hosts = data.call_method1("get", ("hosts",))?;
    if !is_instance(&module, &hosts, "list")? {
        let constructor = builtin(&module, "ValueError")?;
        let message = format(
            py,
            "扩展行情主机配置缺少 hosts 数组（{}）",
            &[formatted(py, &target, "")?],
        )?;
        return Err(PyErr::from_value(constructor.call1((message,))?));
    }
    let method = module.getattr("parse_ex_hosts")?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("source", builtin(&module, "str")?.call1((target,))?)?;
    Ok(method.call((hosts,), Some(&kwargs))?.unbind())
}
#[pyfunction]
pub fn ex_hosts_error<'py>(
    py: Python<'py>,
    kind: &str,
    target: Bound<'py, PyAny>,
    cause: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let constructor = builtin(&module, "ValueError")?;
    let target = formatted(py, &target, "")?;
    let cause = formatted(py, &cause, "")?;
    let pattern = if kind == "json" {
        "扩展行情主机配置不是合法 JSON（{}）: {}"
    } else {
        "无法读取扩展行情主机配置 {}: {}"
    };
    Ok(constructor
        .call1((format(py, pattern, &[target, cause])?,))?
        .unbind())
}
#[pyfunction]
pub fn ex_hosts_select<'py>(
    _py: Python<'py>,
    env: Bound<'py, PyAny>,
    path: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let value = eltdx::ex::config::select_host_source_with(
        ["load_ex_hosts_from_env", "load_ex_hosts_from_file"],
        |name| {
            let value = module
                .getattr(name)?
                .call1((if name == "load_ex_hosts_from_env" {
                    &env
                } else {
                    &path
                },))?;
            Ok::<_, PyErr>(if value.is_truthy()? {
                Some(value)
            } else {
                None
            })
        },
        || builtin(&module, "list")?.call1((module.getattr("DEFAULT_EX_HOSTS")?,)),
    )?;
    Ok(value.unbind())
}
#[pyfunction]
pub fn ex_hosts_save_prepare<'py>(
    py: Python<'py>,
    hosts: Bound<'py, PyAny>,
    path: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let parse = module.getattr("parse_ex_hosts")?;
    let values = builtin(&module, "list")?.call1((hosts,))?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("source", "save_ex_hosts")?;
    let normalized = parse.call((values,), Some(&kwargs))?;
    if !normalized.is_truthy()? {
        return Err(error(
            &module,
            "hosts 不能为空".into_pyobject(py)?.into_any(),
        )?);
    }
    let target = if path.is_none() {
        module.getattr("ex_server_file_path")?.call0()?
    } else {
        module.getattr("Path")?.call1((path,))?
    };
    let kwargs = PyDict::new(py);
    kwargs.set_item("parents", true)?;
    kwargs.set_item("exist_ok", true)?;
    target
        .getattr("parent")?
        .call_method("mkdir", (), Some(&kwargs))?;
    let payload = PyDict::new(py);
    payload.set_item(
        "schema_version",
        module.getattr("EX_SERVER_SCHEMA_VERSION")?,
    )?;
    payload.set_item("hosts", normalized)?;
    let with_name = target.getattr("with_name")?;
    let name = formatted(py, &target.getattr("name")?, "")?;
    let pid = formatted(py, &module.getattr("os")?.getattr("getpid")?.call0()?, "")?;
    let ident = formatted(py, &module.getattr("get_ident")?.call0()?, "")?;
    let temporary = with_name.call1((format(py, ".{}.{}.{}.tmp", &[name, pid, ident])?,))?;
    Ok((target, payload, temporary)
        .into_pyobject(py)?
        .into_any()
        .unbind())
}
#[pyfunction]
pub fn ex_hosts_save_write<'py>(
    stream: Bound<'py, PyAny>,
    payload: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<()> {
    let kwargs = PyDict::new(stream.py());
    kwargs.set_item("ensure_ascii", false)?;
    kwargs.set_item("indent", 2)?;
    module
        .getattr("json")?
        .getattr("dump")?
        .call((payload, &stream), Some(&kwargs))?;
    stream.call_method1("write", ("\n",))?;
    stream.call_method0("flush")?;
    let fsync = module.getattr("os")?.getattr("fsync")?;
    fsync.call1((stream.call_method0("fileno")?,))?;
    Ok(())
}
