//! Python request/identity compatibility without strict SDK input narrowing.
use eltdx::f10::{input_template, page_template, position_arguments, position_prefix,
    python_legacy as shared, sector_choices, sector_dynamic, sector_selection,
    sector_weights_without_date, simple_endpoint, Route, SimpleArgument, SimpleEndpoint};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};

fn builtin<'py>(module: &Bound<'py, PyAny>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    let builtins = module.py().import("builtins")?;
    builtins.getattr("getattr")?.call1((module, name, builtins.getattr(name)?))
}

fn format_field(value: &Bound<'_, PyAny>) -> PyResult<String> {
    // A literal str.format uses the same special-method lookup and return-type
    // validation as a Python f-string; an instance __format__ attribute or a
    // replaced module-level format function must not intercept it.
    "{:}".into_pyobject(value.py())?.call_method1("format", (value,))?.extract()
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
    validate_template(&definition, &section, &module)?;
    call_template(py, &client, definition, &code, &section, &module, None)
}

fn validate_template<'py>(definition: &SimpleEndpoint, section: &Bound<'py, PyAny>, module: &Bound<'py, PyAny>) -> PyResult<()> {
    if let Some(valid) = definition.valid_sections {
        let require = module.getattr("_require_section")?;
        let mapped = definition.arguments.iter().find_map(|argument| match argument {
            SimpleArgument::MappedSection(map) => Some(*map), _ => None,
        });
        let valid = match mapped {
            // These methods originally called the live tuple builtin on a
            // live mapping. Both lookups happen again for the value below.
            Some(map) => builtin(module, "tuple")?.call1((module.getattr(map.python_attribute())?,))?,
            None => PyTuple::new(module.py(), valid.iter().copied())?.into_any(),
        };
        require.call1((section, valid))?;
    }
    Ok(())
}

fn call_template<'py>(py: Python<'py>, client: &Bound<'py, PyAny>, definition: SimpleEndpoint,
    code: &Bound<'py, PyAny>, section: &Bound<'py, PyAny>, module: &Bound<'py, PyAny>,
    inputs: Option<&Bound<'py, PyDict>>) -> PyResult<Py<PyAny>>
{
    // Python resolves self.params before evaluating any call arguments. A
    // custom code converter may change that attribute for a subsequent call.
    let params = client.getattr(if matches!(definition.method, "market_news" | "channel_news") { "call" } else { "params" })?;
    let entry = if definition.append_section {
        py.import("operator")?.getattr("add")?.call1((definition.entry, section))?
    } else { definition.entry.into_pyobject(py)?.into_any() };
    call_template_values(py, params, entry, definition, code, section, module, inputs)
}

fn call_template_values<'py>(py: Python<'py>, params: Bound<'py, PyAny>, entry: Bound<'py, PyAny>,
    definition: SimpleEndpoint, code: &Bound<'py, PyAny>, section: &Bound<'py, PyAny>,
    module: &Bound<'py, PyAny>, inputs: Option<&Bound<'py, PyDict>>) -> PyResult<Py<PyAny>>
{
    let values = definition.compose(|argument| -> PyResult<Py<PyAny>> {
        Ok(match argument {
            SimpleArgument::Literal(value) => value.into_pyobject(py)?.into_any().unbind(),
            SimpleArgument::Integer(value) => value.into_pyobject(py)?.into_any().unbind(),
            SimpleArgument::Code => match definition.code_helper {
                Some(helper) => module.getattr(helper)?.call1((code,))?.unbind(),
                None => code.clone().unbind(),
            },
            SimpleArgument::Section => section.clone().unbind(),
            SimpleArgument::Market => module.getattr("_bond_sc")?.call1((code,))?.unbind(),
            SimpleArgument::MappedSection(map) => module.getattr(map.python_attribute())?.get_item(section)?.unbind(),
            SimpleArgument::Input(name) | SimpleArgument::DateInput(name)
                | SimpleArgument::TextInput(name) | SimpleArgument::IntTextInput(name) => {
                let value = match inputs { Some(inputs) => inputs.get_item(name)?, None => None };
                let value = value.ok_or_else(|| pyo3::exceptions::PyValueError::new_err(format!("missing F10 input: {name}")))?;
                match argument {
                    SimpleArgument::DateInput(_) => date_text(&value)?.unbind(),
                    SimpleArgument::TextInput(_) => builtin(module, "str")?.call1((value,))?.unbind(),
                    SimpleArgument::IntTextInput(_) => {
                        let text = builtin(module, "str")?;
                        text.call1((builtin(module, "int")?.call1((value,))?,))?.unbind()
                    }
                    _ => value.unbind(),
                }
            }
            SimpleArgument::SectionChoice(value, yes, no) => (if section.eq(value)? { yes } else { no }).into_pyobject(py)?.into_any().unbind(),
        })
    })?;
    if matches!(definition.method, "market_news" | "channel_news") {
        let body = PyDict::new(py);
        body.set_item("Params", pyo3::types::PyList::new(py, values)?)?;
        return Ok(params.call1((entry, body))?.unbind());
    }
    let mut arguments = Vec::with_capacity(values.len() + 1);
    arguments.push(entry.unbind());
    arguments.extend(values);
    Ok(params.call1(PyTuple::new(py, arguments)?)?.unbind())
}

fn date_text<'py>(value: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    value.call_method0("strip")?.call_method1("replace", ("-", ""))
}

fn input<'py>(inputs: &Bound<'py, PyDict>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    inputs.get_item(name)?.ok_or_else(|| pyo3::exceptions::PyKeyError::new_err(name.to_owned()))
}

#[pyfunction]
pub fn f10_input_endpoint<'py>(py: Python<'py>, client: Bound<'py, PyAny>, method: &str,
    code: Bound<'py, PyAny>, section: Bound<'py, PyAny>, inputs: Bound<'py, PyDict>, module: Bound<'py, PyAny>) -> PyResult<Py<PyAny>>
{
    let definition = input_template(method).ok_or_else(||
        pyo3::exceptions::PyValueError::new_err(format!("unknown F10 input endpoint: {method}")))?;
    if method == "hk_analyst_forecast" {
        let mut start = input(&inputs, "start")?;
        let mut end = input(&inputs, "end")?;
        if !start.is_truthy()? || !end.is_truthy()? {
            let today = module.getattr("date")?.call_method0("today")?;
            if !start.is_truthy()? {
                let replace = today.getattr("replace")?;
                let year = py.import("operator")?.getattr("sub")?.call1((today.getattr("year")?, 1))?;
                let kwargs = PyDict::new(py);
                kwargs.set_item("year", year)?;
                start = replace.call((), Some(&kwargs))?.call_method0("isoformat")?;
            }
            if !end.is_truthy()? { end = today.call_method0("isoformat")?; }
            inputs.set_item("start", start)?;
            inputs.set_item("end", end)?;
        }
    }
    call_template(py, &client, definition, &code, &section, &module, Some(&inputs))
}

fn sector_lookup(method: &str, section: &Bound<'_, PyAny>, omit_first: bool) -> PyResult<SimpleEndpoint> {
    let py = section.py();
    let keys = PyDict::new(py);
    for (index, &key) in sector_choices(method).iter().enumerate().skip(usize::from(omit_first)) {
        keys.set_item(key, index)?;
    }
    let index: usize = keys.as_any().get_item(section)?.extract()?;
    sector_selection(method, sector_choices(method)[index]).ok_or_else(||
        pyo3::exceptions::PyValueError::new_err("missing F10 sector template"))
}

#[pyfunction]
pub fn f10_sector_endpoint<'py>(py: Python<'py>, client: Bound<'py, PyAny>, method: &str,
    code: Bound<'py, PyAny>, section: Bound<'py, PyAny>, inputs: Bound<'py, PyDict>, module: Bound<'py, PyAny>) -> PyResult<Py<PyAny>>
{
    let definition = match method {
        "hy_summary" => sector_lookup(method, &section, false)?,
        "hy_members_rank" => {
            let mut result = None;
            for &choice in sector_choices(method) {
                if section.eq(choice)? { result = sector_selection(method, choice); break; }
            }
            match result {
                Some(value) => value,
                None => return Err(pyo3::exceptions::PyValueError::new_err(format!(
                    "unknown hy_members_rank section: {}", section.repr()?.to_str()?))),
            }
        }
        "qh_variety_info" => {
            if section.eq("pzjj")? { sector_selection(method, "pzjj").expect("built-in section") }
            else { sector_lookup(method, &section, true)? }
        }
        "qh_position_report" => {
            let prefix = position_prefix(input(&inputs, "index")?.is_truthy()?);
            let mut definition = sector_lookup(method, &section, false)?;
            let suffix = definition.entry;
            let first = date_text(&input(&inputs, "date")?)?;
            let end = date_text(&input(&inputs, "end_date")?)?;
            inputs.set_item("date", first)?;
            inputs.set_item("end_date", end)?;
            let branch = if PyTuple::new(py, ["cjcc"])?.contains(&section)? { "cjcc" }
                else if section.eq("cjjg")? { "cjjg" } else { "range" };
            definition.arguments = position_arguments(branch);
            let params = client.getattr("params")?;
            let entry = format!("{prefix}{suffix}").into_pyobject(py)?.into_any();
            return call_template_values(py, params, entry, definition, &code, &section, &module, Some(&inputs));
        }
        "qh_weights" => {
            // This conversion precedes the branch even when the date is omitted.
            let date = date_text(&input(&inputs, "date")?)?;
            inputs.set_item("date", date)?;
            if section.eq("cfghyfb")? { sector_weights_without_date() }
            else { sector_dynamic(method).expect("built-in sector") }
        }
        "qh_index_info" | "qh_index_valuation" => sector_dynamic(method).expect("built-in sector"),
        _ => return Err(pyo3::exceptions::PyValueError::new_err(format!("unknown F10 sector endpoint: {method}"))),
    };
    call_template(py, &client, definition, &code, &section, &module, Some(&inputs))
}

#[pyfunction]
pub fn f10_page_endpoint<'py>(py: Python<'py>, client: Bound<'py, PyAny>, method: &str,
    code: Bound<'py, PyAny>, section: Bound<'py, PyAny>, inputs: Bound<'py, PyDict>, module: Bound<'py, PyAny>) -> PyResult<Py<PyAny>>
{
    let prototype = page_template(method, |_| Ok::<_, PyErr>(false))?.ok_or_else(||
        pyo3::exceptions::PyValueError::new_err(format!("unknown F10 page endpoint: {method}")))?;
    validate_template(&prototype, &section, &module)?;
    let definition = page_template(method, |value| section.eq(value))?.ok_or_else(||
        pyo3::exceptions::PyValueError::new_err("missing F10 page definition"))?;
    call_template(py, &client, definition, &code, &section, &module, Some(&inputs))
}

#[pyfunction]
pub fn f10_cache_list<'py>(py: Python<'py>, client: Bound<'py, PyAny>, code: Bound<'py, PyAny>,
    kind: Bound<'py, PyAny>, module: Bound<'py, PyAny>) -> PyResult<Py<PyAny>>
{
    // The old membership operation is a set lookup, including hash errors.
    let kinds = pyo3::types::PySet::new(py, eltdx::f10::CACHE_KINDS.iter().copied())?;
    if !kinds.contains(&kind)? {
        return Err(pyo3::exceptions::PyValueError::new_err("kind must be one of: xw, gg, ly"));
    }
    let identity = crate::offline_financial::unpack(module.getattr("split_code")?.call1((code,))?, 3)?;
    let body = PyDict::new(py);
    for (name, value) in eltdx::f10::cache_payload(|slot| -> PyResult<Py<PyAny>> { Ok(match slot {
        eltdx::f10::CacheValue::Literal(value) => value.into_pyobject(py)?.into_any().unbind(),
        eltdx::f10::CacheValue::Qsid => module.getattr("DEFAULT_QSID")?.unbind(),
        eltdx::f10::CacheValue::Key => {
            let kind = format_field(&kind)?;
            let market = format_field(&identity[0])?;
            let number = format_field(&identity[2])?;
            format!("{kind}:{market}_{number}").into_pyobject(py)?.into_any().unbind()
        }
    }) })? { body.set_item(name, value)?; }
    Ok(client.call_method1("call", (eltdx::f10::CACHE_ENTRY, body))?.unbind())
}

#[pyfunction]
pub fn f10_cache_alias<'py>(client: Bound<'py, PyAny>, method: &str, code: Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    let kind = eltdx::f10::cache_alias(method).ok_or_else(|| pyo3::exceptions::PyValueError::new_err("unknown F10 cache alias"))?;
    Ok(client.call_method1("cache_list", (code, kind))?.unbind())
}

#[pyfunction]
pub fn f10_news_channels(module: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let sorted = builtin(&module, "sorted")?;
    Ok(sorted.call1((module.getattr("NEWS_CHANNEL_LABELS")?.call_method0("items")?,))?.unbind())
}
