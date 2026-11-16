//! Python request/identity compatibility without strict SDK input narrowing.
use eltdx::f10::{python_legacy as shared, simple_endpoint, Route, SimpleArgument};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};

fn builtin<'py>(module: &Bound<'py, PyAny>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    let builtins = module.py().import("builtins")?;
    builtins.getattr("getattr")?.call1((module, name, builtins.getattr(name)?))
}

#[pyfunction]
pub fn f10_call<'py>(py: Python<'py>, client: Bound<'py, PyAny>, entry: Bound<'py, PyAny>,
    body: Bound<'py, PyAny>, params: Bound<'py, PyAny>, module: Bound<'py, PyAny>) -> PyResult<Py<PyAny>>
{
    if !body.is_none() && !params.is_none() {
        return Err(pyo3::exceptions::PyValueError::new_err("pass either body or params, not both"));
    }
    let request = if !params.is_none() {
        let value = builtin(&module, "list")?.call1((params,))?;
        let wrapper = PyDict::new(py);
        wrapper.set_item("Params", value)?;
        wrapper.into_any()
    } else if !body.is_none() { body } else { PyDict::new(py).into_any() };
    Ok(client.call_method1("_post", (entry, request))?.unbind())
}

#[pyfunction]
pub fn f10_params<'py>(py: Python<'py>, client: Bound<'py, PyAny>, entry: Bound<'py, PyAny>,
    params: Bound<'py, PyAny>) -> PyResult<Py<PyAny>>
{
    let kwargs = PyDict::new(py);
    kwargs.set_item("params", params)?;
    Ok(client.call_method("call", (entry,), Some(&kwargs))?.unbind())
}

#[pyfunction]
pub fn f10_prepare_request<'py>(py: Python<'py>, client: Bound<'py, PyAny>, entry: Bound<'py, PyAny>,
    body: Bound<'py, PyAny>, module: Bound<'py, PyAny>) -> PyResult<Py<PyAny>>
{
    let route = shared::route_with(|| entry.eq(module.getattr("LIMIT_BOARD_LADDER_ENTRY")?),
        || entry.call_method1("startswith", (module.getattr("_MG_HK_ENTRY_PREFIXES")?,))?.is_truthy(),
        || entry.call_method1("startswith", (module.getattr("_QH_ENTRY_PREFIXES")?,))?.is_truthy())?;
    let base = client.getattr(match route { Route::Static => "base_url", Route::Ladder => "limit_board_ladder_base_url", Route::International => "mg_hk_base_url" })?;
    let base: String = builtin(&module, "format")?.call1((base, ""))?.extract()?;
    let query = PyDict::new(py);
    query.set_item("Entry", entry)?;
    let query = module.getattr("urlencode")?.call1((query,))?;
    let query: String = builtin(&module, "format")?.call1((query, ""))?.extract()?;
    let url = format!("{base}?{query}");
    let kwargs = PyDict::new(py);
    kwargs.set_item("ensure_ascii", false)?;
    kwargs.set_item("separators", (",", ":"))?;
    let data = module.getattr("json")?.call_method("dumps", (body,), Some(&kwargs))?.call_method1("encode", ("utf-8",))?;
    let request = module.getattr("Request")?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("data", data)?;
    kwargs.set_item("headers", client.getattr("headers")?)?;
    kwargs.set_item("method", "POST")?;
    Ok(request.call((url,), Some(&kwargs))?.unbind())
}

#[pyfunction]
pub fn f10_code6<'py>(client_module: Bound<'py, PyAny>, code: Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    Ok(client_module.getattr("split_code")?.call1((code,))?.get_item(2)?.unbind())
}

fn code_error(module: &Bound<'_, PyAny>, code: &Bound<'_, PyAny>, market: &str) -> PyResult<PyErr> {
    let message = format!("invalid {market} code: {}", code.repr()?.to_str()?);
    Ok(match module.getattr("ProtocolError")?.call1((message,)) {
        Ok(value) => PyErr::from_value(value), Err(error) => error,
    })
}

#[pyfunction]
pub fn f10_normalize_code<'py>(py: Python<'py>, code: Bound<'py, PyAny>, kind: &str,
    module: Bound<'py, PyAny>) -> PyResult<Py<PyAny>>
{
    let mut text = builtin(&module, "str")?.call1((&code,))?.call_method0("strip")?;
    if kind == "us" || kind == "hk" {
        if text.contains(".")? { text = text.call_method1("split", (".",))?.get_item(-1)?; }
    }
    match kind {
        "us" => {
            text = text.call_method0("upper")?;
            if !text.is_truthy()? { return Err(code_error(&module, &code, "US")?); }
            let mut any = false;
            for character in text.try_iter()? {
                if character?.call_method0("isalnum")?.is_truthy()? { any = true; break; }
            }
            if !any { return Err(code_error(&module, &code, "US")?); }
            let allowed = shared::allowed_symbols(text.try_iter()?,
                |character| character.call_method0("isalnum")?.is_truthy(), |character| character.eq("-"))?;
            if !allowed { return Err(code_error(&module, &code, "US")?); }
        }
        "hk" => {
            if !text.call_method0("isdigit")?.is_truthy()? { return Err(code_error(&module, &code, "HK")?); }
            text = text.call_method1("zfill", (5,))?;
        }
        "bond" | "bond_market" => {
            if kind == "bond_market" { text = text.call_method0("lower")?; }
            if text.len()? == 8 && module.getattr("MARKET_TO_ID")?.contains(text.get_item(pyo3::types::PySlice::new(py, 0, 2, 1))?)? {
                if kind == "bond_market" {
                    let market = if text.get_item(pyo3::types::PySlice::new(py, 0, 2, 1))?.eq("sh")? { "1" } else { "0" };
                    return Ok(market.into_pyobject(py)?.into_any().unbind());
                }
                text = text.get_item(pyo3::types::PySlice::new(py, 2, isize::MAX, 1))?;
            }
            if kind == "bond_market" { return Ok("1".into_pyobject(py)?.into_any().unbind()); }
            if text.len()? != 6 || !text.call_method0("isdigit")?.is_truthy()? { return Err(code_error(&module, &code, "bond")?); }
        }
        _ => return Err(pyo3::exceptions::PyValueError::new_err("unknown private F10 code profile")),
    }
    Ok(text.unbind())
}

#[pyfunction]
pub fn f10_require_section(section: Bound<'_, PyAny>, valid: Bound<'_, PyAny>, module: Bound<'_, PyAny>) -> PyResult<()> {
    if valid.contains(&section)? { return Ok(()); }
    let sorted = builtin(&module, "sorted")?.call1((valid,))?;
    let names: String = ", ".into_pyobject(module.py())?.call_method1("join", (sorted,))?.extract()?;
    Err(pyo3::exceptions::PyValueError::new_err(format!("unknown section: {}; expected one of: {names}", section.repr()?.to_str()?)))
}

#[pyfunction]
pub fn f10_section_request<'py>(section: Bound<'py, PyAny>, table: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>) -> PyResult<Py<PyAny>>
{
    let valid = builtin(&module, "tuple")?.call1((&table,))?;
    module.getattr("_require_section")?.call1((&section, valid))?;
    Ok(table.get_item(section)?.unbind())
}

/// Evaluate the same ordered wire template as the checked Rust query while
/// retaining Python objects, its public helper overrides, and evaluation order.
#[pyfunction]
pub fn f10_simple_endpoint<'py>(py: Python<'py>, client: Bound<'py, PyAny>, method: &str,
    code: Bound<'py, PyAny>, section: Bound<'py, PyAny>, module: Bound<'py, PyAny>) -> PyResult<Py<PyAny>>
{
    let definition = simple_endpoint(method).ok_or_else(||
        pyo3::exceptions::PyValueError::new_err(format!("unknown fixed F10 endpoint: {method}")))?;
    if let Some(valid) = definition.valid_sections {
        // The original methods pass a tuple literal; do not use a replaceable
        // module `tuple` callable for this literal construction.
        let valid = PyTuple::new(py, valid.iter().copied())?;
        module.getattr("_require_section")?.call1((&section, valid))?;
    }
    // Python resolves self.params before evaluating any call arguments. A
    // custom code converter may change that attribute for a subsequent call.
    let params = client.getattr("params")?;
    let values = definition.compose(|argument| -> PyResult<Py<PyAny>> {
        Ok(match argument {
            SimpleArgument::Literal(value) => value.into_pyobject(py)?.into_any().unbind(),
            SimpleArgument::Code => match definition.code_helper {
                Some(helper) => module.getattr(helper)?.call1((&code,))?.unbind(),
                None => code.clone().unbind(),
            },
            SimpleArgument::Section => section.clone().unbind(),
            SimpleArgument::Market => module.getattr("_bond_sc")?.call1((&code,))?.unbind(),
        })
    })?;
    let mut arguments = Vec::with_capacity(values.len() + 1);
    arguments.push(definition.entry.into_pyobject(py)?.into_any().unbind());
    arguments.extend(values);
    Ok(params.call1(PyTuple::new(py, arguments)?)?.unbind())
}
