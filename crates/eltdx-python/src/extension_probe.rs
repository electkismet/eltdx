//! Legacy extension discovery: Python owns executor/context/exception adapters,
//! Rust owns candidates, scheduling traversal, result projection and ranking.
use crate::f10_request::builtin;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyIterator, PyList, PyTuple};

#[pyclass]
pub(crate) struct ReadyProbes {
    source: Py<PyIterator>,
}
#[pymethods]
impl ReadyProbes {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }
    fn __next__(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        let mut source = self.source.bind(py).clone();
        loop {
            let Some(item) = source.next() else {
                return Ok(None);
            };
            let result: PyResult<Option<Py<PyAny>>> = (|| {
                let item = item?;
                if item.getattr("ok")?.is_truthy()? && !item.getattr("latency_ms")?.is_none() {
                    Ok(Some(item.unbind()))
                } else {
                    Ok(None)
                }
            })();
            match result {
                Ok(Some(item)) => return Ok(Some(item)),
                Ok(None) => continue,
                Err(error) if error.is_instance_of::<pyo3::exceptions::PyStopIteration>(py) => {
                    let outer =
                        pyo3::exceptions::PyRuntimeError::new_err("generator raised StopIteration");
                    outer.set_cause(py, Some(error));
                    return Err(outer);
                }
                Err(error) => return Err(error),
            }
        }
    }
}

fn ready_sorted<'py>(
    py: Python<'py>,
    results: &Bound<'py, PyAny>,
    module: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let sorted = builtin(module, "sorted")?;
    let values = Py::new(
        py,
        ReadyProbes {
            source: results.try_iter()?.unbind(),
        },
    )?;
    let generator = module.getattr("_native_probe_values")?.call1((values,))?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("key", module.getattr("_native_probe_latency")?)?;
    sorted.call((generator,), Some(&kwargs))
}

#[pyfunction]
pub fn extension_probe_key<'py>(
    result: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let value = if !result.getattr("latency_ms")?.is_none() {
        result.getattr("latency_ms")?
    } else {
        builtin(&module, "float")?.call1(("inf",))?
    };
    Ok(value.unbind())
}

#[pyfunction]
pub fn extension_probe_default<'py>(
    operation: &str,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(if operation == "first" {
        module.getattr("DEFAULT_EX_HOSTS")?.get_item(0)?
    } else {
        module.getattr("GetExMarketsCmd")?.call0()?
    }
    .unbind())
}

#[pyfunction]
pub fn extension_registry_init<'py>(
    py: Python<'py>,
    registry: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<()> {
    registry.setattr(
        "_lock",
        module.getattr("threading")?.getattr("Lock")?.call0()?,
    )?;
    registry.setattr("_connections", PyList::empty(py))
}

#[pyfunction]
pub fn extension_registry_change<'py>(
    operation: &str,
    registry: Bound<'py, PyAny>,
    connection: Bound<'py, PyAny>,
) -> PyResult<()> {
    registry.getattr("_connections")?.call_method1(
        if operation == "add" {
            "append"
        } else {
            "remove"
        },
        (connection,),
    )?;
    Ok(())
}

#[pyfunction]
pub fn extension_registry_take<'py>(
    py: Python<'py>,
    registry: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let connections = registry.getattr("_connections")?;
    registry.setattr("_connections", PyList::empty(py))?;
    Ok(connections.unbind())
}

#[pyfunction]
pub fn extension_registry_abort(connections: Bound<'_, PyAny>) -> PyResult<()> {
    for connection in connections.try_iter()? {
        connection?.call_method0("close")?;
    }
    Ok(())
}

#[pyfunction]
pub fn extension_probe_begin<'py>(
    py: Python<'py>,
    domain: &str,
    args: Bound<'py, PyTuple>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let started = module.getattr("time")?.call_method0("monotonic")?;
    let constructor = module.getattr(if domain == "ex" {
        "ExTdxConnection"
    } else {
        "MacConnection"
    })?;
    let kwargs = PyDict::new(py);
    kwargs.set_item(
        if domain == "ex" {
            "mac_ex_mode"
        } else {
            "socket_factory"
        },
        args.get_item(3)?,
    )?;
    let connection = constructor.call(
        (args.get_item(0)?, args.get_item(1)?, args.get_item(2)?),
        Some(&kwargs),
    )?;
    if domain == "ex" && !args.get_item(4)?.is_none() {
        args.get_item(4)?.call_method1("add", (&connection,))?;
    }
    Ok((started, connection).into_pyobject(py)?.into_any().unbind())
}

#[pyfunction]
pub fn extension_probe_run<'py>(
    connection: Bound<'py, PyAny>,
    factory: Bound<'py, PyAny>,
    mac: bool,
) -> PyResult<bool> {
    connection.call_method0("connect")?;
    let execute = connection.getattr("execute")?;
    let result = execute.call1((factory.call0()?,))?;
    if mac {
        result.getattr("today")?.is_truthy()
    } else {
        Ok(true)
    }
}

#[pyfunction]
pub fn extension_probe_close(
    connection: Bound<'_, PyAny>,
    registry: Bound<'_, PyAny>,
) -> PyResult<()> {
    if !registry.is_none() {
        registry.call_method1("discard", (&connection,))?;
    }
    connection.call_method0("close")?;
    Ok(())
}

#[pyfunction]
pub fn extension_probe_error<'py>(
    py: Python<'py>,
    host: Bound<'py, PyAny>,
    cause: Bound<'py, PyAny>,
    detailed: bool,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let constructor = module.getattr("ExHostProbe")?;
    let name = builtin(&module, "type")?
        .call1((&cause,))?
        .getattr("__name__")?;
    let reason = if detailed {
        let name = "{}".into_pyobject(py)?.call_method1("format", (name,))?;
        let cause = "{}".into_pyobject(py)?.call_method1("format", (cause,))?;
        "{}: {}"
            .into_pyobject(py)?
            .call_method1("format", (name, cause))?
    } else {
        name
    };
    let kwargs = PyDict::new(py);
    kwargs.set_item("host", host)?;
    kwargs.set_item("ok", false)?;
    kwargs.set_item("error", reason)?;
    Ok(constructor.call((), Some(&kwargs))?.unbind())
}

#[pyfunction]
pub fn extension_probe_success<'py>(
    py: Python<'py>,
    domain: &str,
    started: Bound<'py, PyAny>,
    host: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let constructor = if domain == "ex" {
        Some(module.getattr("ExHostProbe")?)
    } else {
        None
    };
    let round = if domain == "ex" {
        Some(builtin(&module, "round")?)
    } else {
        None
    };
    let now = module.getattr("time")?.call_method0("monotonic")?;
    let op = py.import("operator")?;
    let duration = op.getattr("sub")?.call1((now, started))?;
    if let (Some(constructor), Some(round)) = (constructor, round) {
        let latency = round.call1((op.getattr("mul")?.call1((duration, 1000.0))?, 2))?;
        let kwargs = PyDict::new(py);
        kwargs.set_item("host", host)?;
        kwargs.set_item("ok", true)?;
        kwargs.set_item("latency_ms", latency)?;
        Ok(constructor.call((), Some(&kwargs))?.unbind())
    } else {
        Ok(duration.unbind())
    }
}

#[pyfunction]
pub fn extension_probe_prepare<'py>(
    py: Python<'py>,
    domain: &str,
    hosts: Bound<'py, PyAny>,
    max_workers: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let candidates = if domain == "ex" {
        if hosts.is_none() {
            module.getattr("load_ex_hosts")?.call0()?
        } else {
            builtin(&module, "list")?.call1((hosts,))?
        }
    } else if hosts.is_truthy()? {
        let out = PyList::empty(py);
        for host in hosts.try_iter()? {
            out.append(builtin(&module, "str")?.call1((host?,))?)?;
        }
        out.into_any()
    } else {
        let list = builtin(&module, "list")?;
        list.call1((module.getattr("MAC_HOSTS")?,))?
    };
    if !candidates.is_truthy()? {
        return Ok(py.None());
    }
    let results = if domain == "ex" {
        PyDict::new(py).into_any()
    } else {
        PyList::empty(py).into_any()
    };
    let (registry, pool) = if domain == "ex" {
        let min = builtin(&module, "min")?;
        let max = builtin(&module, "max")?;
        let workers = min.call1((
            max.call1((1, builtin(&module, "int")?.call1((max_workers,))?))?,
            builtin(&module, "len")?.call1((&candidates,))?,
        ))?;
        let registry = module.getattr("_ProbeRegistry")?.call0()?;
        let constructor = module.getattr("ThreadPoolExecutor")?;
        let kwargs = PyDict::new(py);
        kwargs.set_item("max_workers", workers)?;
        kwargs.set_item("thread_name_prefix", "eltdx-ex-probe")?;
        (registry, constructor.call((), Some(&kwargs))?)
    } else {
        let constructor = module.getattr("ThreadPoolExecutor")?;
        let kwargs = PyDict::new(py);
        kwargs.set_item(
            "max_workers",
            builtin(&module, "len")?.call1((&candidates,))?,
        )?;
        (
            py.None().into_bound(py),
            constructor.call((), Some(&kwargs))?,
        )
    };
    Ok((candidates, results, registry, pool)
        .into_pyobject(py)?
        .into_any()
        .unbind())
}

#[pyfunction]
pub fn extension_probe_collect<'py>(
    py: Python<'py>,
    domain: &str,
    candidates: Bound<'py, PyAny>,
    results: Bound<'py, PyAny>,
    registry: Bound<'py, PyAny>,
    pool: Bound<'py, PyAny>,
    args: Bound<'py, PyTuple>,
    module: Bound<'py, PyAny>,
) -> PyResult<()> {
    let futures = PyDict::new(py);
    for host in candidates.try_iter()? {
        let host = host?;
        let submit = pool.getattr("submit")?;
        let probe = module.getattr(if domain == "ex" {
            "probe_ex_host"
        } else {
            "ping_mac_host"
        })?;
        let kwargs = PyDict::new(py);
        if domain == "ex" {
            kwargs.set_item("command_factory", args.get_item(2)?)?;
            kwargs.set_item("mac_ex_mode", args.get_item(3)?)?;
            kwargs.set_item("registry", &registry)?;
        } else {
            kwargs.set_item("socket_factory", args.get_item(2)?)?;
        }
        let future = submit.call(
            (probe, &host, args.get_item(0)?, args.get_item(1)?),
            Some(&kwargs),
        )?;
        futures.set_item(future, host)?;
    }
    for future in module
        .getattr("as_completed")?
        .call1((&futures,))?
        .try_iter()?
    {
        let future = future?;
        if domain == "ex" {
            let host = futures.as_any().get_item(&future)?;
            let result = module
                .getattr("_native_probe_future_result")?
                .call1((&future, &host))?;
            results.set_item(host, &result)?;
            if args.get_item(4)?.is_truthy()? && result.getattr("ok")?.is_truthy()? {
                registry.call_method0("abort_all")?;
                break;
            }
        } else {
            let latency = future.call_method0("result")?;
            if !latency.is_none() {
                let append = results.getattr("append")?;
                let host = futures.as_any().get_item(&future)?;
                append.call1(((host, latency),))?;
            }
        }
    }
    Ok(())
}

#[pyfunction]
pub fn extension_probe_cleanup(
    py: Python<'_>,
    registry: Bound<'_, PyAny>,
    pool: Bound<'_, PyAny>,
) -> PyResult<()> {
    registry.call_method0("abort_all")?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("wait", true)?;
    kwargs.set_item("cancel_futures", true)?;
    pool.call_method("shutdown", (), Some(&kwargs))?;
    Ok(())
}

#[pyfunction]
pub fn extension_probe_finish<'py>(
    py: Python<'py>,
    domain: &str,
    candidates: Bound<'py, PyAny>,
    results: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    if domain == "mac" {
        let sort = results.getattr("sort")?;
        let kwargs = PyDict::new(py);
        kwargs.set_item("key", module.getattr("_native_probe_pair_latency")?)?;
        sort.call((), Some(&kwargs))?;
        return Ok(results.unbind());
    }
    let out = PyList::empty(py);
    for host in candidates.try_iter()? {
        let host = host?;
        let get = results.getattr("get")?;
        let constructor = module.getattr("ExHostProbe")?;
        let kwargs = PyDict::new(py);
        kwargs.set_item("host", &host)?;
        kwargs.set_item("ok", false)?;
        kwargs.set_item("error", "not probed")?;
        let default = constructor.call((), Some(&kwargs))?;
        out.append(get.call1((host, default))?)?;
    }
    Ok(out.into_any().unbind())
}

#[pyfunction]
pub fn extension_probe_rank<'py>(
    py: Python<'py>,
    results: Bound<'py, PyAny>,
    candidates: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let ordered = if !candidates.is_none() {
        candidates
    } else {
        let out = PyList::empty(py);
        for result in results.try_iter()? {
            out.append(result?.getattr("host")?)?;
        }
        out.into_any()
    };
    let healthy = ready_sorted(py, &results, &module)?;
    let hosts = PyList::empty(py);
    for result in healthy.try_iter()? {
        hosts.append(result?.getattr("host")?)?;
    }
    let seen = builtin(&module, "set")?.call1((&hosts,))?;
    let mut tail = PyList::empty(py);
    eltdx::ex::legacy::append_rank_tail_with(
        &mut tail,
        ordered.try_iter()?,
        |_, host| seen.contains(host),
        |tail, host| tail.append(host),
    )?;
    Ok(py
        .import("operator")?
        .getattr("add")?
        .call1((hosts, tail))?
        .unbind())
}

#[pyfunction]
pub fn extension_probe_ping<'py>(
    py: Python<'py>,
    all: bool,
    args: Bound<'py, PyTuple>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let probe = module.getattr(if all {
        "probe_ex_hosts"
    } else {
        "probe_ex_host"
    })?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("command_factory", args.get_item(3)?)?;
    kwargs.set_item("mac_ex_mode", args.get_item(4)?)?;
    let results = probe.call(
        (args.get_item(0)?, args.get_item(1)?, args.get_item(2)?),
        Some(&kwargs),
    )?;
    let divide = py.import("operator")?.getattr("truediv")?;
    if !all {
        if !results.getattr("ok")?.is_truthy()? || results.getattr("latency_ms")?.is_none() {
            return Ok(py.None());
        }
        return Ok(divide
            .call1((results.getattr("latency_ms")?, 1000.0))?
            .unbind());
    }
    let healthy = ready_sorted(py, &results, &module)?;
    let out = PyList::empty(py);
    for result in healthy.try_iter()? {
        let result = result?;
        if !result.getattr("latency_ms")?.is_none() {
            out.append((
                result.getattr("host")?,
                divide.call1((result.getattr("latency_ms")?, 1000.0))?,
            ))?;
        }
    }
    Ok(out.into_any().unbind())
}
