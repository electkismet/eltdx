//! Legacy command policy with live Python object/struct hooks. The independent
//! SDK shares the wire registry; checked SDK input types do not narrow Python.
use crate::f10_request::builtin;
use pyo3::prelude::*;
use pyo3::types::PyTuple;

fn invalid<'py>(
    py: Python<'py>,
    module: &Bound<'py, PyAny>,
    template: &str,
    value: Option<&Bound<'py, PyAny>>,
) -> PyResult<()> {
    // The source loads its exception constructor before evaluating the f-string.
    let constructor = builtin(module, "ValueError")?;
    let message = match value {
        Some(value) => template
            .into_pyobject(py)?
            .call_method1("format", (value,))?,
        None => template.into_pyobject(py)?.into_any(),
    };
    Err(PyErr::from_value(constructor.call1((message,))?))
}

fn validate_page<'py>(
    py: Python<'py>,
    module: &Bound<'py, PyAny>,
    start: &Bound<'py, PyAny>,
    count: &Bound<'py, PyAny>,
) -> PyResult<()> {
    if start.lt(0)? {
        return invalid(py, module, "start 必须 >= 0，当前: {:}", Some(start));
    }
    if count.le(0)? {
        return invalid(py, module, "count 必须 > 0，当前: {:}", Some(count));
    }
    Ok(())
}

#[pyfunction]
pub fn ex_command_init<'py>(
    py: Python<'py>,
    kind: &str,
    command: Bound<'py, PyAny>,
    args: Bound<'py, PyTuple>,
    module: Bound<'py, PyAny>,
) -> PyResult<()> {
    let arg = |index| args.get_item(index);
    if kind == "info" {
        validate_page(py, &module, &arg(0)?, &arg(1)?)?;
        command.setattr("start", arg(0)?)?;
        command.setattr("count", arg(1)?)?;
        return Ok(());
    }
    if kind == "quotes" {
        let category = arg(1)?;
        if !PyTuple::new(py, [2, 3])?.contains(&category)? {
            return invalid(py, &module, "不支持的扩展行情类别: {:}", Some(&category));
        }
        validate_page(py, &module, &arg(2)?, &arg(3)?)?;
        for (name, index) in [("market", 0), ("category", 1), ("start", 2), ("count", 3)] {
            command.setattr(name, arg(index)?)?;
        }
        return Ok(());
    }
    let (market, code) = if kind == "bars" { (1, 2) } else { (0, 1) };
    if !arg(code)?.is_truthy()? {
        return invalid(py, &module, "code 不能为空", None);
    }
    match kind {
        "bars" | "history_transactions" => validate_page(py, &module, &arg(3)?, &arg(4)?)?,
        "transactions" => validate_page(py, &module, &arg(2)?, &arg(3)?)?,
        _ => (),
    }
    if kind == "bars" {
        command.setattr("category", arg(0)?)?;
    }
    command.setattr("market", arg(market)?)?;
    command.setattr("code", arg(code)?.call_method1("encode", ("utf-8",))?)?;
    match kind {
        "quote" | "minute" => (),
        "history_minute" => command.setattr("date", arg(2)?)?,
        "range" => {
            command.setattr("start_date", arg(2)?)?;
            command.setattr("end_date", arg(3)?)?;
        }
        "bars" => {
            command.setattr("start", arg(3)?)?;
            command.setattr("count", arg(4)?)?;
        }
        "transactions" => {
            command.setattr("start", arg(2)?)?;
            command.setattr("count", arg(3)?)?;
        }
        "history_transactions" => {
            command.setattr("date", arg(2)?)?;
            command.setattr("start", arg(3)?)?;
            command.setattr("count", arg(4)?)?;
        }
        _ => {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "unknown private EX constructor",
            ))
        }
    }
    Ok(())
}

fn hex(bytes: &[u8], upper: bool) -> String {
    bytes
        .iter()
        .map(|byte| {
            if upper {
                format!("{byte:02X}")
            } else {
                format!("{byte:02x}")
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
fn header(kind: &str) -> PyResult<&'static [u8; 12]> {
    use eltdx::ex::layout::*;
    Ok(match kind {
        "markets" => &MARKETS,
        "count" => &COUNT,
        "info" => &INFO,
        "quote" => &QUOTE,
        "quotes" => &QUOTES,
        "bars" => &BARS,
        "minute" => &MINUTE,
        "history_minute" => &HISTORY_MINUTE,
        "transactions" => &TRANSACTIONS,
        "history_transactions" => &HISTORY_TRANSACTIONS,
        _ => {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "unknown private EX request",
            ))
        }
    })
}
fn pack<'py>(
    py: Python<'py>,
    command: &Bound<'py, PyAny>,
    module: &Bound<'py, PyAny>,
    fmt: &str,
    names: &[&str],
) -> PyResult<Bound<'py, PyAny>> {
    let method = module.getattr("struct")?.getattr("pack")?;
    let mut args = vec![fmt.into_pyobject(py)?.into_any()];
    for name in names {
        args.push(match *name {
            "#0" => 0_i32.into_pyobject(py)?.into_any(),
            "#1" => 1_i32.into_pyobject(py)?.into_any(),
            name => command.getattr(name)?,
        });
    }
    method.call1(PyTuple::new(py, args)?)
}

#[pyfunction]
pub fn ex_command_build<'py>(
    py: Python<'py>,
    kind: &str,
    command: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let op = py.import("operator")?;
    if kind == "range" {
        let bytes = eltdx::ex::layout::RANGE;
        let pkg =
            builtin(&module, "bytearray")?.call_method1("fromhex", (hex(&bytes[..1], false),))?;
        let extend = pkg.getattr("extend")?;
        let pack = module.getattr("struct")?.getattr("pack")?;
        extend.call1((pack.call1(("<B", command.call_method0("_next_seqid")?))?,))?;
        let extend = pkg.getattr("extend")?;
        extend
            .call1((builtin(&module, "bytearray")?
                .call_method1("fromhex", (hex(&bytes[2..], true),))?,))?;
        let extend = pkg.getattr("extend")?;
        extend.call1((pack_identity(py, &command, &module)?,))?;
        let extend = pkg.getattr("extend")?;
        extend.call1((builtin(&module, "bytearray")?.call_method1("fromhex", ("07 00",))?,))?;
        let extend = pkg.getattr("extend")?;
        extend.call1((self::pack(
            py,
            &command,
            &module,
            "<II",
            &["start_date", "end_date"],
        )?,))?;
        return Ok(builtin(&module, "bytes")?.call1((pkg,))?.unbind());
    }
    if kind == "login" {
        let method = module.getattr("struct")?.getattr("pack")?;
        let packed = method.call1(("<H", module.getattr("_MSG_ID")?))?;
        let inner = op
            .getattr("add")?
            .call1((packed, module.getattr("_LOGIN_BODY")?))?;
        let method = module.getattr("struct")?.getattr("pack")?;
        let fmt = module.getattr("_HEADER_FMT")?;
        let flag = module.getattr("_HEAD_FLAG")?;
        let first_len = builtin(&module, "len")?.call1((&inner,))?;
        let second_len = builtin(&module, "len")?.call1((&inner,))?;
        let header = method.call1((fmt, flag, 0, 1, first_len, second_len))?;
        return Ok(op.getattr("add")?.call1((header, inner))?.unbind());
    }
    let header =
        builtin(&module, "bytes")?.call_method1("fromhex", (hex(header(kind)?, false),))?;
    let payload = match kind {
        "markets" | "count" => return Ok(header.unbind()),
        "info" => pack(py, &command, &module, "<IH", &["start", "count"])?,
        "quote" | "minute" => pack_identity(py, &command, &module)?,
        "quotes" => pack(
            py,
            &command,
            &module,
            "<BHHHH",
            &["market", "#0", "start", "count", "#1"],
        )?,
        "bars" => pack(
            py,
            &command,
            &module,
            "<B9sHHIH",
            &["market", "code", "category", "#1", "start", "count"],
        )?,
        "history_minute" => pack(py, &command, &module, "<IB9s", &["date", "market", "code"])?,
        "transactions" => pack(
            py,
            &command,
            &module,
            "<B9siH",
            &["market", "code", "start", "count"],
        )?,
        "history_transactions" => pack(
            py,
            &command,
            &module,
            "<IB9siH",
            &["date", "market", "code", "start", "count"],
        )?,
        _ => unreachable!("registry checked above"),
    };
    Ok(op.getattr("add")?.call1((header, payload))?.unbind())
}
fn pack_identity<'py>(
    py: Python<'py>,
    command: &Bound<'py, PyAny>,
    module: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    pack(py, command, module, "<B9s", &["market", "code"])
}

#[pyfunction]
pub fn ex_command_sequence<'py>(
    py: Python<'py>,
    command: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let seqid = builtin(&module, "type")?
        .call1((&command,))?
        .getattr("_seqid")?;
    let next = if seqid.lt(command.getattr("_SEQID_MAX")?)? {
        py.import("operator")?.getattr("add")?.call1((&seqid, 1))?
    } else {
        command.getattr("_SEQID_MIN")?
    };
    // A custom type/maximum/addition can change the class: look type up again.
    builtin(&module, "type")?
        .call1((command,))?
        .setattr("_seqid", next)?;
    Ok(seqid.unbind())
}

#[pyfunction]
pub fn ex_login_parse<'py>(
    py: Python<'py>,
    body: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    // Legacy heuristic only. This does not establish a production login rule.
    Ok(py
        .import("operator")?
        .getattr("ge")?
        .call1((builtin(&module, "len")?.call1((body,))?, 2))?
        .unbind())
}
