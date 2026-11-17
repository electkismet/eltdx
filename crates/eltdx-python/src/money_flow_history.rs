//! Python history orchestration with original callbacks, objects and slicing.
//! The independent Rust service supplies strict typed queries separately.
use crate::helper_f10::builtin;
use eltdx::f10::collection::{
    history_parameters, reverse_then_window, MONEY_FLOW_HISTORY_SELECTOR,
};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PySlice, PyTuple};

#[pyfunction]
pub fn money_flow_history<'py>(
    py: Python<'py>,
    api: Bound<'py, PyAny>,
    code: Bound<'py, PyAny>,
    days: Bound<'py, PyAny>,
    start: Bound<'py, PyAny>,
    count: Bound<'py, PyAny>,
    include_raw: Bound<'py, PyAny>,
    scope: Bound<'py, PyDict>,
) -> PyResult<Py<PyAny>> {
    // Attribute lookup precedes argument conversion, just as for obj.params(...).
    let params = api.call_method0("_resolve_f10")?.getattr("params")?;
    let entry = builtin(&scope, "F10_HISTORY_ENTRY")?;
    let selector = MONEY_FLOW_HISTORY_SELECTOR.into_pyobject(py)?.into_any();
    let first_code = builtin(&scope, "split_code")?
        .call1((&code,))?
        .get_item(2)?;
    let day_text = builtin(&scope, "str")?.call1((&days,))?;
    let last_code = builtin(&scope, "split_code")?
        .call1((&code,))?
        .get_item(2)?;
    let mut arguments = vec![entry];
    arguments.extend(history_parameters(
        selector, first_code, day_text, last_code,
    ));
    let response = params.call1(PyTuple::new(py, arguments)?)?;
    let window = reverse_then_window(
        || {
            // LOAD_GLOBAL tuple precedes LOAD_GLOBAL reversed and the rows property.
            // Materialize all reversed rows even for an empty/outside window.
            let tuple = builtin(&scope, "tuple")?;
            let reversed = builtin(&scope, "reversed")?;
            tuple.call1((reversed.call1((response.getattr("rows")?,))?,))
        },
        |rows| {
            let stop = if count.is_none() {
                py.None().into_bound(py)
            } else {
                py.import("operator")?
                    .getattr("add")?
                    .call1((&start, &count))?
            };
            // A slice expression uses the actual type, not a module's `slice` name.
            let slice = py.get_type::<PySlice>().call1((&start, stop, py.None()))?;
            rows.get_item(slice)
        },
    )?;
    let payload = if include_raw.is_truthy()? {
        let dumps = builtin(&scope, "json")?.getattr("dumps")?;
        let raw = response.getattr("raw")?;
        let kwargs = PyDict::new(py);
        kwargs.set_item("ensure_ascii", false)?;
        kwargs.set_item("separators", (",", ":"))?;
        dumps
            .call((raw,), Some(&kwargs))?
            .call_method1("encode", ("utf-8",))?
    } else {
        PyBytes::new(py, b"").into_any()
    };
    let constructor = builtin(&scope, "FundFlowRangeSeries")?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("code", &code)?;
    kwargs.set_item("days", &days)?;
    let single_code = builtin(&scope, "split_code")?
        .call1((&code,))?
        .get_item(2)?;
    kwargs.set_item("codes", PyTuple::new(py, [single_code])?)?;
    kwargs.set_item("rows", window)?;
    kwargs.set_item("raw_payload", payload)?;
    Ok(constructor.call((), Some(&kwargs))?.unbind())
}
