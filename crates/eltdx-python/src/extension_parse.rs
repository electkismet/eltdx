//! Source-prefix EX response policy. Shared native schemas retain wire meaning;
//! live Python struct/codec/model hooks retain the facade's object contract.
use pyo3::prelude::*;
use pyo3::types::{PyBytes,PyDict,PyList,PySlice};
use eltdx::ex::layout;
use crate::f10_request::builtin;

fn add<'py>(py: Python<'py>,value: &Bound<'py,PyAny>,offset: i32) -> PyResult<Bound<'py,PyAny>> {
    py.import("operator")?.getattr("add")?.call1((value,offset))
}
fn advance<'py>(py: Python<'py>,value: &Bound<'py,PyAny>,offset: i32) -> PyResult<Bound<'py,PyAny>> {
    py.import("operator")?.getattr("iadd")?.call1((value,offset))
}
fn length<'py>(module: &Bound<'py,PyAny>,body: &Bound<'py,PyAny>) -> PyResult<Bound<'py,PyAny>> {
    builtin(module,"len")?.call1((body,))
}
fn short<'py>(py: Python<'py>,module: &Bound<'py,PyAny>,body: &Bound<'py,PyAny>,
    pos: &Bound<'py,PyAny>,width: i32) -> PyResult<bool> {
    add(py,pos,width)?.gt(length(module,body)?)
}
fn slice<'py>(py: Python<'py>,body: &Bound<'py,PyAny>,start: &Bound<'py,PyAny>,
    end: &Bound<'py,PyAny>) -> PyResult<Bound<'py,PyAny>> {
    body.get_item(py.get_type::<PySlice>().call1((start,end))?)
}
fn fixed_slice<'py>(py: Python<'py>,body: &Bound<'py,PyAny>,start: i32,end: i32) -> PyResult<Bound<'py,PyAny>> {
    slice(py,body,&start.into_pyobject(py)?.into_any(),&end.into_pyobject(py)?.into_any())
}
fn exact<'py>(value: Bound<'py,PyAny>,count: usize) -> PyResult<Vec<Bound<'py,PyAny>>> {
    let mut iter = value.try_iter()?; let mut values = Vec::with_capacity(count);
    for index in 0..count {
        values.push(iter.next().transpose()?.ok_or_else(|| pyo3::exceptions::PyValueError::new_err(
            format!("not enough values to unpack (expected {count}, got {index})")))?);
    }
    if iter.next().transpose()?.is_some() {
        return Err(pyo3::exceptions::PyValueError::new_err(format!("too many values to unpack (expected {count})")));
    }
    Ok(values)
}
fn unpack<'py>(py: Python<'py>,module: &Bound<'py,PyAny>,fmt: &str,body: &Bound<'py,PyAny>,
    pos: &Bound<'py,PyAny>,width: i32,count: usize) -> PyResult<Vec<Bound<'py,PyAny>>> {
    let method = module.getattr("struct")?.getattr("unpack")?;
    let value = method.call1((fmt,slice(py,body,pos,&add(py,pos,width)?)?))?;
    exact(value,count)
}
fn decode<'py>(py: Python<'py>,raw: &Bound<'py,PyAny>,encoding: &str) -> PyResult<Bound<'py,PyAny>> {
    let method = raw.getattr("decode")?; let kwargs = PyDict::new(py); kwargs.set_item("errors","replace")?;
    method.call((encoding,),Some(&kwargs))
}
fn text<'py>(py: Python<'py>,raw: &Bound<'py,PyAny>,encoding: &str) -> PyResult<Bound<'py,PyAny>> {
    decode(py,raw,encoding)?.call_method1("rstrip",("\0",))
}
fn append_model<'py>(py: Python<'py>,module: &Bound<'py,PyAny>,results: &Bound<'py,PyAny>,
    name: &str,fields: &[(&str,Bound<'py,PyAny>)],body: &Bound<'py,PyAny>,
    start: &Bound<'py,PyAny>,end: &Bound<'py,PyAny>) -> PyResult<()> {
    let append = results.getattr("append")?; let constructor = module.getattr(name)?;
    let kwargs = PyDict::new(py);
    for (name,value) in fields { kwargs.set_item(*name,value)?; }
    kwargs.set_item("_raw",slice(py,body,start,end)?)?;
    append.call1((constructor.call((),Some(&kwargs))?,))?;
    Ok(())
}

#[pyfunction]
pub fn ex_response_parse<'py>(py: Python<'py>,kind: &str,command: Bound<'py,PyAny>,
    body: Bound<'py,PyAny>,module: Bound<'py,PyAny>) -> PyResult<Py<PyAny>> {
    if kind == "count" {
        if length(&module,&body)?.lt(23)? { return Ok(0_i32.into_pyobject(py)?.into_any().unbind()); }
        let value = exact(module.getattr("unpack_from")?.call1(("<I",body,19,"ex instrument count"))?,1)?;
        return Ok(builtin(&module,"int")?.call1((&value[0],))?.unbind());
    }
    if kind == "quote" { return quote_parse(py,&body,&module); }
    if kind == "quotes" { return quotes_parse(py,&command,&body,&module); }
    if kind == "bars" || kind == "range" { return bars_parse(py,kind,&command,&body,&module); }
    if kind == "markets" || kind == "info" { return directory_parse(py,kind,&body,&module); }
    let (width,fmt,count,helper,history) = match kind {
        "minute" => (12,"<B9sH",3,"GetExMinuteTimeDataCmd",false),
        "history_minute" => (20,"<B9s8sH",4,"GetExMinuteTimeDataCmd",true),
        "transactions" => (16,"<B9s4sH",4,"GetExTransactionDataCmd",false),
        "history_transactions" => (16,"<B9s4sH",4,"GetExTransactionDataCmd",true),
        _ => return Err(pyo3::exceptions::PyValueError::new_err("unknown private EX response")),
    };
    if length(&module,&body)?.lt(width)? { return Ok(PyList::empty(py).into_any().unbind()); }
    let zero = 0_i32.into_pyobject(py)?.into_any();
    let values = unpack(py,&module,fmt,&body,&zero,width,count)?;
    let method = if history { module.getattr(helper)?.getattr("_parse_records")? } else { command.getattr("_parse_records")? };
    Ok(method.call1((body,width,&values[count-1]))?.unbind())
}

fn directory_parse<'py>(py: Python<'py>,kind: &str,body: &Bound<'py,PyAny>,module: &Bound<'py,PyAny>) -> PyResult<Py<PyAny>> {
    let results = PyList::empty(py); let info = kind == "info";
    let width: i32 = if info {6} else {2};
    if length(module,body)?.lt(width)? { return Ok(results.into_any().unbind()); }
    let count = if info {
        unpack(py,module,"<IH",body,&0_i32.into_pyobject(py)?.into_any(),6,2)?[1].clone()
    } else { exact(module.getattr("struct")?.call_method1("unpack_from",("<H",body,0))?,1)?[0].clone() };
    let mut pos = width.into_pyobject(py)?.into_any();
    for item in builtin(module,"range")?.call1((count,))?.try_iter()? {
        item?; py.check_signals()?;
        let raw = if info {
            if short(py,module,body,&pos,64)? { break; }
            module.getattr("slice_bytes")?.call1((body,&pos,64,"ex instrument info record"))?
        } else {
            if add(py,&pos,64)?.le(length(module,body)?)? {
                module.getattr("slice_bytes")?.call1((body,&pos,64,"ex markets record"))?
            } else { break; }
        };
        if !info && raw.is_none() { break; }
        let method = module.getattr("struct")?.getattr("unpack")?;
        let prefix = raw.get_item(py.get_type::<PySlice>().call1((py.None(),if info {40} else {36}))?)?;
        let values = exact(method.call1((if info {"<BB3s9s17s9s"} else {"<B32sB2s"},prefix))?,if info {6} else {4})?;
        pos = advance(py,&pos,64)?;
        let (model,fields) = if info {
            let code = text(py,&values[3],"gbk")?; let name = text(py,&values[4],"gbk")?;
            let desc = text(py,&values[5],"gbk")?;
            ("ExInstrumentInfo",vec![("category",values[0].clone()),("market",values[1].clone()),
                ("code",code),("name",name),("desc",desc)])
        } else {
            if values[0].eq(0)? && values[2].eq(0)? { continue; }
            let name = text(py,&values[1],"gbk")?; let short_name = text(py,&values[3],"gbk")?;
            ("ExMarketInfo",vec![("market",values[2].clone()),("category",values[0].clone()),("name",name),("short_name",short_name)])
        };
        let append = results.getattr("append")?; let constructor = module.getattr(model)?;
        let kwargs = PyDict::new(py); for (name,value) in fields { kwargs.set_item(name,value)?; }
        kwargs.set_item("_raw",raw)?; append.call1((constructor.call((),Some(&kwargs))?,))?;
    }
    Ok(results.into_any().unbind())
}

#[pyfunction]
pub fn ex_records_parse<'py>(py: Python<'py>,kind: &str,body: Bound<'py,PyAny>,mut pos: Bound<'py,PyAny>,
    count: Bound<'py,PyAny>,module: Bound<'py,PyAny>) -> PyResult<Py<PyAny>> {
    let results = PyList::empty(py); let minute = kind == "minute";
    let (width,fmt,n) = if minute {(18,layout::MINUTE_FORMAT,5)} else {(16,layout::TRANSACTION_FORMAT,5)};
    let op = py.import("operator")?;
    for item in builtin(&module,"range")?.call1((count,))?.try_iter()? {
        item?; py.check_signals()?;
        if short(py,&module,&body,&pos,width)? { break; }
        let start = pos.clone(); let values = unpack(py,&module,fmt,&body,&pos,width,n)?;
        pos = advance(py,&pos,width)?;
        if minute {
            // Capture append/model before evaluating hour/minute and the raw slice.
            let append = results.getattr("append")?; let constructor = module.getattr("ExMinuteBar")?;
            let kwargs = PyDict::new(py);
            kwargs.set_item("hour",op.getattr("floordiv")?.call1((&values[0],60))?)?;
            kwargs.set_item("minute",op.getattr("mod")?.call1((&values[0],60))?)?;
            for (name,index) in [("price",1),("avg_price",2),("volume",3),("open_interest",4)] { kwargs.set_item(name,&values[index])?; }
            kwargs.set_item("_raw",slice(py,&body,&start,&pos)?)?;
            append.call1((constructor.call((),Some(&kwargs))?,))?;
        } else {
            let mut second = op.getattr("mod")?.call1((&values[4],10000))?;
            if second.gt(59)? { second = 0_i32.into_pyobject(py)?.into_any(); }
            let append = results.getattr("append")?; let constructor = module.getattr("ExTransactionRecord")?;
            let kwargs = PyDict::new(py);
            kwargs.set_item("hour",op.getattr("floordiv")?.call1((&values[0],60))?)?;
            kwargs.set_item("minute",op.getattr("mod")?.call1((&values[0],60))?)?;
            kwargs.set_item("second",second)?;
            for (name,index) in [("price",1),("volume",2),("zengcang",3)] { kwargs.set_item(name,&values[index])?; }
            kwargs.set_item("nature",op.getattr("floordiv")?.call1((&values[4],10000))?)?;
            kwargs.set_item("_raw",slice(py,&body,&start,&pos)?)?;
            append.call1((constructor.call((),Some(&kwargs))?,))?;
        }
    }
    Ok(results.into_any().unbind())
}

fn bars_parse<'py>(py: Python<'py>,kind: &str,command: &Bound<'py,PyAny>,body: &Bound<'py,PyAny>,
    module: &Bound<'py,PyAny>) -> PyResult<Py<PyAny>> {
    let range = kind == "range"; let results = PyList::empty(py);
    let mut pos = (if range {12_i32} else {18_i32}).into_pyobject(py)?.into_any();
    if short(py,module,body,&pos,2)? { return Ok(results.into_any().unbind()); }
    let count = unpack(py,module,"<H",body,&pos,2,1)?[0].clone(); pos = advance(py,&pos,2)?;
    for item in builtin(module,"range")?.call1((count,))?.try_iter()? {
        item?; py.check_signals()?; let start = pos.clone();
        let (values,date,clock,amount) = if range {
            if short(py,module,body,&pos,32)? { break; }
            let values = unpack(py,module,layout::RANGE_BAR_FORMAT,body,&pos,32,9)?;
            pos = advance(py,&pos,32)?;
            let date = exact(command.call_method1("_parse_date",(&values[0],))?,3)?;
            let clock = exact(command.call_method1("_parse_time",(&values[1],))?,2)?;
            let amount = values[8].clone();
            (values[2..].to_vec(),date,clock,amount)
        } else {
            let datetime = module.getattr("get_datetime")?;
            let stamp = exact(datetime.call1((command.getattr("category")?,body,&pos))?,6)?;
            pos = stamp[5].clone();
            if short(py,module,body,&pos,28)? { break; }
            let values = unpack(py,module,layout::BAR_FORMAT,body,&pos,28,7)?;
            let method = module.getattr("struct")?.getattr("unpack")?;
            let amount = exact(method.call1(("<f",slice(py,body,&add(py,&pos,16)?,&add(py,&pos,20)?)?))?,1)?[0].clone();
            pos = advance(py,&pos,28)?;
            (values,stamp[..3].to_vec(),stamp[3..5].to_vec(),amount)
        };
        let mut fields = Vec::new();
        for (name,index) in [("open",0),("high",1),("low",2),("close",3),("position",4),("trade",5)] { fields.push((name,values[index].clone())); }
        fields.push(("amount",amount));
        for (name,index) in [("year",0),("month",1),("day",2)] { fields.push((name,date[index].clone())); }
        fields.push(("hour",clock[0].clone())); fields.push(("minute",clock[1].clone()));
        append_model(py,module,results.as_any(),"ExInstrumentBar",&fields,body,&start,&pos)?;
    }
    Ok(results.into_any().unbind())
}

fn quote_parse<'py>(py: Python<'py>,body: &Bound<'py,PyAny>,module: &Bound<'py,PyAny>) -> PyResult<Py<PyAny>> {
    if length(module,body)?.lt(150)? { return Ok(py.None()); }
    let identity = unpack(py,module,"<B9s",body,&0_i32.into_pyobject(py)?.into_any(),10,2)?;
    let values = unpack(py,module,layout::QUOTE_FORMAT,body,&14_i32.into_pyobject(py)?.into_any(),136,34)?;
    let code = text(py,&identity[1],"utf-8")?;
    let constructor = module.getattr("ExInstrumentQuote")?; let kwargs = PyDict::new(py);
    kwargs.set_item("market",&identity[0])?; kwargs.set_item("code",code)?;
    for &(name,index,_) in layout::QUOTE_FIELDS { kwargs.set_item(name,&values[usize::from(index)])?; }
    kwargs.set_item("_raw",fixed_slice(py,body,0,150)?)?;
    Ok(constructor.call((),Some(&kwargs))?.unbind())
}

fn quotes_parse<'py>(py: Python<'py>,command: &Bound<'py,PyAny>,body: &Bound<'py,PyAny>,
    module: &Bound<'py,PyAny>) -> PyResult<Py<PyAny>> {
    let results = PyList::empty(py);
    if length(module,body)?.lt(2)? { return Ok(results.into_any().unbind()); }
    let count = unpack(py,module,"<H",body,&0_i32.into_pyobject(py)?.into_any(),2,1)?[0].clone();
    let mut pos = 2_i32.into_pyobject(py)?.into_any();
    for item in builtin(module,"range")?.call1((count,))?.try_iter()? {
        item?; py.check_signals()?;
        if short(py,module,body,&pos,10)? { break; }
        let identity = unpack(py,module,"<B9s",body,&pos,10,2)?;
        let stripped = identity[1].call_method1("strip",(PyBytes::new(py,b"\0"),))?;
        let code = decode(py,&stripped,"gbk")?;
        pos = advance(py,&pos,10)?;
        let helper = if command.getattr("category")?.eq(3)? {"_parse_futures"} else {"_parse_hk_stocks"};
        pos = command.call_method1(helper,(&identity[0],code,body,pos,&results))?;
    }
    Ok(results.into_any().unbind())
}

#[pyfunction]
pub fn ex_quote_list_record<'py>(py: Python<'py>,kind: &str,market: Bound<'py,PyAny>,code: Bound<'py,PyAny>,
    body: Bound<'py,PyAny>,pos: Bound<'py,PyAny>,results: Bound<'py,PyAny>,module: Bound<'py,PyAny>) -> PyResult<Py<PyAny>> {
    if short(py,&module,&body,&pos,140)? { return Ok(add(py,&pos,290)?.unbind()); }
    let (fmt,count,schema) = if kind == "futures" {(layout::FUTURES_FORMAT,34,layout::FUTURES_FIELDS)}
        else {(layout::HONG_KONG_FORMAT,35,layout::HONG_KONG_FIELDS)};
    let values = unpack(py,&module,fmt,&body,&pos,140,count)?;
    let pos = advance(py,&pos,290)?;
    let append = results.getattr("append")?; let constructor = module.getattr("OrderedDict")?;
    let fields = PyList::empty(py);
    fields.append(("market",market))?; fields.append(("code",code))?;
    for &(name,index,_) in schema { fields.append((name,&values[usize::from(index)]))?; }
    append.call1((constructor.call1((fields,))?,))?;
    Ok(pos.unbind())
}
