//! Dynamic field dictionaries remain Python data; selection and traversal
//! share the SDK's projection policy without narrowing custom field objects.
use eltdx::f10::fields::visit_projected;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyString, PyTuple};
use crate::f10_request::builtin;

#[pyfunction]
pub fn f10_duplicate_suffix<'py>(py: Python<'py>, field: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>) -> PyResult<Py<PyAny>>
{
    let values = crate::offline_financial::unpack(field.call_method1("rpartition", ("__",))?, 3)?;
    if values[1].is_truthy()? && values[2].call_method0("isdigit")?.is_truthy()?
        && builtin(&module, "int")?.call1((&values[2],))?.ge(2)? {
        // Keep the second int call: a custom builtin can observe both calls.
        return Ok(PyTuple::new(py, [values[0].clone(), builtin(&module, "int")?.call1((&values[2],))?])?.into_any().unbind());
    }
    Ok(PyTuple::new(py, [field, py.None().into_bound(py)])?.into_any().unbind())
}

#[pyfunction]
pub fn f10_field_scope<'py>(py: Python<'py>, entry: Bound<'py, PyAny>, body: Bound<'py, PyAny>,
    metadata: Bound<'py, PyAny>, international: bool, module: Bound<'py, PyAny>) -> PyResult<Py<PyAny>>
{
    // International checks the body before reading its live index registry;
    // the mainland helper reads the registry first.
    let mut index = if international { None } else {
        let value = module.getattr("_SCOPE_PARAM_INDEX")?.call_method1("get", (&entry,))?;
        if value.is_none() { return Ok(py.None()); }
        Some(value)
    };
    let isinstance = builtin(&module, "isinstance")?;
    if !isinstance.call1((&body, module.getattr("Mapping")?))?.is_truthy()? { return Ok(py.None()); }
    let params = body.call_method1("get", ("Params",))?;
    if !builtin(&module, "isinstance")?.call1((&params, module.getattr("Sequence")?))?.is_truthy()? { return Ok(py.None()); }
    let isinstance = builtin(&module, "isinstance")?;
    let scalar_types = PyTuple::new(py, [builtin(&module, "str")?, builtin(&module, "bytes")?, builtin(&module, "bytearray")?])?;
    if isinstance.call1((&params, scalar_types))?.is_truthy()? { return Ok(py.None()); }
    if international { index = Some(module.getattr("_SCOPE_PARAM_INDEX")?.call_method1("get", (&entry, 0))?); }
    let index = index.expect("scope index resolved by policy");
    if index.ge(builtin(&module, "len")?.call1((&params,))?)? { return Ok(py.None()); }
    let mut scope = builtin(&module, "str")?.call1((params.get_item(index)?,))?;
    if !international && entry.eq("CWServ.tdxf10_gg_cwfx")? {
        // Python membership keeps the original hashing/equality behavior of a
        // live str replacement; ordinary strings use the shared family rule.
        let report = if scope.is_exact_instance_of::<PyString>() {
            match scope.extract::<String>() {
                Ok(text) => eltdx::f10::fields::requires_industry_template("CWServ.tdxf10_gg_cwfx", &text),
                // Python strings can contain isolated surrogates. Membership
                // still works, and no UTF-8 conversion error belongs here.
                Err(_) => false,
            }
        } else {
            pyo3::types::PySet::new(py, ["zcfzb", "lyb", "xjllb"])?.contains(&scope)?
        };
        if report {
            let industry = if metadata.is_truthy()? { metadata.call_method1("get", ("nhytype",))? } else { py.None().into_bound(py) };
            if industry.is_none() { return Ok(py.None()); }
            scope = "{}:{}".into_pyobject(py)?.call_method1("format", (scope, industry))?;
        }
    }
    Ok(scope.unbind())
}

#[pyfunction]
pub fn f10_field_labels<'py>(py: Python<'py>, entry: Bound<'py, PyAny>, body: Bound<'py, PyAny>,
    metadata: Bound<'py, PyAny>, kind: &str, module: Bound<'py, PyAny>) -> PyResult<Py<PyAny>>
{
    if kind == "labels" && module.getattr("_is_mg_hk_entry")?.call1((&entry,))?.is_truthy()? {
        return Ok(module.getattr("_mg_hk_labels_for")?.call1((entry, body))?.unbind());
    }
    let international = kind == "international";
    let scope = if international { module.getattr("_scope")?.call1((&entry, body))? } else {
        let kwargs = PyDict::new(py); kwargs.set_item("metadata", metadata)?;
        module.getattr("_field_scope")?.call((&entry, body), Some(&kwargs))?
    };
    if scope.is_none() { return Ok(PyDict::new(py).into_any().unbind()); }
    if international {
        let mut labels = module.getattr("FIELD_LABELS")?.call_method1("get", ((&entry, scope),))?;
        if labels.is_none() { labels = module.getattr("FIELD_LABELS")?.call_method1("get", ((entry, module.getattr("_WILDCARD")?),))?; }
        let dict = builtin(&module, "dict")?;
        let labels = if labels.is_truthy()? { labels } else { PyDict::new(py).into_any() };
        Ok(dict.call1((labels,))?.unbind())
    } else {
        let dict = builtin(&module, "dict")?;
        let labels = module.getattr(if kind == "notes" { "_FIELD_NOTES" } else { "_FIELD_LABELS" })?
            .call_method1("get", ((entry, scope), PyDict::new(py)))?;
        Ok(dict.call1((labels,))?.unbind())
    }
}

#[pyfunction]
pub fn f10_describe_fields<'py>(py: Python<'py>, entry: Bound<'py, PyAny>, body: Bound<'py, PyAny>,
    fields: Bound<'py, PyAny>, metadata: Bound<'py, PyAny>, kind: &str,
    module: Bound<'py, PyAny>) -> PyResult<Py<PyAny>>
{
    let international = kind == "international";
    let notes = kind == "notes";
    let function = module.getattr(if international { "labels_for" } else if notes { "field_notes_for" } else { "field_labels_for" })?;
    let known = if international { function.call1((entry, body))? } else {
        let kwargs = PyDict::new(py); kwargs.set_item("metadata", metadata)?;
        function.call((entry, body), Some(&kwargs))?
    };
    let labels = PyDict::new(py);
    let unmapped = PyList::empty(py);
    visit_projected(fields.try_iter()?, |field| {
        py.check_signals()?;
        let pair = crate::offline_financial::unpack(module.getattr("_split_duplicate_suffix")?.call1((field,))?, 2)?;
        Ok::<_, PyErr>((pair[0].clone(), pair[1].clone()))
    }, |field| {
        let label = known.call_method1("get", (field,))?;
        Ok(if label.is_none() { None } else { Some(label) })
    }, |label| match label {
        Some(label) if international => Ok(!label.is_truthy()?),
        _ => Ok(label.is_none()),
    }, |field, label, duplicate| {
        if let Some(mut label) = label {
            if !notes && !duplicate.is_none() {
                label = "{}（{}）".into_pyobject(py)?.call_method1("format", (label, duplicate))?;
            }
            labels.set_item(field, label)?;
        } else if !notes { unmapped.append(field)?; }
        Ok(())
    })?;
    if notes { Ok(labels.into_any().unbind()) }
    else { Ok((labels, unmapped).into_pyobject(py)?.into_any().unbind()) }
}

#[pyfunction]
pub fn f10_international_entry(entry: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    Ok(entry.call_method1("startswith", (("CWServ.skef10_mg_", "CWServ.skef10_hk_", "CWServ.skef10_sb_"),))?.unbind())
}
