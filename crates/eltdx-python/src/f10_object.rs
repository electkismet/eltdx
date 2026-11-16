//! Object request compatibility: retain Python conversions and extra updates.
use eltdx::f10::{object_entry, object_fields, ObjectArgument, ObjectPart};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PySet};
use crate::f10_request::builtin;

fn get<'py>(inputs: &Bound<'py, PyDict>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    inputs.get_item(name)?.ok_or_else(|| pyo3::exceptions::PyKeyError::new_err(name.to_owned()))
}

fn text<'py>(module: &Bound<'py, PyAny>, value: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    builtin(module, "str")?.call1((value,))
}

fn populate<'py>(payload: &Bound<'py, PyDict>, part: ObjectPart,
    mut input: impl FnMut(&str) -> PyResult<Bound<'py, PyAny>>) -> PyResult<()>
{
    for &(name, value) in object_fields(part) {
        let value = match value {
            ObjectArgument::Literal(value) => value.into_pyobject(payload.py())?.into_any(),
            ObjectArgument::Integer(value) => value.into_pyobject(payload.py())?.into_any(),
            ObjectArgument::Input(name) => input(name)?,
        };
        payload.set_item(name, value)?;
    }
    Ok(())
}

#[pyfunction]
pub fn f10_object_request<'py>(py: Python<'py>, client: Bound<'py, PyAny>, method: &str,
    code: Bound<'py, PyAny>, inputs: Bound<'py, PyDict>, module: Bound<'py, PyAny>) -> PyResult<Py<PyAny>>
{
    let payload = PyDict::new(py);
    let theme = match method {
        "theme_market" => {
            populate(&payload, ObjectPart::Theme, |name| match name {
                "req_id" => text(&module, get(&inputs, "req_id")?),
                "market" => module.getattr("split_code")?.call1((&code,))?.get_item(0),
                "code" => module.getattr("_code6")?.call1((&code,)),
                "page" => get(&inputs, "page"),
                "page_size" => text(&module, get(&inputs, "page_size")?),
                _ => unreachable!(),
            })?;
            // The source evaluates str(req_id) a second time for membership.
            let request = text(&module, get(&inputs, "req_id")?)?;
            if PySet::new(py, eltdx::f10::THEME_WINDOW_REQUESTS.iter().copied())?.contains(request)? {
                populate(&payload, ObjectPart::Window, |_| text(&module, get(&inputs, "zq_num")?))?;
            }
            true
        }
        "stock_board_member_quotes" => {
            populate(&payload, ObjectPart::Board, |name| match name {
                "market" => text(&module, get(&inputs, "board_market")?),
                "code" => module.getattr("_code6")?.call1((&code,)),
                "page" => get(&inputs, "page"),
                "page_size" => text(&module, get(&inputs, "page_size")?),
                _ => unreachable!(),
            })?;
            true
        }
        "valuation" => {
            let identity = crate::offline_financial::unpack(module.getattr("split_code")?.call1((&code,))?, 3)?;
            let market = &identity[0];
            let number = &identity[2];
            let request = text(&module, get(&inputs, "req_id")?)?;
            let range = PySet::new(py, eltdx::f10::VALUATION_RANGE_REQUESTS.iter().copied())?.contains(&request)?;
            if range {
                populate(&payload, ObjectPart::Range, |name| match name {
                    "req_id" => Ok(request.clone()),
                    "full_code" => "{:}|{:}".into_pyobject(py)?.call_method1("format", (number, market)),
                    "page" => text(&module, get(&inputs, "page")?),
                    "page_size" => text(&module, get(&inputs, "page_size")?), _ => unreachable!(),
                })?;
                populate(&payload, ObjectPart::ValuationModule, |_| unreachable!())?;
                if request.eq("200192")? {
                    populate(&payload, ObjectPart::Percentile, |name| match name {
                        "kind" => Ok("0".into_pyobject(py)?.into_any()),
                        "adjusted_page" => {
                            // str is resolved before evaluating the conditional.
                            let convert = builtin(&module, "str")?;
                            let value = get(&inputs, "page")?;
                            let value = if value.ne(0)? { value } else { (-1).into_pyobject(py)?.into_any() };
                            convert.call1((value,))
                        }
                        "adjusted_size" => {
                            let convert = builtin(&module, "str")?;
                            let value = get(&inputs, "page_size")?;
                            let value = if value.ne(20)? { value } else { 5.into_pyobject(py)?.into_any() };
                            convert.call1((value,))
                        }
                        _ => unreachable!(),
                    })?;
                }
            } else {
                // The original elif chain compares 200135 before 200124.
                let history = request.eq("200135")?;
                let series = if history { false } else { request.eq("200124")? };
                populate(&payload, if series { ObjectPart::Series } else { ObjectPart::History }, |name| match name {
                    "req_id" => Ok(request.clone()), "code" => Ok(number.clone()),
                    "market" => text(&module, market.clone()),
                    "window" => Ok("1Y".into_pyobject(py)?.into_any()),
                    "indicator" | "period" => Ok("0".into_pyobject(py)?.into_any()),
                    "page" => {
                        let page = get(&inputs, "page")?;
                        if page.ne(0)? { Ok(page) } else { Ok(1.into_pyobject(py)?.into_any()) }
                    }
                    "page_size" => {
                        let convert = builtin(&module, "str")?;
                        let value = get(&inputs, "page_size")?;
                        let value = if value.ne(20)? { value } else { 300.into_pyobject(py)?.into_any() };
                        convert.call1((value,))
                    }
                    _ => unreachable!(),
                })?;
                populate(&payload, ObjectPart::ValuationModule, |_| unreachable!())?;
            }
            false
        }
        _ => return Err(pyo3::exceptions::PyValueError::new_err("unknown F10 object request")),
    };
    if method != "stock_board_member_quotes" {
        let extra = get(&inputs, "extra")?;
        if extra.is_truthy()? { payload.call_method1("update", (extra,))?; }
    }
    Ok(client.call_method1("call", (object_entry(theme), PyList::new(py, [payload])?))?.unbind())
}

#[pyfunction]
pub fn f10_object_alias<'py>(py: Python<'py>, client: Bound<'py, PyAny>, method: &str,
    code: Bound<'py, PyAny>, inputs: Bound<'py, PyDict>) -> PyResult<Py<PyAny>>
{
    let kwargs = PyDict::new(py);
    let (target, request) = if let Some(request) = eltdx::f10::stage_request(method) {
        kwargs.set_item("page_size", get(&inputs, "page_size")?)?;
        kwargs.set_item("zq_num", get(&inputs, "days")?)?;
        ("theme_market", request)
    } else {
        match method {
            "stock_valuation_percentile" => {
                kwargs.set_item("page", get(&inputs, "page")?)?;
                kwargs.set_item("page_size", get(&inputs, "page_size")?)?;
                let extra = PyDict::new(py); extra.set_item("Type", get(&inputs, "kind")?)?;
                kwargs.set_item("extra", extra)?;
                ("valuation", "200192")
            }
            "stock_pepb_percentile" => {
                kwargs.set_item("page", get(&inputs, "page")?)?;
                kwargs.set_item("page_size", get(&inputs, "page_size")?)?;
                let extra = PyDict::new(py);
                for (key, source) in [("sjfw", "window"), ("zb", "indicator"), ("hqzq", "period")] {
                    extra.set_item(key, get(&inputs, source)?)?;
                }
                kwargs.set_item("extra", extra)?;
                ("valuation", "200124")
            }
            "stock_history_trend" => ("valuation", "200135"),
            _ => return Err(pyo3::exceptions::PyValueError::new_err("unknown F10 object alias")),
        }
    };
    let request: i64 = request.parse().expect("built-in request id");
    Ok(client.call_method(target, (code, request), Some(&kwargs))?.unbind())
}
