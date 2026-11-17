//! Legacy board aggregation and ranking over shared numeric primitives.
//! Object-access lambdas and a generator shell are explicit Python adapters.
//! All selection, aggregation, history and ranking decisions live here.
use eltdx::mac::python_legacy::BOARD_MONEY_AGGREGATES;
use eltdx::__python_compat::percentage_change_with;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyIterator, PyList, PySet, PyTuple};
use crate::f10_request::builtin;

#[derive(Clone,Copy)]
enum SummaryKind { Money, Volume, Up, Down }

#[pyclass]
struct SummaryValues {
    iterator: Py<PyIterator>, module: Py<PyAny>, field: &'static str, kind: SummaryKind,
}
#[pymethods]
impl SummaryValues {
    fn __iter__(slf: PyRef<'_,Self>) -> PyRef<'_,Self> { slf }
    fn __next__(&self,py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        let mut iterator = self.iterator.bind(py).clone();
        loop {
            let Some(row) = iterator.next() else { return Ok(None); };
            let result: PyResult<Option<Py<PyAny>>> = (|| {
                py.check_signals()?; let row = row?; let module = self.module.bind(py);
                match self.kind {
                    SummaryKind::Money|SummaryKind::Volume => {
                        let convert = builtin(module,if matches!(self.kind,SummaryKind::Money) { "float" } else { "int" })?;
                        let default = if matches!(self.kind,SummaryKind::Money) { 0.0_f64.into_pyobject(py)?.into_any() }
                            else { 0_i32.into_pyobject(py)?.into_any() };
                        let mut value = row.getattr("fields")?.call_method1("get",(self.field,&default))?;
                        if !value.is_truthy()? { value = default; }
                        Ok(Some(convert.call1((value,))?.unbind()))
                    }
                    SummaryKind::Up|SummaryKind::Down => {
                        let convert = builtin(module,"float")?;
                        let mut close = row.getattr("fields")?.call_method1("get",("close",0.0))?;
                        if !close.is_truthy()? { close = 0.0_f64.into_pyobject(py)?.into_any(); }
                        let close = convert.call1((close,))?;
                        let convert = builtin(module,"float")?;
                        let mut previous = row.getattr("fields")?.call_method1("get",("pre_close",0.0))?;
                        if !previous.is_truthy()? { previous = 0.0_f64.into_pyobject(py)?.into_any(); }
                        let previous = convert.call1((previous,))?;
                        let include = if matches!(self.kind,SummaryKind::Up) { close.gt(previous)? } else { close.lt(previous)? };
                        Ok(if include { Some(1_i32.into_pyobject(py)?.into_any().unbind()) } else { None })
                    }
                }
            })();
            match result {
                Ok(Some(value)) => return Ok(Some(value)),Ok(None) => continue,
                Err(error) if error.is_instance_of::<pyo3::exceptions::PyStopIteration>(py) => {
                    let outer = pyo3::exceptions::PyRuntimeError::new_err("generator raised StopIteration");
                    outer.set_cause(py,Some(error)); return Err(outer);
                }
                Err(error) => return Err(error),
            }
        }
    }
}

fn summarize<'py>(py: Python<'py>,members: &Bound<'py,PyAny>,module: &Bound<'py,PyAny>,
    generator: &Bound<'py,PyAny>,field: &'static str,kind: SummaryKind) -> PyResult<Bound<'py,PyAny>>
{
    // Original float(sum(generator)) and int(sum(generator)) capture both
    // functions before creating the iterator, but conversions within it stay live.
    let convert = match kind { SummaryKind::Money => Some(builtin(module,"float")?),
        SummaryKind::Volume => Some(builtin(module,"int")?),_ => None };
    let sum = builtin(module,"sum")?;
    let values = Py::new(py,SummaryValues { iterator: members.try_iter()?.unbind(),
        module: module.clone().unbind(),field,kind })?;
    let value = sum.call1((generator.call1((values,))?,))?;
    match convert { Some(convert) => convert.call1((value,)),None => Ok(value) }
}

#[pyfunction]
pub fn mac_board_summary<'py>(py: Python<'py>,client: Bound<'py,PyAny>,symbol: Bound<'py,PyAny>,
    sort_type: Bound<'py,PyAny>,sort_order: Bound<'py,PyAny>,field_bit: Bound<'py,PyAny>,
    generator: Bound<'py,PyAny>,module: Bound<'py,PyAny>) -> PyResult<Py<PyAny>>
{
    let op = py.import("operator")?;
    let mut fields = module.getattr("PresetField")?.getattr("BASIC")?;
    for name in ["AMOUNT","MAIN_NET_AMOUNT","MAIN_NET_3D_AMOUNT","MAIN_NET_5D_AMOUNT"] {
        fields = op.getattr("add")?.call1((fields,field_bit.getattr(name)?))?;
    }
    let get = client.getattr("get_board_members")?; let kwargs = PyDict::new(py);
    kwargs.set_item("sort_type",sort_type)?; kwargs.set_item("sort_order",sort_order)?; kwargs.set_item("fields",fields)?;
    let members = get.call((symbol,),Some(&kwargs))?;
    let mut totals = Vec::new();
    for (_,field,_) in BOARD_MONEY_AGGREGATES {
        totals.push(summarize(py,&members,&module,&generator,field,SummaryKind::Money)?);
    }
    let volume = summarize(py,&members,&module,&generator,"vol",SummaryKind::Volume)?;
    let up = summarize(py,&members,&module,&generator,"",SummaryKind::Up)?;
    let down = summarize(py,&members,&module,&generator,"",SummaryKind::Down)?;
    let output = PyDict::new(py);
    output.set_item("member_count",builtin(&module,"len")?.call1((&members,))?)?;
    output.set_item("amount",&totals[0])?; output.set_item("vol",volume)?;
    for (index,(_,_,name)) in BOARD_MONEY_AGGREGATES.into_iter().enumerate().skip(1) {
        output.set_item(name,&totals[index])?;
    }
    output.set_item("up_count",up)?; output.set_item("down_count",down)?; output.set_item("members",members)?;
    Ok(output.into_any().unbind())
}

#[pyfunction]
pub fn mac_board_row<'py>(py: Python<'py>,client: Bound<'py,PyAny>,board: Bound<'py,PyAny>,
    change: Bound<'py,PyAny>,module: Bound<'py,PyAny>) -> PyResult<Py<PyAny>>
{
    let summary = client.call_method1("get_board_summary",(board.getattr("code")?,))?;
    let output = PyDict::new(py); output.set_item("code",board.getattr("code")?)?;
    output.set_item("name",board.getattr("name")?)?;
    output.set_item("change_pct",builtin(&module,"round")?.call1((change,2))?)?;
    for name in ["amount","vol","main_net_amount","up_count","down_count","member_count"] {
        output.set_item(name,summary.get_item(name)?)?;
    }
    Ok(output.into_any().unbind())
}

#[pyclass]
struct MetricKey { field: Py<PyAny>, module: Py<PyAny>, default: bool }
#[pymethods]
impl MetricKey {
    fn __call__(&self,py: Python<'_>,row: Bound<'_,PyAny>) -> PyResult<Py<PyAny>> {
        let convert = builtin(self.module.bind(py),"float")?;
        let value = if self.default {
            let mut value = row.call_method1("get",(self.field.bind(py),0.0))?;
            if !value.is_truthy()? { value = 0.0_f64.into_pyobject(py)?.into_any(); } value
        } else { row.get_item(self.field.bind(py))? };
        Ok(convert.call1((value,))?.unbind())
    }
}

fn prefix<'py>(rows: Bound<'py,PyAny>,end: &Bound<'py,PyAny>) -> PyResult<Bound<'py,PyAny>> {
    let py = rows.py();
    let slice = py.import("builtins")?.getattr("slice")?.call1((py.None(),end,py.None()))?;
    rows.get_item(slice)
}
fn metric_key<'py>(py: Python<'py>,module: &Bound<'py,PyAny>,field: Bound<'py,PyAny>,
    default: bool,shell: &Bound<'py,PyAny>) -> PyResult<Bound<'py,PyAny>>
{
    shell.call1((Py::new(py,MetricKey { field: field.unbind(),module: module.clone().unbind(),default })?,))
}

#[pyfunction]
pub fn mac_board_ranking<'py>(py: Python<'py>,client: Bound<'py,PyAny>,args: Bound<'py,PyTuple>,
    change_key: Bound<'py,PyAny>,key_shell: Bound<'py,PyAny>,module: Bound<'py,PyAny>) -> PyResult<Py<PyAny>>
{
    let sort_by = args.get_item(2)?;
    let valid = PySet::new(py,["change_pct","amount","main_net_amount","vol"])?;
    if !valid.contains(&sort_by)? {
        let constructor = builtin(&module,"ValueError")?;
        let message = "sort_by 必须是 {} 之一， got {!r}".into_pyobject(py)?.call_method1("format",(valid,&sort_by))?;
        return Err(PyErr::from_value(constructor.call1((message,))?));
    }
    let boards = client.call_method1("get_board_list",(args.get_item(0)?,))?;
    if !boards.is_truthy()? { return Ok(PyList::empty(py).into_any().unbind()); }
    let changes = PyDict::new(py); let op = py.import("operator")?;
    for board in boards.try_iter()? {
        py.check_signals()?; let board = board?;
        let mut pre = board.getattr("pre_close")?;
        pre = if pre.is_truthy()? { board.getattr("pre_close")? } else { builtin(&module,"float")?.call1(("nan",))? };
        let value = if pre.eq(&pre)? {
            percentage_change_with(board.getattr("price")?,&pre,
                |price,_| op.getattr("sub")?.call1((price,board.getattr("pre_close")?)),
                |change,base| op.getattr("truediv")?.call1((change,base)),
                |change| op.getattr("mul")?.call1((change,100)))?
        } else { 0.0_f64.into_pyobject(py)?.into_any() };
        changes.set_item(board.getattr("code")?,value)?;
    }
    let rows = PyList::empty(py);
    if sort_by.eq("change_pct")? {
        let sorted = builtin(&module,"sorted")?; let kwargs = PyDict::new(py);
        kwargs.set_item("key",change_key.call1((&changes,))?)?;
        kwargs.set_item("reverse",!args.get_item(3)?.is_truthy()?)?;
        let ordered = prefix(sorted.call((boards,),Some(&kwargs))?,&args.get_item(1)?)?;
        for board in ordered.try_iter()? {
            py.check_signals()?; let board = board?; let get = client.getattr("_board_row")?;
            let change = changes.as_any().get_item(board.getattr("code")?)?;
            rows.append(get.call1((board,change))?)?;
        }
        return Ok(rows.into_any().unbind());
    }
    for board in boards.try_iter()? {
        py.check_signals()?; let board = board?; let get = client.getattr("_board_row")?;
        let change = changes.as_any().get_item(board.getattr("code")?)?;
        rows.append(get.call1((board,change))?)?;
    }
    let sort = rows.getattr("sort")?; let kwargs = PyDict::new(py);
    kwargs.set_item("key",metric_key(py,&module,sort_by,true,&key_shell)?)?;
    kwargs.set_item("reverse",!args.get_item(3)?.is_truthy()?)?;
    sort.call((),Some(&kwargs))?;
    Ok(prefix(rows.into_any(),&args.get_item(1)?)?.unbind())
}

#[pyfunction]
pub fn mac_board_change_ranking<'py>(py: Python<'py>,client: Bound<'py,PyAny>,args: Bound<'py,PyTuple>,
    bar_key: Bound<'py,PyAny>,key_shell: Bound<'py,PyAny>,module: Bound<'py,PyAny>) -> PyResult<Py<PyAny>>
{
    let days = args.get_item(2)?;
    if days.lt(1)? {
        let constructor = builtin(&module,"ValueError")?;
        let message = "days 必须 >= 1，got {}".into_pyobject(py)?.call_method1("format",(&days,))?;
        return Err(PyErr::from_value(constructor.call1((message,))?));
    }
    let boards = client.call_method1("get_board_list",(args.get_item(0)?,))?;
    if !boards.is_truthy()? { return Ok(PyList::empty(py).into_any().unbind()); }
    let op = py.import("operator")?;
    let count = op.getattr("add")?.call1((&days,10))?;
    let target = module.getattr("_ymd_to_date")?.call1((args.get_item(1)?,))?;
    let rows = PyList::empty(py);
    for board in boards.try_iter()? {
        py.check_signals()?; let board = board?;
        let fetched: PyResult<Bound<'py,PyAny>> = (|| {
            let fetch = client.getattr("get_stock_kline")?; let kwargs = PyDict::new(py);
            kwargs.set_item("market",board.getattr("market")?)?; kwargs.set_item("code",board.getattr("code")?)?;
            kwargs.set_item("period",module.getattr("Period")?.getattr("DAILY")?)?; kwargs.set_item("count",&count)?;
            kwargs.set_item("adjust",module.getattr("Adjust")?.getattr("NONE")?)?;
            fetch.call((),Some(&kwargs))
        })();
        let bars = match fetched {
            Ok(bars) => bars,Err(error) => {
                if error.matches(py,module.getattr("TransportError")?)? { continue; } return Err(error);
            }
        };
        let sorted = builtin(&module,"sorted")?; let kwargs = PyDict::new(py); kwargs.set_item("key",&bar_key)?;
        let bars = sorted.call((bars,),Some(&kwargs))?;
        if builtin(&module,"len")?.call1((&bars,))?.lt(2)? { continue; }
        let end = if !target.is_none() {
            let eligible = PyList::empty(py);
            for pair in builtin(&module,"enumerate")?.call1((&bars,))?.try_iter()? {
                py.check_signals()?; let pair = crate::offline_financial::unpack(pair?,2)?;
                if pair[1].getattr("datetime")?.call_method0("date")?.le(&target)? { eligible.append(&pair[0])?; }
            }
            if eligible.is_empty() { continue; } eligible.as_any().get_item(-1)?
        } else { op.getattr("sub")?.call1((builtin(&module,"len")?.call1((&bars,))?,1))? };
        let maximum = builtin(&module,"max")?;
        let start = maximum.call1((0,op.getattr("sub")?.call1((&end,&days))?))?;
        let close_end = bars.get_item(end)?.getattr("close")?;
        let close_start = bars.get_item(start)?.getattr("close")?;
        if close_start.eq(0)? { continue; }
        let append = rows.getattr("append")?; let row = PyDict::new(py);
        row.set_item("code",board.getattr("code")?)?; row.set_item("name",board.getattr("name")?)?;
        row.set_item("close_end",&close_end)?; row.set_item("close_start",&close_start)?;
        let round = builtin(&module,"round")?;
        let change = percentage_change_with(close_end,&close_start,
            |price,base| op.getattr("sub")?.call1((price,base)),
            |change,base| op.getattr("truediv")?.call1((change,base)),
            |change| op.getattr("mul")?.call1((change,100)))?;
        row.set_item("change_pct",round.call1((change,2))?)?; append.call1((row,))?;
    }
    let sort = rows.getattr("sort")?; let kwargs = PyDict::new(py);
    kwargs.set_item("key",metric_key(py,&module,"change_pct".into_pyobject(py)?.into_any(),false,&key_shell)?)?;
    kwargs.set_item("reverse",!args.get_item(4)?.is_truthy()?)?; sort.call((),Some(&kwargs))?;
    let top = args.get_item(3)?;
    Ok(if top.is_none() { rows.into_any().unbind() } else { prefix(rows.into_any(),&top)?.unbind() })
}
