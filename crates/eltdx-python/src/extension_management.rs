//! Python extension client management. Exception scopes stay in explicit
//! Python adapters; selection, retry and publication policy live here.
use crate::f10_request::builtin;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PySet, PyTuple};

fn error(module: &Bound<'_, PyAny>, kind: &str, text: &str) -> PyResult<PyErr> {
    Ok(PyErr::from_value(builtin(module, kind)?.call1((text,))?))
}

fn transport_error(module: &Bound<'_, PyAny>, text: &str) -> PyResult<PyErr> {
    Ok(PyErr::from_value(
        module.getattr("TransportError")?.call1((text,))?,
    ))
}

fn prepend<'py>(
    py: Python<'py>,
    first: &Bound<'py, PyAny>,
    values: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let tail = PyList::empty(py);
    for value in values.try_iter()? {
        let value = value?;
        if value.ne(first)? {
            tail.append(value)?;
        }
    }
    let out = PyList::empty(py);
    out.append(first)?;
    for value in tail.iter() {
        out.append(value)?;
    }
    Ok(out.into_any())
}

fn string_hosts<'py>(
    py: Python<'py>,
    hosts: &Bound<'py, PyAny>,
    module: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let out = PyList::empty(py);
    for host in hosts.try_iter()? {
        out.append(builtin(module, "str")?.call1((host?,))?)?;
    }
    Ok(out.into_any())
}

fn candidates<'py>(
    py: Python<'py>,
    domain: &str,
    hosts: &Bound<'py, PyAny>,
    module: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    if domain == "ex" {
        if hosts.is_none() {
            module.getattr("load_ex_hosts")?.call0()
        } else {
            builtin(module, "list")?.call1((hosts,))
        }
    } else if hosts.is_truthy()? {
        if domain == "mac" {
            string_hosts(py, hosts, module)
        } else {
            builtin(module, "list")?.call1((hosts,))
        }
    } else {
        let list = builtin(module, "list")?;
        list.call1((module.getattr(if domain == "mac" {
            "MAC_HOSTS"
        } else {
            "MAC_EX_HOSTS"
        })?,))
    }
}

fn ex_connection<'py>(
    client: &Bound<'py, PyAny>,
    host: &Bound<'py, PyAny>,
    module: &Bound<'py, PyAny>,
    mac_ex: bool,
) -> PyResult<Bound<'py, PyAny>> {
    let constructor = module.getattr("ExTdxConnection")?;
    let port = client.getattr("_port")?;
    let timeout = client.getattr("_timeout")?;
    if mac_ex {
        let kwargs = PyDict::new(client.py());
        kwargs.set_item("mac_ex_mode", true)?;
        constructor.call((host, port, timeout), Some(&kwargs))
    } else {
        constructor.call1((host, port, timeout))
    }
}

#[pyfunction]
pub fn extension_client_init<'py>(
    py: Python<'py>,
    domain: &str,
    client: Bound<'py, PyAny>,
    args: Bound<'py, PyTuple>,
    module: Bound<'py, PyAny>,
) -> PyResult<()> {
    let host = args.get_item(0)?;
    let port = args.get_item(1)?;
    let timeout = args.get_item(2)?;
    let hosts = args.get_item(3)?;
    if domain == "mac" {
        let selected = if !hosts.is_none() {
            string_hosts(py, &hosts, &module)?
        } else if !host.is_none() {
            let out = PyList::empty(py);
            out.append(&host)?;
            out.into_any()
        } else {
            let list = builtin(&module, "list")?;
            list.call1((module.getattr("MAC_HOSTS")?,))?
        };
        client.setattr("_hosts", selected)?;
        if !client.getattr("_hosts")?.is_truthy()? {
            return Err(error(&module, "ValueError", "至少需要一个 host")?);
        }
        client.setattr(
            "_port",
            if port.is_none() {
                module.getattr("MAC_PORT")?
            } else {
                port
            },
        )?;
        client.setattr(
            "_timeout",
            if timeout.is_none() {
                module.getattr("MAC_TIMEOUT")?
            } else {
                timeout
            },
        )?;
        client.setattr("_socket_factory", args.get_item(4)?)?;
        client.setattr("_xdxr_provider", args.get_item(5)?)?;
        client.setattr("_host_index", 0)?;
        client.setattr("_conn", py.None())?;
        return Ok(());
    }
    let mut selected = candidates(py, domain, &hosts, &module)?;
    if !host.is_none() {
        selected = prepend(py, &host, &selected)?;
    }
    if !selected.is_truthy()? {
        return Err(error(&module, "ValueError", "hosts 不能为空")?);
    }
    client.setattr("_hosts", &selected)?;
    if domain == "ex" {
        client.setattr("_port", &port)?;
        client.setattr("_timeout", &timeout)?;
        client.setattr("_auto_failover", args.get_item(4)?)?;
        client.setattr("_probe_on_connect", args.get_item(5)?)?;
        let probe_timeout = args.get_item(6)?;
        let probe_timeout = if probe_timeout.is_none() {
            let min = builtin(&module, "min")?;
            min.call1((module.getattr("DEFAULT_PROBE_TIMEOUT")?, &timeout))?
        } else {
            probe_timeout
        };
        client.setattr("_probe_timeout", probe_timeout)?;
        client.setattr("_max_probe_workers", args.get_item(7)?)?;
        client.setattr("_preferred_host", &host)?;
        client.setattr(
            "_host",
            if host.is_none() {
                selected.get_item(0)?
            } else {
                host
            },
        )?;
        // The original constructor uses its captured arguments, not attributes
        // possibly changed by a subclass's __setattr__.
        let constructor = module.getattr("ExTdxConnection")?;
        client.setattr(
            "_conn",
            constructor.call1((client.getattr("_host")?, port, timeout))?,
        )?;
        client.setattr("_connected", false)?;
        client.setattr("_ordered", builtin(&module, "list")?.call1((&selected,))?)?;
        client.setattr("_healthy", builtin(&module, "set")?.call0()?)?;
        client.setattr("_unhealthy", PyDict::new(py))?;
        client.setattr("_probed", false)?;
    } else {
        client.setattr(
            "_host",
            if host.is_none() {
                selected.get_item(0)?
            } else {
                host
            },
        )?;
        client.setattr("_port", &port)?;
        client.setattr("_timeout", &timeout)?;
        let constructor = module.getattr("ExTdxConnection")?;
        let kwargs = PyDict::new(py);
        kwargs.set_item("mac_ex_mode", true)?;
        client.setattr(
            "_conn",
            constructor.call((client.getattr("_host")?, port, timeout), Some(&kwargs))?,
        )?;
        client.setattr("_connected", false)?;
        client.setattr("_bounds_cache", PyDict::new(py))?;
        client.setattr("_macro_cache", PyDict::new(py))?;
    }
    Ok(())
}

#[pyfunction]
pub fn extension_client_best<'py>(
    py: Python<'py>,
    domain: &str,
    cls: Bound<'py, PyAny>,
    args: Bound<'py, PyTuple>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let hosts = args.get_item(0)?;
    let port = args.get_item(1)?;
    let timeout = args.get_item(2)?;
    let ping_timeout = args.get_item(3)?;
    if domain == "mac" {
        let ping = module.getattr("ping_mac_all")?;
        let probe_port = if port.is_none() {
            module.getattr("MAC_PORT")?
        } else {
            port.clone()
        };
        let ranked = ping.call1((&hosts, probe_port, ping_timeout))?;
        let selected = candidates(py, domain, &hosts, &module)?;
        let best = if ranked.is_truthy()? {
            ranked.get_item(0)?.get_item(0)?
        } else if selected.is_truthy()? {
            selected.get_item(0)?
        } else {
            py.None().into_bound(py)
        };
        if best.is_none() {
            return Err(error(&module, "ValueError", "hosts 不能为空")?);
        }
        let ordered = prepend(py, &best, &selected)?;
        let kwargs = PyDict::new(py);
        kwargs.set_item("hosts", ordered)?;
        return Ok(cls.call((best, port, timeout), Some(&kwargs))?.unbind());
    }
    let selected = candidates(py, domain, &hosts, &module)?;
    let probe = module.getattr(if domain == "ex" {
        "probe_ex_hosts"
    } else {
        "ping_ex_all"
    })?;
    let result = if domain == "ex" {
        probe.call1((&selected, &port, ping_timeout))?
    } else {
        let kwargs = PyDict::new(py);
        kwargs.set_item("mac_ex_mode", true)?;
        kwargs.set_item("command_factory", module.getattr("MacExLoginCmd")?)?;
        probe.call((&selected, &port, ping_timeout), Some(&kwargs))?
    };
    let ordered = if domain == "ex" {
        let rank = module.getattr("rank_ex_hosts")?;
        let kwargs = PyDict::new(py);
        kwargs.set_item("candidates", &selected)?;
        rank.call((&result,), Some(&kwargs))?
    } else {
        result.clone()
    };
    let best = if ordered.is_truthy()? {
        if domain == "ex" {
            ordered.get_item(0)?
        } else {
            ordered.get_item(0)?.get_item(0)?
        }
    } else if domain == "ex" {
        py.None().into_bound(py)
    } else {
        selected.get_item(0)?
    };
    let kwargs = PyDict::new(py);
    kwargs.set_item("hosts", selected)?;
    if domain == "ex" {
        kwargs.set_item("probe_on_connect", false)?;
    }
    let client = cls.call((best, port, timeout), Some(&kwargs))?;
    if domain == "ex" {
        client.call_method1("_seed_from_probe", (result,))?;
    }
    Ok(client.unbind())
}

#[pyfunction]
pub fn extension_client_ping<'py>(
    py: Python<'py>,
    domain: &str,
    args: Bound<'py, PyTuple>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let ping = module.getattr(if domain == "mac" {
        "ping_mac_all"
    } else {
        "ping_ex_all"
    })?;
    let mut hosts = args.get_item(0)?;
    let mut port = args.get_item(1)?;
    let timeout = args.get_item(2)?;
    if domain == "mac" && port.is_none() {
        port = module.getattr("MAC_PORT")?;
    }
    if domain == "mac_ex" {
        if !hosts.is_truthy()? {
            let list = builtin(&module, "list")?;
            hosts = list.call1((module.getattr("MAC_EX_HOSTS")?,))?;
        }
        let kwargs = PyDict::new(py);
        kwargs.set_item("mac_ex_mode", true)?;
        kwargs.set_item("command_factory", module.getattr("MacExLoginCmd")?)?;
        Ok(ping.call((hosts, port, timeout), Some(&kwargs))?.unbind())
    } else {
        Ok(ping.call1((hosts, port, timeout))?.unbind())
    }
}

#[pyfunction]
pub fn extension_client_state<'py>(
    py: Python<'py>,
    domain: &str,
    operation: &str,
    client: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    match operation {
        "host" if domain == "mac" => Ok(client
            .getattr("_hosts")?
            .get_item(client.getattr("_host_index")?)?
            .unbind()),
        "host" => Ok(client.getattr("_host")?.unbind()),
        "hosts" => Ok(builtin(&module, "list")?
            .call1((client.getattr("_hosts")?,))?
            .unbind()),
        "healthy" => {
            let out = PyList::empty(py);
            for host in client.getattr("_ordered")?.try_iter()? {
                let host = host?;
                if client.getattr("_healthy")?.contains(&host)?
                    && !client.getattr("_unhealthy")?.contains(&host)?
                {
                    out.append(host)?;
                }
            }
            Ok(out.into_any().unbind())
        }
        "unhealthy" => Ok(builtin(&module, "dict")?
            .call1((client.getattr("_unhealthy")?,))?
            .unbind()),
        "order" if domain == "mac_ex" => {
            // The first host is captured before the comprehension, while its
            // comparisons deliberately reread _host for every candidate.
            let first = client.getattr("_host")?;
            let tail = PyList::empty(py);
            for host in client.getattr("_hosts")?.try_iter()? {
                let host = host?;
                if host.ne(client.getattr("_host")?)? {
                    tail.append(host)?;
                }
            }
            let out = PyList::empty(py);
            out.append(first)?;
            for host in tail.iter() {
                out.append(host)?;
            }
            Ok(out.into_any().unbind())
        }
        "order" => {
            let selected = PyList::empty(py);
            for host in client.getattr("_ordered")?.try_iter()? {
                let host = host?;
                if !client.getattr("_unhealthy")?.contains(&host)? {
                    selected.append(host)?;
                }
            }
            let mut selected = selected.into_any();
            if !selected.is_truthy()? {
                client.getattr("_unhealthy")?.call_method0("clear")?;
                selected = builtin(&module, "list")?.call1((client.getattr("_ordered")?,))?;
            }
            if !client.getattr("_preferred_host")?.is_none()
                && selected.contains(client.getattr("_preferred_host")?)?
                && selected
                    .get_item(0)?
                    .ne(client.getattr("_preferred_host")?)?
            {
                selected = prepend_live_preferred(py, &client, &selected)?;
            }
            Ok(selected.unbind())
        }
        "close" => {
            if domain != "mac" || !client.getattr("_conn")?.is_none() {
                client.getattr("_conn")?.call_method0("close")?;
                if domain == "mac" {
                    client.setattr("_conn", py.None())?;
                } else {
                    client.setattr("_connected", false)?;
                }
            }
            Ok(py.None())
        }
        "disconnect" | "exit" => {
            client.call_method0("close")?;
            Ok(py.None())
        }
        "enter" => {
            client.call_method0("connect")?;
            Ok(client.unbind())
        }
        "ensure" => {
            if domain == "mac" {
                let execute = client.getattr("_execute")?;
                execute.call1((module.getattr("KlineOffsetCmd")?.call1((0, 1))?,))?;
            } else if !client.getattr("_connected")?.is_truthy()? {
                client.call_method0("connect")?;
            }
            Ok(py.None())
        }
        _ => Err(error(
            &module,
            "ValueError",
            "unknown extension client management operation",
        )?),
    }
}

fn prepend_live_preferred<'py>(
    py: Python<'py>,
    client: &Bound<'py, PyAny>,
    values: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let first = client.getattr("_preferred_host")?;
    let tail = PyList::empty(py);
    for host in values.try_iter()? {
        let host = host?;
        if host.ne(client.getattr("_preferred_host")?)? {
            tail.append(host)?;
        }
    }
    let out = PyList::empty(py);
    out.append(first)?;
    for host in tail.iter() {
        out.append(host)?;
    }
    Ok(out.into_any())
}

fn healthy_from<'py>(py: Python<'py>, results: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    let healthy = PySet::empty(py)?;
    for result in results.try_iter()? {
        let result = result?;
        if result.getattr("ok")?.is_truthy()? {
            healthy.add(result.getattr("host")?)?;
        }
    }
    Ok(healthy.into_any())
}

#[pyfunction]
pub fn extension_client_probe<'py>(
    py: Python<'py>,
    operation: &str,
    client: Bound<'py, PyAny>,
    args: Bound<'py, PyTuple>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    if operation == "seed" {
        let results = args.get_item(0)?;
        let rank = module.getattr("rank_ex_hosts")?;
        let kwargs = PyDict::new(py);
        kwargs.set_item("candidates", client.getattr("_hosts")?)?;
        client.setattr("_ordered", rank.call((&results,), Some(&kwargs))?)?;
        client.setattr("_healthy", healthy_from(py, &results)?)?;
        client.getattr("_unhealthy")?.call_method0("clear")?;
        client.setattr("_probed", true)?;
        return Ok(py.None());
    }
    if operation == "refresh" {
        let probe = client.getattr("_probe_and_seed")?;
        let kwargs = PyDict::new(py);
        kwargs.set_item("timeout", args.get_item(0)?)?;
        kwargs.set_item("max_workers", args.get_item(1)?)?;
        return Ok(probe.call((), Some(&kwargs))?.unbind());
    }
    let probe = if operation == "remaining" {
        None
    } else {
        Some(module.getattr("probe_ex_hosts")?)
    };
    let hosts = if operation == "remaining" {
        let remaining = PyList::empty(py);
        for host in client.getattr("_hosts")?.try_iter()? {
            let host = host?;
            if !client.getattr("_unhealthy")?.contains(&host)? {
                remaining.append(host)?;
            }
        }
        if builtin(&module, "len")?.call1((&remaining,))?.le(1)? {
            return Ok(py.None());
        }
        remaining.into_any()
    } else {
        client.getattr("_hosts")?
    };
    let probe = match probe {
        Some(probe) => probe,
        None => module.getattr("probe_ex_hosts")?,
    };
    let port = client.getattr("_port")?;
    let timeout = if operation == "remaining" {
        client.getattr("_probe_timeout")?
    } else {
        let value = args.get_item(0)?;
        if value.is_none() {
            client.getattr("_probe_timeout")?
        } else {
            value
        }
    };
    let workers = if operation == "remaining" {
        client.getattr("_max_probe_workers")?
    } else {
        let value = args.get_item(1)?;
        if value.is_none() {
            client.getattr("_max_probe_workers")?
        } else {
            value
        }
    };
    let kwargs = PyDict::new(py);
    kwargs.set_item("max_workers", workers)?;
    kwargs.set_item(
        "stop_after_first",
        if operation == "remaining" {
            true.into_pyobject(py)?.to_owned().into_any()
        } else {
            args.get_item(2)?
        },
    )?;
    let results = probe.call((hosts, port, timeout), Some(&kwargs))?;
    if operation == "remaining" {
        let rank = module.getattr("rank_ex_hosts")?;
        let kwargs = PyDict::new(py);
        kwargs.set_item("candidates", client.getattr("_hosts")?)?;
        client.setattr("_ordered", rank.call((&results,), Some(&kwargs))?)?;
        client.setattr("_healthy", healthy_from(py, &results)?)?;
        Ok(py.None())
    } else {
        client.call_method1("_seed_from_probe", (&results,))?;
        Ok(results.unbind())
    }
}

#[pyfunction]
pub fn extension_client_connect_one<'py>(
    domain: &str,
    client: Bound<'py, PyAny>,
    target: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let conn = if domain == "mac" {
        target
    } else {
        ex_connection(&client, &target, &module, domain == "mac_ex")?
    };
    if domain == "mac_ex" {
        let previous = client.getattr("_conn")?;
        client.setattr("_conn", &conn)?;
        if !previous.is(&conn) {
            crate::extension_lifecycle::retire(&previous)?;
        }
        client.getattr("_conn")?.call_method0("connect")?;
        let execute = client.getattr("_conn")?.getattr("execute")?;
        execute.call1((module.getattr("MacExLoginCmd")?.call0()?,))?;
    } else {
        conn.call_method0("connect")?;
    }
    Ok(conn.unbind())
}

fn record_failure(
    client: &Bound<'_, PyAny>,
    host: &Bound<'_, PyAny>,
    cause: &Bound<'_, PyAny>,
    module: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let name = builtin(module, "type")?
        .call1((cause,))?
        .getattr("__name__")?;
    let name = "{}"
        .into_pyobject(client.py())?
        .call_method1("format", (name,))?;
    let reason = "{}"
        .into_pyobject(client.py())?
        .call_method1("format", (cause,))?;
    let reason = "{}: {}"
        .into_pyobject(client.py())?
        .call_method1("format", (name, reason))?;
    client.getattr("_unhealthy")?.set_item(host, reason)
}

#[pyfunction]
pub fn extension_client_connect_failed<'py>(
    domain: &str,
    client: Bound<'py, PyAny>,
    target: Bound<'py, PyAny>,
    cause: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<()> {
    if domain == "mac" {
        target.call_method0("close")?;
    } else if domain == "ex" {
        record_failure(&client, &target, &cause, &module)?;
    }
    Ok(())
}

#[pyfunction]
pub fn extension_client_connect<'py>(
    py: Python<'py>,
    domain: &str,
    client: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<()> {
    if domain == "ex"
        && client.getattr("_auto_failover")?.is_truthy()?
        && client.getattr("_probe_on_connect")?.is_truthy()?
        && !client.getattr("_probed")?.is_truthy()?
        && builtin(&module, "len")?
            .call1((client.getattr("_hosts")?,))?
            .gt(1)?
    {
        let probe = client.getattr("_probe_and_seed")?;
        let kwargs = PyDict::new(py);
        kwargs.set_item("stop_after_first", true)?;
        probe.call((), Some(&kwargs))?;
    }
    let mut last = py.None().into_bound(py);
    let values = if domain == "mac" {
        let range = builtin(&module, "range")?;
        range.call1((builtin(&module, "len")?.call1((client.getattr("_hosts")?,))?,))?
    } else {
        client.call_method0(if domain == "ex" {
            "_candidate_order"
        } else {
            "_ordered_hosts"
        })?
    };
    for value in values.try_iter()? {
        let value = value?;
        let (target, index) = if domain == "mac" {
            let op = py.import("operator")?;
            let added = op
                .getattr("add")?
                .call1((client.getattr("_host_index")?, &value))?;
            let index = op.getattr("mod")?.call1((
                added,
                builtin(&module, "len")?.call1((client.getattr("_hosts")?,))?,
            ))?;
            // The MAC constructor is deliberately outside the try adapter.
            let constructor = module.getattr("MacConnection")?;
            let host = client.getattr("_hosts")?.get_item(&index)?;
            let port = client.getattr("_port")?;
            let timeout = client.getattr("_timeout")?;
            let kwargs = PyDict::new(py);
            kwargs.set_item("socket_factory", client.getattr("_socket_factory")?)?;
            (
                constructor.call((host, port, timeout), Some(&kwargs))?,
                Some(index),
            )
        } else {
            (value.clone(), None)
        };
        let outcome = module
            .getattr("_native_connect_attempt")?
            .call1((&client, domain, &target))?;
        if !outcome.get_item(0)?.is_truthy()? {
            last = outcome.get_item(1)?;
            continue;
        }
        let conn = outcome.get_item(1)?;
        if domain == "mac" {
            let previous = client.getattr("_conn")?;
            client.setattr("_host_index", index.expect("MAC index"))?;
            client.setattr("_conn", &conn)?;
            if !previous.is_none() && !previous.is(&conn) {
                previous.call_method0("close")?;
            }
        } else {
            if domain == "ex" {
                let previous = client.getattr("_conn")?;
                client.setattr("_conn", &conn)?;
                if !previous.is(&conn) {
                    crate::extension_lifecycle::retire(&previous)?;
                }
            }
            client.setattr("_host", value)?;
            client.setattr("_connected", true)?;
        }
        return Ok(());
    }
    if if domain == "mac" {
        last.is_truthy()?
    } else {
        !last.is_none()
    } {
        return Err(PyErr::from_value(last));
    }
    Err(transport_error(
        &module,
        if domain == "mac" {
            "无可用 MAC 服务器"
        } else {
            "没有可用主机"
        },
    )?)
}

fn assert_conn(
    client: &Bound<'_, PyAny>,
    module: &Bound<'_, PyAny>,
    enabled: bool,
) -> PyResult<()> {
    if enabled && client.getattr("_conn")?.is_none() {
        return Err(PyErr::from_value(
            builtin(module, "AssertionError")?.call0()?,
        ));
    }
    Ok(())
}

#[pyfunction]
pub fn extension_client_reconnect(client: Bound<'_, PyAny>) -> PyResult<()> {
    client.call_method0("_reprobe_remaining")?;
    client.call_method0("connect")?;
    Ok(())
}

#[pyfunction]
pub fn extension_client_recover<'py>(
    py: Python<'py>,
    domain: &str,
    client: Bound<'py, PyAny>,
    command: Bound<'py, PyAny>,
    cause: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
    assert_enabled: bool,
) -> PyResult<Py<PyAny>> {
    if domain == "ex" {
        let discard = client.getattr("_healthy")?.getattr("discard")?;
        discard.call1((client.getattr("_host")?,))?;
        // f-string is evaluated before the assignment target's live _host.
        let name = builtin(&module, "type")?
            .call1((&cause,))?
            .getattr("__name__")?;
        let name = "{}".into_pyobject(py)?.call_method1("format", (name,))?;
        let reason = "{}".into_pyobject(py)?.call_method1("format", (&cause,))?;
        let reason = "{}: {}"
            .into_pyobject(py)?
            .call_method1("format", (name, reason))?;
        client
            .getattr("_unhealthy")?
            .set_item(client.getattr("_host")?, reason)?;
        client.getattr("_conn")?.call_method0("close")?;
        client.setattr("_connected", false)?;
        let outcome = module
            .getattr("_native_reconnect_attempt")?
            .call1((&client,))?;
        let again = outcome.get_item(0)?.is_truthy()?;
        let last = if again { cause } else { outcome.get_item(1)? };
        return Ok((false, last, again).into_pyobject(py)?.into_any().unbind());
    }
    if domain == "mac" {
        client.call_method0("close")?;
        let op = py.import("operator")?;
        let added = op
            .getattr("add")?
            .call1((client.getattr("_host_index")?, 1))?;
        let index = op.getattr("mod")?.call1((
            added,
            builtin(&module, "len")?.call1((client.getattr("_hosts")?,))?,
        ))?;
        client.setattr("_host_index", index)?;
        client.call_method0("connect")?;
        assert_conn(&client, &module, assert_enabled)?;
    } else {
        client.getattr("_conn")?.call_method0("close")?;
        client.setattr("_connected", false)?;
        client.call_method0("connect")?;
    }
    let result = client
        .getattr("_conn")?
        .call_method1("execute", (command,))?;
    Ok((true, result, false).into_pyobject(py)?.into_any().unbind())
}

#[pyfunction]
pub fn extension_client_execute<'py>(
    py: Python<'py>,
    domain: &str,
    client: Bound<'py, PyAny>,
    command: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
    assert_enabled: bool,
) -> PyResult<Py<PyAny>> {
    let disconnected = if domain == "mac" {
        client.getattr("_conn")?.is_none()
    } else {
        !client.getattr("_connected")?.is_truthy()?
    };
    if disconnected {
        client.call_method0("connect")?;
    }
    if domain == "mac" {
        assert_conn(&client, &module, assert_enabled)?;
    }
    if domain != "ex" {
        return Ok(module
            .getattr("_native_execute_attempt")?
            .call1((&client, domain, command))?
            .get_item(1)?
            .unbind());
    }
    if !client.getattr("_auto_failover")?.is_truthy()? {
        return Ok(client
            .getattr("_conn")?
            .call_method1("execute", (command,))?
            .unbind());
    }
    let max = builtin(&module, "max")?;
    let limit = max.call1((
        1,
        builtin(&module, "len")?.call1((client.getattr("_hosts")?,))?,
    ))?;
    let mut last = py.None().into_bound(py);
    for iteration in builtin(&module, "range")?.call1((limit,))?.try_iter()? {
        let _ = iteration?;
        let outcome = module
            .getattr("_native_execute_attempt")?
            .call1((&client, domain, &command))?;
        if outcome.get_item(0)?.is_truthy()? {
            let result = outcome.get_item(1)?;
            let add = client.getattr("_healthy")?.getattr("add")?;
            add.call1((client.getattr("_host")?,))?;
            let pop = client.getattr("_unhealthy")?.getattr("pop")?;
            pop.call1((client.getattr("_host")?, py.None()))?;
            return Ok(result.unbind());
        }
        last = outcome.get_item(1)?;
        if !outcome.get_item(2)?.is_truthy()? {
            break;
        }
    }
    if !last.is_none() {
        return Err(PyErr::from_value(last));
    }
    Err(transport_error(&module, "没有可用主机")?)
}
