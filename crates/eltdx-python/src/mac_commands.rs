//! Legacy MAC constructors and request assembly. Public command classes retain
//! their identities/signatures; this module owns all source business decisions.
use crate::f10_request::builtin;
use eltdx::mac::layout;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyIterator, PyList, PySlice, PyTuple};

fn assign<'py>(
    command: &Bound<'py, PyAny>,
    args: &Bound<'py, PyTuple>,
    fields: &[(&str, usize)],
) -> PyResult<()> {
    for &(name, index) in fields {
        command.setattr(name, args.get_item(index)?)?;
    }
    Ok(())
}
fn ymd<'py>(py: Python<'py>, value: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    if value.is_none() {
        return Ok(0_i32.into_pyobject(py)?.into_any());
    }
    let op = py.import("operator")?;
    let year = op.getattr("mul")?.call1((value.getattr("year")?, 10000))?;
    let month = op.getattr("mul")?.call1((value.getattr("month")?, 100))?;
    let prefix = op.getattr("add")?.call1((year, month))?;
    op.getattr("add")?.call1((prefix, value.getattr("day")?))
}

#[pyfunction]
pub fn mac_command_init<'py>(
    py: Python<'py>,
    kind: &str,
    command: Bound<'py, PyAny>,
    args: Bound<'py, PyTuple>,
    module: Bound<'py, PyAny>,
) -> PyResult<()> {
    match kind {
        "quotes" => {
            let stocks = args.get_item(0)?;
            if !stocks.is_truthy()? {
                return Err(PyErr::from_value(
                    builtin(&module, "ValueError")?.call1(("stocks 不能为空",))?,
                ));
            }
            command.setattr("_stocks", stocks)?;
            let fields = args.get_item(1)?;
            command.setattr(
                "_fields",
                if fields.is_none() {
                    module.getattr("PresetField")?.getattr("COMMON")?
                } else {
                    fields
                },
            )?;
            let bytes = builtin(&module, "bytes")?;
            let bitmap = module.getattr("build_bitmap")?;
            command.setattr(
                "_bitmap",
                bytes.call1((bitmap.call1((command.getattr("_fields")?,))?,))?,
            )?;
        }
        "members" => {
            assign(
                &command,
                &args,
                &[
                    ("_board_code", 0),
                    ("_sort_type", 1),
                    ("_start", 2),
                    ("_page_size", 3),
                    ("_sort_order", 4),
                    ("_fields", 5),
                ],
            )?;
            let exclude = args.get_item(6)?;
            command.setattr(
                "_exclude_flags",
                if exclude.is_truthy()? {
                    exclude
                } else {
                    PyList::empty(py).into_any()
                },
            )?;
        }
        "bars" => assign(
            &command,
            &args,
            &[
                ("_market", 0),
                ("_code", 1),
                ("_period", 2),
                ("_times", 6),
                ("_start", 3),
                ("_count", 4),
                ("_fq", 5),
            ],
        )?,
        "minute" | "multi_minute" | "transactions" => {
            if kind == "multi_minute" {
                let days = args.get_item(3)?;
                if !(1_i32.into_pyobject(py)?.le(&days)? && days.le(command.getattr("MAX_DAYS")?)?)
                {
                    let constructor = builtin(&module, "ValueError")?;
                    let maximum = "{:}"
                        .into_pyobject(py)?
                        .call_method1("format", (command.getattr("MAX_DAYS")?,))?;
                    let days = "{:}".into_pyobject(py)?.call_method1("format", (days,))?;
                    let message = "days 必须在 1..{} 之间，收到 {}"
                        .into_pyobject(py)?
                        .call_method1("format", (maximum, days))?;
                    return Err(PyErr::from_value(constructor.call1((message,))?));
                }
            }
            assign(&command, &args, &[("_market", 0), ("_code", 1)])?;
            command.setattr("_ymd", ymd(py, args.get_item(2)?)?)?;
            if kind == "multi_minute" {
                command.setattr("_days", args.get_item(3)?)?;
            }
            if kind == "transactions" {
                assign(&command, &args, &[("_start", 3), ("_count", 4)])?;
            }
        }
        "info" | "belong" | "capital" => assign(&command, &args, &[("_market", 0), ("_code", 1)])?,
        "boards" => assign(
            &command,
            &args,
            &[("_board_type", 0), ("_start", 1), ("_page_size", 2)],
        )?,
        "auction" => assign(
            &command,
            &args,
            &[("_market", 0), ("_code", 1), ("_start", 2), ("_count", 3)],
        )?,
        "unusual" => {
            assign(&command, &args, &[("_market", 0), ("_start", 1)])?;
            command.setattr(
                "_count",
                builtin(&module, "min")?.call1((args.get_item(2)?, 600))?,
            )?;
        }
        "offsets" => assign(&command, &args, &[("_offset", 0), ("_count", 1)])?,
        "sampling" => assign(&command, &args, &[("market", 0), ("code", 1)])?,
        "goods" => {
            let count = args.get_item(2)?;
            if count.gt(command.getattr("MAX_COUNT")?)? {
                let constructor = builtin(&module, "ValueError")?;
                let maximum = "{:}"
                    .into_pyobject(py)?
                    .call_method1("format", (command.getattr("MAX_COUNT")?,))?;
                let count = "{:}".into_pyobject(py)?.call_method1("format", (count,))?;
                let message = "count 不能超过 {}，当前: {}"
                    .into_pyobject(py)?
                    .call_method1("format", (maximum, count))?;
                return Err(PyErr::from_value(constructor.call1((message,))?));
            }
            assign(
                &command,
                &args,
                &[("market", 0), ("start", 1), ("count", 2)],
            )?;
            command.setattr("total", 0)?;
        }
        "file_meta" => assign(&command, &args, &[("filename", 0), ("offset", 1)])?,
        "file_download" => assign(
            &command,
            &args,
            &[("filename", 0), ("index", 1), ("offset", 2), ("size", 3)],
        )?,
        _ => {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "unknown private MAC constructor",
            ))
        }
    }
    Ok(())
}

// Argument assembly preserves source left-to-right attribute/int/encoding
// evaluation, after capturing the live struct.pack callable.
fn pack<'py>(
    py: Python<'py>,
    command: &Bound<'py, PyAny>,
    module: &Bound<'py, PyAny>,
    fmt: &str,
    fields: &[&str],
) -> PyResult<Bound<'py, PyAny>> {
    let method = module.getattr("struct")?.getattr("pack")?;
    let mut args = vec![fmt.into_pyobject(py)?.into_any()];
    for &field in fields {
        let value = if let Some(name) = field.strip_prefix("int:") {
            let int = builtin(module, "int")?;
            int.call1((command.getattr(name)?,))?
        } else if let Some(name) = field.strip_prefix("gbk:") {
            command.getattr(name)?.call_method1("encode", ("gbk",))?
        } else {
            match field {
                "#0" => 0_i32.into_pyobject(py)?.into_any(),
                "#1" => 1_i32.into_pyobject(py)?.into_any(),
                "#20" => 20_i32.into_pyobject(py)?.into_any(),
                "#30" => 30_i32.into_pyobject(py)?.into_any(),
                "#40" => 40_i32.into_pyobject(py)?.into_any(),
                "#50" => 50_i32.into_pyobject(py)?.into_any(),
                "#200" => 200_i32.into_pyobject(py)?.into_any(),
                "#empty" => PyBytes::new(py, b"").into_any(),
                "#bar0" => layout::BARS_FLAGS[0].into_pyobject(py)?.into_any(),
                "#bar1" => layout::BARS_FLAGS[1].into_pyobject(py)?.into_any(),
                "#bar2" => layout::BARS_FLAGS[2].into_pyobject(py)?.into_any(),
                "#bar3" => layout::BARS_FLAGS[3].into_pyobject(py)?.into_any(),
                "#belong" => PyBytes::new(py, layout::BELONG_TAG).into_any(),
                "#capital" => PyBytes::new(py, layout::CAPITAL_TAG).into_any(),
                name => command.getattr(name)?,
            }
        };
        args.push(value);
    }
    method.call1(PyTuple::new(py, args)?)
}
fn padded<'py>(
    py: Python<'py>,
    raw: Bound<'py, PyAny>,
    width: impl Fn() -> PyResult<Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let op = py.import("operator")?;
    let padding = op
        .getattr("mul")?
        .call1((PyBytes::new(py, b"\0"), width()?))?;
    let combined = op.getattr("add")?.call1((raw, padding))?;
    combined.get_item(py.get_type::<PySlice>().call1((py.None(), width()?))?)
}

#[pyfunction]
pub fn mac_command_build<'py>(
    py: Python<'py>,
    kind: &str,
    command: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let op = py.import("operator")?;
    let body = match kind {
        "quotes" => {
            let mut body = builtin(&module, "bytearray")?.call1((command.getattr("_bitmap")?,))?;
            let method = module.getattr("struct")?.getattr("pack")?;
            let len = builtin(&module, "len")?.call1((command.getattr("_stocks")?,))?;
            body = op
                .getattr("iadd")?
                .call1((body, method.call1(("<H", len))?))?;
            for row in command.getattr("_stocks")?.try_iter()? {
                let row = row?;
                py.check_signals()?;
                let mut iter = row.try_iter()?;
                let market = iter.next().transpose()?.ok_or_else(|| {
                    pyo3::exceptions::PyValueError::new_err(
                        "not enough values to unpack (expected 2, got 0)",
                    )
                })?;
                let code = iter.next().transpose()?.ok_or_else(|| {
                    pyo3::exceptions::PyValueError::new_err(
                        "not enough values to unpack (expected 2, got 1)",
                    )
                })?;
                if iter.next().transpose()?.is_some() {
                    return Err(pyo3::exceptions::PyValueError::new_err(
                        "too many values to unpack (expected 2)",
                    ));
                }
                let method = module.getattr("struct")?.getattr("pack")?;
                let value =
                    method.call1(("<H22s", market, code.call_method1("encode", ("gbk",))?))?;
                body = op.getattr("iadd")?.call1((body, value))?;
            }
            // The source captures build and MSG_ID before invoking bytes(body).
            let method = module.getattr("build_mac_request")?;
            let id = command.getattr("MSG_ID")?;
            return Ok(method
                .call1((id, builtin(&module, "bytes")?.call1((body,))?))?
                .unbind());
        }
        "members" => {
            let mut body = pack(
                py,
                &command,
                &module,
                layout::BOARD_MEMBERS_FORMAT,
                &[
                    "_board_code",
                    "int:_sort_type",
                    "_start",
                    "_page_size",
                    "int:_sort_order",
                    "#0",
                ],
            )?;
            let method = module.getattr("build_bitmap")?;
            let fields = command.getattr("_fields")?;
            let kwargs = PyDict::new(py);
            kwargs.set_item("width", module.getattr("BOARD_MEMBERS_BITMAP_SIZE")?)?;
            let bitmap = method.call((fields,), Some(&kwargs))?;
            body = op
                .getattr("iadd")?
                .call1((body, builtin(&module, "bytes")?.call1((bitmap,))?))?;
            let sum = builtin(&module, "sum")?;
            let source = command.getattr("_exclude_flags")?.try_iter()?;
            let values = Py::new(
                py,
                MacExcludeInts {
                    source: source.unbind(),
                    module: module.clone().unbind(),
                },
            )?;
            // The generator-expression shell keeps its lazy consumption and
            // StopIteration conversion without a Python business loop.
            let value =
                sum.call1((module.getattr("_native_exclude_values")?.call1((values,))?,))?;
            let tail = module
                .getattr("struct")?
                .call_method1("pack", ("<BBBB", 0, value, 0, 1))?;
            op.getattr("iadd")?.call1((body, tail))?
        }
        "bars" => pack(
            py,
            &command,
            &module,
            layout::BARS_FORMAT,
            &[
                "_market",
                "gbk:_code",
                "int:_period",
                "_times",
                "_start",
                "_count",
                "int:_fq",
                "#bar0",
                "#bar1",
                "#bar2",
                "#bar3",
                "#0",
                "#empty",
            ],
        )?,
        "minute" => pack(
            py,
            &command,
            &module,
            layout::MINUTE_FORMAT,
            &["_market", "gbk:_code", "_ymd", "#1", "#0", "#0", "#0", "#0"],
        )?,
        "multi_minute" => pack(
            py,
            &command,
            &module,
            layout::MULTI_MINUTE_FORMAT,
            &["_market", "gbk:_code", "_ymd", "_days", "#1"],
        )?,
        "transactions" => pack(
            py,
            &command,
            &module,
            layout::TRANSACTIONS_FORMAT,
            &["_market", "gbk:_code", "_ymd", "_start", "_count"],
        )?,
        "info" => pack(
            py,
            &command,
            &module,
            layout::INFO_FORMAT,
            &["_market", "gbk:_code", "#1"],
        )?,
        "boards" => pack(
            py,
            &command,
            &module,
            layout::BOARDS_FORMAT,
            &["_page_size", "int:_board_type", "#0", "#0", "_start", "#1"],
        )?,
        "belong" | "capital" => pack(
            py,
            &command,
            &module,
            layout::JSON_FORMAT,
            &[
                "_market",
                "gbk:_code",
                if kind == "belong" {
                    "#belong"
                } else {
                    "#capital"
                },
            ],
        )?,
        "auction" => pack(
            py,
            &command,
            &module,
            layout::AUCTION_FORMAT,
            &["_market", "gbk:_code", "_start", "_count"],
        )?,
        "unusual" => pack(
            py,
            &command,
            &module,
            layout::UNUSUAL_FORMAT,
            &[
                "_market", "_start", "_count", "#1", "#200", "#30", "#40", "#50", "#200",
            ],
        )?,
        "offsets" => pack(
            py,
            &command,
            &module,
            layout::OFFSETS_FORMAT,
            &["_offset", "_count"],
        )?,
        "goods" => pack(
            py,
            &command,
            &module,
            layout::GOODS_FORMAT,
            &["market", "start", "count"],
        )?,
        "sampling" => {
            let raw = command.getattr("code")?.call_method1("encode", ("gbk",))?;
            let padded = padded(py, raw, || command.getattr("_CODE_LEN"))?;
            let method = module.getattr("struct")?.getattr("pack")?;
            method.call1((
                layout::SAMPLING_FORMAT,
                command.getattr("market")?,
                padded,
                1,
                20,
            ))?
        }
        "file_meta" | "file_download" => {
            let raw = command
                .getattr("filename")?
                .call_method1("encode", ("gbk",))?;
            let padded = padded(py, raw, || module.getattr("_FILENAME_LEN"))?;
            let prefix = if kind == "file_meta" {
                pack(py, &command, &module, "<I", &["offset"])?
            } else {
                pack(py, &command, &module, "<III", &["index", "offset", "size"])?
            };
            let prefix = op.getattr("add")?.call1((prefix, padded))?;
            let padding = op
                .getattr("mul")?
                .call1((PyBytes::new(py, b"\0"), module.getattr("_FILENAME_PAD")?))?;
            op.getattr("add")?.call1((prefix, padding))?
        }
        "server" => {
            let text: String = layout::SERVER_PREFIX
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            let header = builtin(&module, "bytes")?.call_method1("fromhex", (text,))?;
            let first = op
                .getattr("add")?
                .call1((header, PyBytes::new(py, &[0; 8])))?;
            let second = op
                .getattr("add")?
                .call1((first, PyBytes::new(py, &layout::SERVER_MARKER)))?;
            op.getattr("add")?
                .call1((second, PyBytes::new(py, &[0; 52])))?
        }
        _ => {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "unknown private MAC request",
            ))
        }
    };
    let method = module.getattr("build_mac_request")?;
    let id = command.getattr("MSG_ID")?;
    if kind == "belong" || kind == "capital" {
        let kwargs = PyDict::new(py);
        kwargs.set_item("head_flag", command.getattr("HEAD_FLAG")?)?;
        Ok(method.call((id, body), Some(&kwargs))?.unbind())
    } else {
        Ok(method.call1((id, body))?.unbind())
    }
}

#[pyclass]
pub struct MacExcludeInts {
    source: Py<PyIterator>,
    module: Py<PyAny>,
}
#[pymethods]
impl MacExcludeInts {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }
    fn __next__(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        use pyo3::exceptions::{PyRuntimeError, PyStopIteration};
        let mut iterator = self.source.bind(py).clone();
        let Some(item) = iterator.next() else {
            return Ok(None);
        };
        let convert = || -> PyResult<Py<PyAny>> {
            let item = item?;
            let int = builtin(self.module.bind(py), "int")?;
            Ok(int.call1((item.getattr("value")?,))?.unbind())
        };
        match convert() {
            Err(error) if error.is_instance_of::<PyStopIteration>(py) => {
                let outer = PyRuntimeError::new_err("generator raised StopIteration");
                outer.set_cause(py, Some(error));
                Err(outer)
            }
            result => result.map(Some),
        }
    }
}
