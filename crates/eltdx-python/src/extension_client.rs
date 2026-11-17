//! Legacy extension business orchestration. Command/transport overrides stay
//! explicit Python object adapters; no strict whole-operation cap is added.
use eltdx::mac::python_legacy::{remaining_page_with, repair_needed_with};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};
use crate::f10_request::builtin;

#[derive(Clone,Copy,PartialEq)]
enum PageKind { Quotes, Members, Bars, Transactions, Boards }
struct PageCall<'py> {
    kind: PageKind, args: Bound<'py,PyTuple>, module: Bound<'py,PyAny>,
    fields: Option<Bound<'py,PyAny>>, board: Option<Bound<'py,PyAny>>, date: Option<Bound<'py,PyAny>>,
}
impl<'py> PageCall<'py> {
    fn arg(&self,index: usize) -> PyResult<Bound<'py,PyAny>> { self.args.get_item(index) }
    fn maximum(&self) -> PyResult<Bound<'py,PyAny>> {
        match self.kind {
            PageKind::Quotes|PageKind::Members => self.module.getattr("_BOARD_MEMBERS_PAGE_SIZE"),
            PageKind::Bars => self.module.getattr("_KLINE_PAGE_SIZE"),
            PageKind::Transactions => self.module.getattr("_TRANSACTION_PAGE_SIZE"),
            PageKind::Boards => Ok(150.into_pyobject(self.module.py())?.into_any()),
        }
    }
    fn request(&self,client: &Bound<'py,PyAny>,offset: &Bound<'py,PyAny>,size: &Bound<'py,PyAny>) -> PyResult<Bound<'py,PyAny>> {
        let execute = client.getattr("_execute")?;
        let command = self.command(offset,size)?;
        execute.call1((command,))
    }
    fn command(&self,offset: &Bound<'py,PyAny>,size: &Bound<'py,PyAny>) -> PyResult<Bound<'py,PyAny>> {
        let py = self.module.py();
        match self.kind {
            PageKind::Quotes|PageKind::Members => {
                let constructor = self.module.getattr("BoardMembersQuotesCmd")?;
                let kwargs = PyDict::new(py);
                let board = if self.kind == PageKind::Quotes { builtin(&self.module,"int")?.call1((self.arg(0)?,))? }
                    else { self.board.as_ref().expect("prepared member board").clone() };
                kwargs.set_item("board_code",board)?;
                kwargs.set_item("sort_type",self.arg(if self.kind == PageKind::Quotes { 3 } else { 2 })?)?;
                kwargs.set_item("start",offset)?; kwargs.set_item("page_size",size)?;
                kwargs.set_item("sort_order",self.arg(if self.kind == PageKind::Quotes { 4 } else { 3 })?)?;
                kwargs.set_item("fields",self.fields.as_ref().expect("prepared quote fields"))?;
                kwargs.set_item("exclude_flags",self.arg(5)?)?;
                constructor.call((),Some(&kwargs))
            }
            PageKind::Bars => {
                let constructor = self.module.getattr("SymbolBarCmd")?; let kwargs = PyDict::new(py);
                for (name,index) in [("market",0),("code",1),("period",2),("times",5)] { kwargs.set_item(name,self.arg(index)?)?; }
                kwargs.set_item("start",offset)?; kwargs.set_item("count",size)?; kwargs.set_item("fq",self.arg(6)?)?;
                constructor.call((),Some(&kwargs))
            }
            PageKind::Transactions => self.module.getattr("SymbolTransactionCmd")?.call1((
                self.arg(0)?,self.arg(1)?,self.date.as_ref().expect("prepared transaction date"),offset,size)),
            PageKind::Boards => self.module.getattr("BoardListCmd")?.call1((self.arg(0)?,offset,size)),
        }
    }
}

#[pyfunction]
pub fn mac_page_query<'py>(py: Python<'py>, client: Bound<'py,PyAny>, kind: &str,
    args: Bound<'py,PyTuple>, module: Bound<'py,PyAny>) -> PyResult<Py<PyAny>>
{
    let kind = match kind { "quotes" => PageKind::Quotes,"members" => PageKind::Members,
        "bars" => PageKind::Bars,"transactions" => PageKind::Transactions,"boards" => PageKind::Boards,
        _ => return Err(pyo3::exceptions::PyValueError::new_err("unknown private MAC paging mode")), };
    let mut call = PageCall { kind,args,module,fields: None,board: None,date: None };
    let op = py.import("operator")?;
    let (count_index,start_index) = match kind {
        PageKind::Quotes => (2,Some(1)),PageKind::Members => (1,None),PageKind::Bars => (4,Some(3)),
        PageKind::Transactions => (2,Some(3)),PageKind::Boards => (1,None),
    };
    if kind == PageKind::Quotes {
        let fields = call.arg(6)?;
        call.fields = Some(if fields.is_none() {
            let basic = call.module.getattr("PresetField")?.getattr("BASIC")?;
            op.getattr("add")?.call1((basic,call.module.getattr("PresetField")?.getattr("VOLUME")?))?
        } else { fields });
    } else if kind == PageKind::Members {
        call.board = Some(call.module.getattr("convert_board_code")?.call1((call.arg(0)?,))?);
        call.fields = Some(call.arg(4)?);
    } else if kind == PageKind::Transactions {
        call.date = Some(call.module.getattr("_ymd_to_date")?.call1((call.arg(4)?,))?);
    }
    let count = call.arg(count_index)?;
    let start = match start_index { Some(index) => call.arg(index)?,None => 0.into_pyobject(py)?.into_any() };
    let mut fetched = 0.into_pyobject(py)?.into_any(); let mut offset = start.clone();
    let mut output = PyList::empty(py).into_any();
    if kind == PageKind::Transactions || kind == PageKind::Boards {
        // These legacy methods issue an initial request even for count<=0,
        // and return the actual first result object with subsequent extension.
        let execute = client.getattr("_execute")?;
        let constructor = call.module.getattr(if kind == PageKind::Transactions { "SymbolTransactionCmd" } else { "BoardListCmd" })?;
        let minimum = builtin(&call.module,"min")?;
        let size = minimum.call1((&count,call.maximum()?))?;
        let command = if kind == PageKind::Transactions { constructor.call1((call.arg(0)?,call.arg(1)?,
            call.date.as_ref().expect("prepared date"),&start,size))? }
            else { constructor.call1((call.arg(0)?,0,size))? };
        output = execute.call1((command,))?;
        fetched = builtin(&call.module,"len")?.call1((&output,))?;
        offset = if kind == PageKind::Boards { fetched.clone() } else { op.getattr("add")?.call1((start,&fetched))? };
    }
    while fetched.lt(&count)? {
        py.check_signals()?;
        let minimum = builtin(&call.module,"min")?;
        let size = remaining_page_with(&count,&fetched,|target,seen| op.getattr("sub")?.call1((target,seen)),
            || call.maximum(),|remaining,maximum| minimum.call1((remaining,maximum)))?;
        let batch = call.request(&client,&offset,&size)?;
        if !batch.is_truthy()? { break; }
        if kind == PageKind::Bars { output = op.getattr("add")?.call1((&batch,output))?; }
        else { output.call_method1("extend",(&batch,))?; }
        fetched = op.getattr("iadd")?.call1((fetched,builtin(&call.module,"len")?.call1((&batch,))?))?;
        offset = op.getattr("iadd")?.call1((offset,builtin(&call.module,"len")?.call1((&batch,))?))?;
        if builtin(&call.module,"len")?.call1((batch,))?.lt(size)? { break; }
    }
    Ok(output.unbind())
}

#[pyfunction]
pub fn mac_local_recompute<'py>(client: Bound<'py,PyAny>, bars: Bound<'py,PyAny>, market: Bound<'py,PyAny>,
    code: Bound<'py,PyAny>, module: Bound<'py,PyAny>) -> PyResult<Py<PyAny>>
{
    let py = client.py();
    if client.getattr("_xdxr_provider")?.is_none() { return Ok(py.None()); }
    let events = client.getattr("_xdxr_provider")?.call1((market,code))?;
    if !events.is_truthy()? { return Ok(py.None()); }
    let out = module.getattr("apply_forward_adjust")?.call1((bars,events))?;
    if module.getattr("has_bad_prices")?.call1((&out,))?.is_truthy()? { return Ok(py.None()); }
    Ok(out.unbind())
}

#[pyfunction]
pub fn mac_stock_kline<'py>(py: Python<'py>, client: Bound<'py,PyAny>, args: Bound<'py,PyTuple>,
    bar_time: Bound<'py,PyAny>, module: Bound<'py,PyAny>) -> PyResult<Py<PyAny>>
{
    let mut bars = client.getattr("_fetch_kline_pages")?.call(&args,None)?;
    let adjust = args.get_item(6)?;
    if repair_needed_with(|| adjust.eq(module.getattr("Adjust")?.getattr("QFQ")?),|| bars.is_truthy(),
        || module.getattr("has_bad_prices")?.call1((&bars,))?.is_truthy())? {
        let fetch = client.getattr("_fetch_kline_pages")?;
        let mut plain = Vec::new(); for index in 0..6 { plain.push(args.get_item(index)?); }
        plain.push(module.getattr("Adjust")?.getattr("NONE")?);
        let none_bars = fetch.call(PyTuple::new(py,plain)?,None)?;
        if none_bars.is_truthy()? {
            let recomputed = client.call_method1("_local_recompute_qfq",(none_bars,args.get_item(0)?,args.get_item(1)?))?;
            if !recomputed.is_none() { bars = recomputed; }
        }
    }
    let align = module.getattr("align_datetime_bars")?; let kwargs = PyDict::new(py);
    kwargs.set_item("delta_minutes",module.getattr("period_to_minutes")?.call1((args.get_item(2)?,args.get_item(5)?))?)?;
    kwargs.set_item("bar_time",bar_time)?;
    Ok(align.call((bars,),Some(&kwargs))?.unbind())
}

#[pyfunction]
pub fn mac_download_file<'py>(py: Python<'py>, client: Bound<'py,PyAny>, filename: Bound<'py,PyAny>,
    mut size: Bound<'py,PyAny>, module: Bound<'py,PyAny>) -> PyResult<Py<PyAny>>
{
    if size.le(0)? {
        let execute = client.getattr("_execute")?;
        size = execute.call1((module.getattr("FileListCmd")?.call1((&filename,))?,))?.getattr("size")?;
    }
    let output = builtin(&module,"bytearray")?.call0()?;
    let mut offset = 0.into_pyobject(py)?.into_any(); let mut index = 1.into_pyobject(py)?.into_any();
    let op = py.import("operator")?;
    while offset.lt(&size)? {
        py.check_signals()?;
        let execute = client.getattr("_execute")?;
        let command = module.getattr("FileDownloadCmd")?.call1((&filename,&index,&offset,30000))?;
        let chunk = execute.call1((command,))?;
        if !chunk.is_truthy()? { break; }
        output.call_method1("extend",(&chunk,))?;
        offset = op.getattr("iadd")?.call1((offset,builtin(&module,"len")?.call1((&chunk,))?))?;
        index = op.getattr("iadd")?.call1((index,1))?;
        if builtin(&module,"len")?.call1((chunk,))?.lt(30000)? { break; }
    }
    Ok(output.unbind())
}
