//! Legacy extension entry selection. Commands, transport overrides, DTOs and
//! calendar objects are explicit adapters; business choices live in Rust.
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyIterator, PyList, PyTuple};
use eltdx::mac::python_legacy::remaining_page_with;
use crate::f10_request::builtin;

#[pyfunction]
pub fn extension_single<'py>(py: Python<'py>,client: Bound<'py,PyAny>,domain: &str,kind: &str,
    args: Bound<'py,PyTuple>,bar_time: Bound<'py,PyAny>,module: Bound<'py,PyAny>) -> PyResult<Py<PyAny>>
{
    let name = match (domain,kind) {
        ("mac","quotes")|("mac_ex","quotes") => "SymbolQuotesCmd",
        ("mac","tick")|("mac_ex","tick") => "SymbolTickChartCmd",
        ("mac","ticks") => "TickChartsCmd",("mac","sampling")|("mac_ex","sampling") => "ChartSamplingCmd",
        ("mac","symbol") => "SymbolInfoCmd",("mac","belong") => "SymbolBelongBoardCmd",
        ("mac","capital") => "SymbolCapitalFlowCmd",("mac","auction") => "SymbolAuctionCmd",
        ("mac","unusual") => "UnusualCmd",("mac","server") => "ServerInfoCmd",
        ("mac","offset") => "KlineOffsetCmd",("mac","file_meta") => "FileListCmd",
        ("mac","file_chunk") => "FileDownloadCmd",("mac","goods") => "GoodsListCmd",
        ("mac_ex","bars") => "SymbolBarCmd",("mac_ex","info")|("ex","info") => "GetExInstrumentInfoCmd",
        ("ex","markets") => "GetExMarketsCmd",("ex","count") => "GetExInstrumentCountCmd",
        ("ex","quote") => "GetExInstrumentQuoteCmd",("ex","quotes") => "GetExInstrumentQuoteListCmd",
        ("ex","bars") => "GetExInstrumentBarsCmd",("ex","range") => "GetExHistoryInstrumentBarsRangeCmd",
        ("ex","minute") => "GetExMinuteTimeDataCmd",("ex","history_minute") => "GetExHistoryMinuteTimeDataCmd",
        ("ex","transactions") => "GetExTransactionDataCmd",("ex","history_transactions") => "GetExHistoryTransactionDataCmd",
        _ => return Err(pyo3::exceptions::PyValueError::new_err("unknown private extension entry")),
    };
    if domain == "ex" && kind == "range" {
        if bar_time.eq(module.getattr("BAR_TIME_END")?)? {
            let constructor = builtin(&module,"ValueError")?;
            let message = "get_history_instrument_bars_range 不支持 bar_time='end'（缺少周期信息），请改用 {!r}"
                .into_pyobject(py)?.call_method1("format",(module.getattr("BAR_TIME_START")?,))?;
            return Err(PyErr::from_value(constructor.call1((message,))?));
        }
        module.getattr("check_bar_time")?.call1((&bar_time,))?;
    }
    // MAC-EX resolves its selected fields before looking up execute. Other
    // transforms remain inside command argument evaluation, after both hooks.
    let selected = if domain == "mac_ex" && kind == "quotes" {
        let fields = args.get_item(1)?;
        Some(if fields.is_none() { module.getattr("DEFAULT_QUOTE_FIELDS")? } else { fields })
    } else { None };
    let execute = client.getattr("_execute")?; let constructor = module.getattr(name)?;
    let command = if domain == "mac" && (kind == "tick" || kind == "ticks") {
        let mut values = vec![args.get_item(0)?,args.get_item(1)?,module.getattr("_ymd_to_date")?.call1((args.get_item(2)?,))?];
        if kind == "ticks" { values.push(args.get_item(3)?); }
        constructor.call(PyTuple::new(py,values)?,None)?
    } else if domain == "mac" && kind == "unusual" {
        let mut count = args.get_item(2)?; if !count.is_truthy()? { count = 600_i32.into_pyobject(py)?.into_any(); }
        constructor.call1((args.get_item(0)?,args.get_item(1)?,count))?
    } else if let Some(selected) = selected { constructor.call1((args.get_item(0)?,selected))? }
    else if domain == "mac_ex" && kind == "bars" {
        let kwargs = PyDict::new(py);
        for (index,key) in [(2,"period"),(3,"start"),(4,"count"),(5,"fq")] { kwargs.set_item(key,args.get_item(index)?)?; }
        constructor.call((args.get_item(0)?,args.get_item(1)?),Some(&kwargs))?
    } else { constructor.call(&args,None)? };
    let value = execute.call1((command,))?;
    if domain == "ex" && kind == "bars" {
        let align = module.getattr("align_hour_minute_bars")?; let kwargs = PyDict::new(py);
        kwargs.set_item("delta_minutes",module.getattr("category_to_minutes")?.call1((args.get_item(0)?,))?)?;
        kwargs.set_item("bar_time",bar_time)?;
        return Ok(align.call((value,),Some(&kwargs))?.unbind());
    }
    Ok(value.unbind())
}

#[pyfunction]
pub fn mac_ex_quotes_list<'py>(py: Python<'py>,client: Bound<'py,PyAny>,market: Bound<'py,PyAny>,
    start: Bound<'py,PyAny>,count: Bound<'py,PyAny>,module: Bound<'py,PyAny>) -> PyResult<Py<PyAny>>
{
    let size = builtin(&module,"min")?.call1((count,80))?;
    let get = client.getattr("goods_list")?; let kwargs = PyDict::new(py);
    kwargs.set_item("start",start)?; kwargs.set_item("count",size)?;
    let items = get.call((&market,),Some(&kwargs))?;
    if !items.is_truthy()? { return Ok(PyList::empty(py).into_any().unbind()); }
    let unfiltered = PyList::empty(py);
    for item in items.try_iter()? {
        py.check_signals()?; let item = item?;
        unfiltered.append(PyTuple::new(py,[market.clone(),module.getattr("_first_code")?.call1((item,))?])?)?;
    }
    let codes = PyList::empty(py);
    for pair in unfiltered.try_iter()? {
        let pair = crate::offline_financial::unpack(pair?,2)?;
        if pair[1].is_truthy()? { codes.append(PyTuple::new(py,pair)?)?; }
    }
    if codes.is_empty() { return Ok(codes.into_any().unbind()); }
    let execute = client.getattr("_execute")?;
    let command = module.getattr("SymbolQuotesCmd")?.call1((codes,module.getattr("DEFAULT_QUOTE_FIELDS")?))?;
    Ok(execute.call1((command,))?.unbind())
}

#[pyfunction]
pub fn mac_ex_transactions<'py>(py: Python<'py>,client: Bound<'py,PyAny>,args: Bound<'py,PyTuple>,
    all: bool,module: Bound<'py,PyAny>) -> PyResult<Py<PyAny>>
{
    let hk = module.getattr("is_hk_stock_market")?.call1((args.get_item(0)?,))?.is_truthy()?;
    if all {
        if !hk {
            let constructor = builtin(&module,"ValueError")?;
            let message = "goods_transaction_all 仅支持港股股票类市场（HK_STOCK_MARKETS），收到 market={}；其他市场请用 goods_transaction。"
                .into_pyobject(py)?.call_method1("format",(args.get_item(0)?,))?;
            return Err(PyErr::from_value(constructor.call1((message,))?));
        }
        let fetch = module.getattr("_fetch_all_hk_transactions_sync")?;
        return Ok(fetch.call1((client.getattr("_execute")?,args.get_item(0)?,args.get_item(1)?,args.get_item(2)?))?.unbind());
    }
    if hk {
        let fetch = module.getattr("_fetch_hk_transactions_sync")?;
        return Ok(fetch.call1((client.getattr("_execute")?,args.get_item(0)?,args.get_item(1)?,args.get_item(2)?,
            args.get_item(3)?,args.get_item(4)?))?.unbind());
    }
    let execute = client.getattr("_execute")?;
    Ok(execute.call1((module.getattr("SymbolTransactionCmd")?.call(&args,None)?,))?.unbind())
}

#[pyfunction]
pub fn hk_is_stock(market: Bound<'_,PyAny>,module: Bound<'_,PyAny>) -> PyResult<bool> {
    module.getattr("HK_STOCK_MARKETS")?.contains(market)
}

#[pyfunction]
pub fn hk_to_ymd<'py>(py: Python<'py>,date: Bound<'py,PyAny>) -> PyResult<Py<PyAny>> {
    let op = py.import("operator")?;
    let year = op.getattr("mul")?.call1((date.getattr("year")?,10000))?;
    let month = op.getattr("mul")?.call1((date.getattr("month")?,100))?;
    let partial = op.getattr("add")?.call1((year,month))?;
    Ok(op.getattr("add")?.call1((partial,date.getattr("day")?))?.unbind())
}

#[pyfunction]
pub fn hk_build_command<'py>(py: Python<'py>,args: Bound<'py,PyTuple>,module: Bound<'py,PyAny>) -> PyResult<Py<PyAny>> {
    let ymd = args.get_item(2)?;
    let constructor = module.getattr(if ymd.is_none() { "GetExTransactionDataCmd" } else { "GetExHistoryTransactionDataCmd" })?;
    let mut values = vec![args.get_item(0)?,args.get_item(1)?];
    if !ymd.is_none() { values.push(ymd); } values.push(args.get_item(3)?); values.push(args.get_item(4)?);
    Ok(constructor.call(PyTuple::new(py,values)?,None)?.unbind())
}

#[pyfunction]
pub fn hk_map_record<'py>(py: Python<'py>,record: Bound<'py,PyAny>,module: Bound<'py,PyAny>) -> PyResult<Py<PyAny>> {
    let constructor = module.getattr("MacTransaction")?; let kwargs = PyDict::new(py);
    let time = module.getattr("time")?;
    kwargs.set_item("time",time.call1((record.getattr("hour")?,record.getattr("minute")?,record.getattr("second")?))?)?;
    kwargs.set_item("price",eltdx::mac::routing::hk_price_with(record.getattr("price")?,module.getattr("_HK_PRICE_DIVISOR")?,
        |value,divisor| py.import("operator")?.getattr("truediv")?.call1((value,divisor)))?)?;
    kwargs.set_item("vol",record.getattr("volume")?)?; kwargs.set_item("trade_count",0)?;
    kwargs.set_item("bs_flag",record.getattr("nature")?)?;
    Ok(constructor.call((),Some(&kwargs))?.unbind())
}

#[pyclass]
struct HkRecords { iterator: Py<PyIterator>,module: Py<PyAny> }
#[pymethods]
impl HkRecords {
    fn __iter__(slf: PyRef<'_,Self>) -> PyRef<'_,Self> { slf }
    fn __next__(&self,py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        let mut iterator = self.iterator.bind(py).clone();
        let Some(row) = iterator.next() else { return Ok(None); };
        let result: PyResult<Py<PyAny>> = (|| {
            let row = row?; py.check_signals()?;
            Ok(self.module.bind(py).getattr("_map_record")?.call1((row,))?.unbind())
        })();
        match result {
            Err(error) if error.is_instance_of::<pyo3::exceptions::PyStopIteration>(py) => {
                let outer = pyo3::exceptions::PyRuntimeError::new_err("generator raised StopIteration");
                outer.set_cause(py,Some(error)); Err(outer)
            }
            value => value.map(Some),
        }
    }
}

#[pyfunction]
pub fn hk_transaction_pages<'py>(py: Python<'py>,execute: Bound<'py,PyAny>,args: Bound<'py,PyTuple>,
    all: bool,generator: Bound<'py,PyAny>,module: Bound<'py,PyAny>) -> PyResult<Py<PyAny>>
{
    let date = args.get_item(2)?;
    let ymd = if date.is_none() { date } else { module.getattr("_to_ymd")?.call1((date,))? };
    let results = PyList::empty(py); let mut fetched = 0_i32.into_pyobject(py)?.into_any(); let mut offset = args.get_item(3)?;
    let count = if all { py.None().into_bound(py) } else { args.get_item(4)? };
    let op = py.import("operator")?;
    let mut pages = if all { Some(builtin(&module,"range")?.call1((module.getattr("_HK_TRANSACTION_MAX_PAGES")?,))?.try_iter()?) } else { None };
    loop {
        if let Some(pages) = pages.as_mut() { if let Some(page) = pages.next() { page?; } else { break; } }
        else if !fetched.lt(&count)? { break; }
        py.check_signals()?;
        let page = if all { None } else {
            let minimum = builtin(&module,"min")?;
            Some(remaining_page_with(&count,&fetched,|target,seen| op.getattr("sub")?.call1((target,seen)),
                || module.getattr("_HK_TRANSACTION_PAGE_SIZE"),|remaining,maximum| minimum.call1((remaining,maximum)))?)
        };
        let build = module.getattr("_build_cmd")?;
        let command = build.call1((args.get_item(0)?,args.get_item(1)?,&ymd,&offset,
            match &page { Some(page) => page.clone(),None => module.getattr("_HK_TRANSACTION_PAGE_SIZE")? }))?;
        let batch = execute.call1((command,))?;
        if !batch.is_truthy()? { break; }
        let extend = results.getattr("extend")?;
        let values = Py::new(py,HkRecords { iterator: batch.try_iter()?.unbind(),module: module.clone().unbind() })?;
        extend.call1((generator.call1((values,))?,))?;
        if !all { fetched = op.getattr("iadd")?.call1((fetched,builtin(&module,"len")?.call1((&batch,))?))?; }
        offset = op.getattr("iadd")?.call1((offset,builtin(&module,"len")?.call1((&batch,))?))?;
        let length = builtin(&module,"len")?.call1((batch,))?;
        let maximum = match page { Some(page) => page,None => module.getattr("_HK_TRANSACTION_PAGE_SIZE")? };
        if length.lt(maximum)? { break; }
    }
    Ok(results.into_any().unbind())
}
