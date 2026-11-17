//! Legacy report preparation keeps eager expressions and live Python hooks.
use crate::f10_request::builtin;
use eltdx::f10::{report_table, ReportArgument, ReportEntry, SHAREHOLDER_PLAN_SECTION};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};

fn get<'py>(values: &Bound<'py, PyDict>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    values
        .get_item(name)?
        .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err(name.to_owned()))
}

fn or_empty<'py>(value: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    if value.is_truthy()? {
        Ok(value)
    } else {
        Ok("".into_pyobject(value.py())?.into_any())
    }
}

fn entry_values<'py>(
    definition: &ReportEntry,
    code: &Bound<'py, PyAny>,
    inputs: &Bound<'py, PyDict>,
) -> PyResult<Bound<'py, PyTuple>> {
    let py = code.py();
    let values = definition.compose(|slot| -> PyResult<Py<PyAny>> {
        Ok(match slot {
            ReportArgument::Code => code.clone().unbind(),
            ReportArgument::Literal(value) => value.into_pyobject(py)?.into_any().unbind(),
            ReportArgument::Input(name) => get(inputs, name)?.unbind(),
            ReportArgument::OrInput(name, default) => {
                let value = get(inputs, name)?;
                if value.is_truthy()? {
                    value.unbind()
                } else {
                    default.into_pyobject(py)?.into_any().unbind()
                }
            }
        })
    })?;
    PyTuple::new(py, values)
}

fn execute_selected<'py>(
    client: &Bound<'py, PyAny>,
    selected: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let selected = crate::offline_financial::unpack(selected, 2)?;
    // Unpacking occurs before resolving self.params, while star expansion of
    // the selected parameter object occurs afterwards in the old call.
    let params = client.getattr("params")?;
    let mut values = vec![selected[0].clone().unbind()];
    for value in selected[1].try_iter()? {
        values.push(value?.unbind());
    }
    Ok(params.call1(PyTuple::new(client.py(), values)?)?.unbind())
}

#[pyfunction]
pub fn f10_report<'py>(
    py: Python<'py>,
    client: Bound<'py, PyAny>,
    method: &str,
    code: Bound<'py, PyAny>,
    section: Bound<'py, PyAny>,
    inputs: Bound<'py, PyDict>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let definitions = report_table(method);
    if definitions.is_empty() {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "unknown F10 report",
        ));
    }
    let section = if method == "market_report" {
        module
            .getattr("_MARKET_REPORT_SECTION_ALIASES")?
            .call_method1("get", (&section, &section))?
    } else {
        section
    };
    let code = if method == "market_report" {
        // Membership is evaluated even for a truthy code, before _code6.
        let code_less = module
            .getattr("_MARKET_REPORT_CODELESS_SECTIONS")?
            .contains(&section)?;
        if code.is_truthy()? || !code_less {
            module.getattr("_code6")?.call1((code,))?
        } else {
            "".into_pyobject(py)?.into_any()
        }
    } else {
        module.getattr("_code6")?.call1((code,))?
    };

    if matches!(method, "shareholders" | "fund_report" | "market_report") {
        inputs.set_item(
            "page",
            builtin(&module, "str")?.call1((get(&inputs, "page")?,))?,
        )?;
        inputs.set_item(
            "page_size",
            builtin(&module, "str")?.call1((get(&inputs, "page_size")?,))?,
        )?;
    }
    if method == "digest" {
        inputs.set_item(
            "page_size",
            builtin(&module, "str")?.call1((get(&inputs, "page_size")?,))?,
        )?;
    }
    match method {
        "shareholders" => inputs.set_item("date", or_empty(get(&inputs, "report_date")?)?)?,
        "business_report" => {
            let date = get(&inputs, "report_date")?;
            let text = if date.is_none() {
                "".into_pyobject(py)?.into_any()
            } else {
                builtin(&module, "str")?.call1((date,))?
            };
            inputs.set_item("date", text)?;
        }
        "fund_report" => {
            let mut date = or_empty(get(&inputs, "date")?)?;
            if !date.is_truthy()?
                && module
                    .getattr("_FUND_REPORT_PERIOD_ENTRIES")?
                    .contains(&section)?
            {
                // Resolve both callables before evaluating their arguments.
                let latest = module.getattr("_latest_report_period")?;
                let params = client.getattr("params")?;
                let period = module
                    .getattr("_FUND_REPORT_PERIOD_ENTRIES")?
                    .get_item(&section)?;
                let response = params.call1(("CWServ.skef10_jj_tzzh", period, &code, ""))?;
                date = latest.call1((response,))?;
            }
            inputs.set_item("date", date)?;
        }
        _ => (),
    }

    let table = PyDict::new(py);
    for definition in definitions {
        // Capital detail was not a row in the original eager table; its date
        // conversion belongs after section validation and params lookup.
        if method == "capital_structure" && definition.section == "restricted_detail" {
            continue;
        }
        table.set_item(
            definition.section,
            (definition.entry, entry_values(definition, &code, &inputs)?),
        )?;
    }
    if method == "capital_structure" {
        let valid = PyTuple::new(py, definitions.iter().map(|row| row.section))?;
        module
            .getattr("_require_section")?
            .call1((&section, valid))?;
        if section.eq("restricted_detail")? {
            let date = get(&inputs, "date")?;
            if !date.is_truthy()? {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "capital_structure section 'restricted_detail' requires date (YYYYMMDD)",
                ));
            }
            let params = client.getattr("params")?;
            let date = module.getattr("_date_text")?.call1((date,))?;
            return Ok(params
                .call1((
                    definitions.last().expect("capital detail definition").entry,
                    code,
                    date,
                ))?
                .unbind());
        }
        return execute_selected(&client, table.as_any().get_item(&section)?);
    }
    let selected = module
        .getattr("_section_request")?
        .call1((section, table))?;
    execute_selected(&client, selected)
}

#[pyfunction]
pub fn f10_shareholder_plans<'py>(
    py: Python<'py>,
    client: Bound<'py, PyAny>,
    code: Bound<'py, PyAny>,
    inputs: Bound<'py, PyDict>,
) -> PyResult<Py<PyAny>> {
    let method = client.getattr("shareholders")?;
    let kwargs = PyDict::new(py);
    for name in ["page", "page_size", "filter1", "filter2"] {
        kwargs.set_item(name, get(&inputs, name)?)?;
    }
    Ok(method
        .call((code, SHAREHOLDER_PLAN_SECTION), Some(&kwargs))?
        .unbind())
}

#[pyfunction]
pub fn f10_business_composition<'py>(
    py: Python<'py>,
    client: Bound<'py, PyAny>,
    code: Bound<'py, PyAny>,
    date: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let number = module.getattr("_code6")?.call1((&code,))?;
    let selected = if date.is_truthy()? {
        date
    } else {
        let first = module.getattr("_first_value")?;
        let response = client.call_method1("business_periods", (&number,))?;
        first.call1((response, "T002"))?
    };
    if selected.is_none() {
        let exception = module.getattr("ProtocolError")?;
        // str.format matches the original f-string protocol for custom codes.
        let message = "no business composition report date for {:}"
            .into_pyobject(py)?
            .call_method1("format", (number,))?;
        return Err(PyErr::from_value(exception.call1((message,))?));
    }
    let method = client.getattr("business_report")?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("report_date", builtin(&module, "str")?.call1((selected,))?)?;
    Ok(method.call((code, "composition"), Some(&kwargs))?.unbind())
}

#[pyfunction]
pub fn f10_topic_compare_first<'py>(
    py: Python<'py>,
    client: Bound<'py, PyAny>,
    code: Bound<'py, PyAny>,
    section: Bound<'py, PyAny>,
    sort_by: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let first = module.getattr("_first_value")?;
    let response = client.call_method1("topic_ids", (&code,))?;
    let topic = first.call1((response, "t001"))?;
    if topic.is_none() {
        let exception = module.getattr("ProtocolError")?;
        let number = module.getattr("_code6")?.call1((code,))?;
        let message = "no topic id for {:}"
            .into_pyobject(py)?
            .call_method1("format", (number,))?;
        return Err(PyErr::from_value(exception.call1((message,))?));
    }
    let method = client.getattr("topic_compare")?;
    let topic = builtin(&module, "str")?.call1((topic,))?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("section", section)?;
    kwargs.set_item("sort_by", sort_by)?;
    Ok(method.call((code, topic), Some(&kwargs))?.unbind())
}
