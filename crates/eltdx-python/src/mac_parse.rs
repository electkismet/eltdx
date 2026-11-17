//! MAC response orchestration with the original Python buffer/model adapters.
//! Strict native decoders keep their independently specified validation policy.
use pyo3::prelude::*;
use pyo3::types::{PyBytes,PyDict,PyList,PySlice};
use crate::f10_request::builtin;
use eltdx::mac::layout;

struct Reader<'py> {
    py: Python<'py>, module: Bound<'py,PyAny>, body: Bound<'py,PyAny>, operator: Bound<'py,PyAny>,
}
impl<'py> Reader<'py> {
    fn n(&self,value: i32) -> PyResult<Bound<'py,PyAny>> {Ok(value.into_pyobject(self.py)?.into_any())}
    fn s(&self,value: &str) -> PyResult<Bound<'py,PyAny>> {Ok(value.into_pyobject(self.py)?.into_any())}
    fn binary(&self,name: &str,a: &Bound<'py,PyAny>,b: &Bound<'py,PyAny>) -> PyResult<Bound<'py,PyAny>> {
        self.operator.getattr(name)?.call1((a,b))
    }
    fn calc(&self,name: &str,a: &Bound<'py,PyAny>,b: i32) -> PyResult<Bound<'py,PyAny>> {self.binary(name,a,&self.n(b)?)}
    fn left(&self,name: &str,a: i32,b: &Bound<'py,PyAny>) -> PyResult<Bound<'py,PyAny>> {self.binary(name,&self.n(a)?,b)}
    fn len(&self,value: &Bound<'py,PyAny>) -> PyResult<Bound<'py,PyAny>> {builtin(&self.module,"len")?.call1((value,))}
    fn size(&self) -> PyResult<Bound<'py,PyAny>> {self.len(&self.body)}
    fn range(&self,count: &Bound<'py,PyAny>) -> PyResult<Bound<'py,PyAny>> {builtin(&self.module,"range")?.call1((count,))}
    fn format(&self,text: &str,values: &[Bound<'py,PyAny>]) -> PyResult<Bound<'py,PyAny>> {
        self.s(text)?.call_method1("format",pyo3::types::PyTuple::new(self.py,values)?)
    }
    fn slice(&self,start: &Bound<'py,PyAny>,end: Option<&Bound<'py,PyAny>>) -> PyResult<Bound<'py,PyAny>> {
        self.body.get_item(self.py.get_type::<PySlice>().call1((start,end.map_or_else(||self.py.None().into_bound(self.py),Clone::clone)))?)
    }
    fn at(&self,fmt: &str,pos: i32,context: &str,count: usize) -> PyResult<Vec<Bound<'py,PyAny>>> {
        exact(self.module.getattr("unpack_from")?.call1((fmt,&self.body,pos,context))?,count)
    }
    fn row(&self,fmt: &str,pos: &Bound<'py,PyAny>,context: &str,index: &Bound<'py,PyAny>,count: usize) -> PyResult<Vec<Bound<'py,PyAny>>> {
        let method = self.module.getattr("unpack_from")?;
        exact(method.call1((fmt,&self.body,pos,self.format(context,&[index.clone()])?))?,count)
    }
    fn text(&self,raw: &Bound<'py,PyAny>,encoding: &str) -> PyResult<Bound<'py,PyAny>> {
        let method = raw.getattr("decode")?; let kwargs = PyDict::new(self.py); kwargs.set_item("errors","replace")?;
        method.call((encoding,),Some(&kwargs))?.call_method1("rstrip",("\0",))
    }
    fn decoded(&self,raw: &Bound<'py,PyAny>) -> PyResult<Bound<'py,PyAny>> {self.module.getattr("_decode_gbk")?.call1((raw,))}
    fn clock(&self,raw: &Bound<'py,PyAny>) -> PyResult<Bound<'py,PyAny>> {
        let constructor = self.module.getattr("time")?;
        let hour = self.calc("floordiv",raw,3600)?;
        let minute = self.calc("floordiv",&self.calc("mod",raw,3600)?,60)?;
        let second = self.calc("mod",raw,60)?;
        constructor.call1((hour,minute,second))
    }
    fn require(&self,pos: &Bound<'py,PyAny>,width: &Bound<'py,PyAny>,context: Bound<'py,PyAny>) -> PyResult<()> {
        self.module.getattr("require_bytes")?.call1((&self.body,pos,width,context))?; Ok(())
    }
}
fn exact<'py>(value: Bound<'py,PyAny>,count: usize) -> PyResult<Vec<Bound<'py,PyAny>>> {
    let mut iterator = value.try_iter()?; let mut result = Vec::with_capacity(count);
    for index in 0..count {result.push(iterator.next().transpose()?.ok_or_else(||pyo3::exceptions::PyValueError::new_err(
        format!("not enough values to unpack (expected {count}, got {index})")))?);}
    if iterator.next().transpose()?.is_some() {return Err(pyo3::exceptions::PyValueError::new_err(format!("too many values to unpack (expected {count})")));}
    Ok(result)
}
// Capture the live DTO constructor before evaluating fields, as CALL does.
macro_rules! model {
    ($r:expr,$name:expr,$($key:expr => $value:expr),* $(,)?) => {{
        let constructor = $r.module.getattr($name)?; let kwargs = PyDict::new($r.py);
        $(kwargs.set_item($key,$value)?;)*
        constructor.call((),Some(&kwargs))?
    }};
}

#[pyfunction]
pub fn mac_numeric_value<'py>(_py: Python<'py>,integer: bool,value: Bound<'py,PyAny>,module: Bound<'py,PyAny>) -> PyResult<Py<PyAny>> {
    let int = if integer {Some(builtin(&module,"int")?)} else {None};
    let float = builtin(&module,"float")?;
    let cast = module.getattr("cast")?.call1(("float",value))?;
    let result = float.call1((cast,))?;
    let result = if let Some(int) = int {int.call1((result,))?} else {result};
    Ok(result.unbind())
}
#[pyfunction]
pub fn mac_numeric_default(py: Python<'_>,integer: bool) -> PyResult<Py<PyAny>> {
    let result = if integer {0.into_pyobject(py)?.into_any()} else {0.0.into_pyobject(py)?.into_any()};
    Ok(result.unbind())
}

#[pyfunction]
pub fn mac_response_parse<'py>(py: Python<'py>,kind: &str,command: Bound<'py,PyAny>,body: Bound<'py,PyAny>,module: Bound<'py,PyAny>) -> PyResult<Py<PyAny>> {
    let r = Reader {py,module,body,operator: py.import("operator")?.into_any()};
    match kind {
        "quotes" | "members" => quotes(&r,kind),
        "bars" => bars(&r,&command),
        "minute" | "multi_minute" => charts(&r,&command,kind),
        "unusual" => unusual(&r),
        "transactions" | "auction" => trades(&r,kind),
        "info" => info(&r),
        "boards" => boards(&r),
        "belong" | "capital" => json(&r,kind),
        "server" => server(&r),
        "offsets" => offsets(&r),
        "sampling" | "goods" => inventory(&r,&command,kind),
        "file_meta" => file_meta(&r),
        "file_download" => {
            let result = if r.size()?.lt(8)? {PyBytes::new(py,b"").into_any()} else {r.slice(&r.n(8)?,None)?};
            Ok(result.unbind())
        }
        _ => Err(pyo3::exceptions::PyValueError::new_err("unknown private MAC response")),
    }
}
fn quotes<'py>(r: &Reader<'py>,kind: &str) -> PyResult<Py<PyAny>> {
    let members = kind == "members";
    let mut pos = if members {r.n(0)?} else {r.module.getattr("RESP_BITMAP_SIZE")?};
    let bitmap;
    let header;
    if members {
        bitmap = r.body.get_item(r.py.get_type::<PySlice>().call1((r.py.None(),r.module.getattr("RESP_BITMAP_SIZE")?))?)?;
        let method = r.module.getattr("unpack_from")?;
        header = exact(method.call1(("<IH",&r.body,r.module.getattr("RESP_BITMAP_SIZE")?,"board_members header"))?,2)?;
    } else {
        header = exact(r.module.getattr("unpack_from")?.call1(("<IH",&r.body,&pos,"symbol_quotes header"))?,2)?;
        pos = r.calc("iadd",&pos,6)?;
        bitmap = r.body.get_item(r.py.get_type::<PySlice>().call1((r.py.None(),r.module.getattr("RESP_BITMAP_SIZE")?))?)?;
    }
    let list = builtin(&r.module,"list")?;
    let slots = list.call1((r.module.getattr("iter_set_bits")?.call1((&bitmap,))?,))?;
    let count = r.module.getattr("count_set_bits")?.call1((&bitmap,))?;
    let fields_width = if members {r.calc("mul",&count,4)?} else {r.left("mul",4,&count)?};
    let width = r.left("add",68,&fields_width)?;
    let results = PyList::empty(r.py);
    for index in r.range(&header[1])?.try_iter()? {
        let index = index?; r.py.check_signals()?;
        let (raw,market,code,name,start) = if members {
            let start = r.left("add",26,&r.binary("mul",&index,&width)?)?;
            let method = r.module.getattr("unpack_from")?;
            let market = method.call1(("<H",&r.body,&start,r.format("board_members row[{}] market",&[index])?))?.get_item(0)?;
            let code = r.slice(&r.calc("add",&start,2)?,Some(&r.calc("add",&start,24)?))?;
            let name = r.slice(&r.calc("add",&start,24)?,Some(&r.calc("add",&start,68)?))?;
            (r.body.clone(),market,code,name,start)
        } else {
            let end = r.binary("add",&pos,&width)?;
            if end.gt(r.size()?)? {break;}
            let raw = r.slice(&pos,Some(&end))?; pos = end;
            let id = exact(r.module.getattr("unpack_from")?.call1(("<H22s44s",&raw,0,"symbol_quotes row"))?,3)?;
            (raw,id[0].clone(),id[1].clone(),id[2].clone(),r.n(0)?)
        };
        let fields = PyDict::new(r.py);
        for slot in slots.try_iter()? {
            let slot = exact(slot?,3)?; let field = &slot[1];
            if field.is_none() {continue;}
            let (a,b) = if members {
                (r.binary("add",&r.calc("add",&start,68)?,&r.calc("mul",&slot[0],4)?)?,
                 r.binary("add",&r.calc("add",&start,68)?,&r.calc("mul",&r.calc("add",&slot[0],1)?,4)?)?)
            } else {
                (r.left("add",68,&r.calc("mul",&slot[0],4)?)?,r.left("add",68,&r.calc("mul",&r.calc("add",&slot[0],1)?,4)?)?)
            };
            let bytes = raw.get_item(r.py.get_type::<PySlice>().call1((a,b))?)?;
            if members && r.len(&bytes)?.lt(4)? {break;}
            let mut value = exact(r.module.getattr("struct")?.call_method1("unpack",(&slot[2],bytes))?,1)?.remove(0);
            let method = r.module.getattr("FIELD_POSTPROCESS")?.getattr("get")?;
            let post = method.call1((field.getattr("value")?,))?;
            if !post.is_none() {let cast = r.module.getattr("cast")?; value = cast.call1(("object",post.call1((&value,&market))?))?;}
            fields.set_item(field.getattr("field_name")?,value)?;
        }
        let constructor = r.module.getattr("MacQuoteField")?; let kwargs = PyDict::new(r.py);
        kwargs.set_item("market",market)?;
        kwargs.set_item("code",if members {r.text(&code,"gbk")?} else {r.decoded(&code)?})?;
        kwargs.set_item("name",if members {r.text(&name,"gbk")?} else {r.decoded(&name)?})?;
        kwargs.set_item("fields",fields)?; results.append(constructor.call((),Some(&kwargs))?)?;
    }
    Ok(results.into_any().unbind())
}
fn bars<'py>(r: &Reader<'py>,command: &Bound<'py,PyAny>) -> PyResult<Py<PyAny>> {
    let header = r.at("<HBHI",24,"symbol_bar header",4)?;
    let minimum = builtin(&r.module,"min")?; let maximum = builtin(&r.module,"max")?;
    let count = minimum.call1((&header[2],maximum.call1((0,r.calc("floordiv",&r.calc("sub",&r.size()?,33)?,36)?))?))?;
    let mut intraday = r.binary("lt",&command.getattr("_period")?,&r.module.getattr("Period")?.getattr("DAILY")?)?;
    for name in ["MIN_1","MINS","SECONDS"] {
        if intraday.is_truthy()? {break;}
        intraday = r.binary("eq",&command.getattr("_period")?,&r.module.getattr("Period")?.getattr(name)?)?;
    }
    let results = PyList::empty(r.py);
    for index in r.range(&count)?.try_iter()? {
        let index = index?; r.py.check_signals()?; let pos = r.left("add",33,&r.calc("mul",&index,36)?)?;
        if r.calc("add",&pos,36)?.gt(r.size()?)? {break;}
        let values = r.row("<II7f",&pos,"symbol_bar bar[{}]",&index,9)?;
        if values[0].lt(19900101)? || values[0].gt(20991231)? {continue;}
        results.append(model!(r,"MacBar","datetime" => r.module.getattr("_combine_bar_datetime")?.call1((&values[0],&values[1],&intraday))?,
            "open" => &values[2],"high" => &values[3],"low" => &values[4],"close" => &values[5],"vol" => &values[7],"amount" => &values[6],"float_shares" => &values[8]))?;
    }
    Ok(results.into_any().unbind())
}
fn minute_clock<'py>(r: &Reader<'py>,value: &Bound<'py,PyAny>) -> PyResult<Bound<'py,PyAny>> {
    let method = r.module.getattr("time")?;
    method.call1((r.calc("mod",&r.calc("floordiv",value,60)?,24)?,r.calc("mod",value,60)?))
}
fn chart_model<'py>(r: &Reader<'py>,name: &str,id: &[Bound<'py,PyAny>],tail: &[Bound<'py,PyAny>],ticks: Bound<'py,PyAny>) -> PyResult<Py<PyAny>> {
    Ok(model!(r,name,"market" => &id[0],"code" => r.decoded(&id[1])?,"name" => r.decoded(&tail[0])?,
        "pre_close" => &tail[6],"open" => &tail[7],"high" => &tail[8],"low" => &tail[9],"close" => &tail[10],
        "vol" => builtin(&r.module,"int")?.call1((&tail[12],))?,"amount" => &tail[13],"turnover" => &tail[15],"avg" => &tail[16],"charts" => ticks).unbind())
}
fn charts<'py>(r: &Reader<'py>,command: &Bound<'py,PyAny>,kind: &str) -> PyResult<Py<PyAny>> {
    let days = PyList::empty(r.py);
    if kind == "minute" {
        let header = r.at("<H22sIBfH",0,"tick_chart header",6)?;
        for index in r.range(&header[5])?.try_iter()? {
            let index = index?; r.py.check_signals()?; let pos = r.left("add",35,&r.calc("mul",&index,18)?)?;
            let values = r.row("<HffIf",&pos,"tick_chart tick[{}]",&index,5)?;
            days.append(model!(r,"MacTick","time" => minute_clock(r,&values[0])?,"price" => &values[1],"avg" => &values[2],"vol" => &values[3],"momentum" => &values[4]))?;
        }
        let pos = r.left("add",35,&r.calc("mul",&header[5],18)?)?;
        let tail = exact(r.module.getattr("unpack_from")?.call1((layout::CHART_TAIL_FORMAT,&r.body,pos,"tick_chart tail"))?,18)?;
        return chart_model(r,"MacTickChart",&header,&tail,days.into_any());
    }
    let id = r.at("<H22s",0,"tick_charts header",2)?;
    let dates = r.module.getattr("unpack_from")?.call1(("<5I",&r.body,24,"tick_charts dates"))?;
    let closes = r.module.getattr("unpack_from")?.call1(("<5f",&r.body,44,"tick_charts pre_closes"))?;
    let header = r.at("<HBHH",64,"tick_charts header2",4)?;
    let mut leading = r.n(0)?;
    for value in dates.try_iter()? {if !value?.is_truthy()? {break;} leading = r.calc("iadd",&leading,1)?;}
    let maximum = builtin(&r.module,"max")?; let minimum = builtin(&r.module,"min")?;
    let count = maximum.call1((0,minimum.call1((&header[0],leading,command.getattr("MAX_DAYS")?))?))?;
    let page = builtin(&r.module,"max")?.call1((1,&header[2]))?;
    let maximum = builtin(&r.module,"max")?;
    let remaining = r.binary("sub",&r.binary("sub",&r.size()?,&command.getattr("_TICK_OFFSET")?)?,&command.getattr("_TAIL_SIZE")?)?;
    let max_ticks = maximum.call1((0,r.binary("floordiv",&remaining,&command.getattr("_TICK_SIZE")?)?))?;
    let total = builtin(&r.module,"min")?.call1((&header[3],max_ticks))?;
    let tick_counts = if total.ge(r.binary("mul",&count,&page)?)? {
        r.binary("mul",&PyList::new(r.py,[&page])?.into_any(),&count)?
    } else {
        let complete = if count.is_truthy()? {let minimum = builtin(&r.module,"min")?; minimum.call1((r.calc("sub",&count,1)?,r.binary("floordiv",&total,&page)?))?} else {r.n(0)?};
        let first = r.binary("sub",&total,&r.binary("mul",&complete,&page)?)?;
        let empty = r.calc("sub",&r.binary("sub",&count,&complete)?,1)?;
        let left = PyList::new(r.py,[first])?.into_any(); let full = r.binary("mul",&PyList::new(r.py,[&page])?.into_any(),&complete)?;
        let left = r.binary("add",&left,&full)?; let empty = r.binary("mul",&PyList::new(r.py,[0])?.into_any(),&empty)?;
        r.binary("add",&left,&empty)?
    };
    let mut tick_index = r.n(0)?;
    for item in builtin(&r.module,"enumerate")?.call1((tick_counts,))?.try_iter()? {
        let pair = exact(item?,2)?; let d = &pair[0]; let ticks = PyList::empty(r.py);
        for t in r.range(&pair[1])?.try_iter()? {
            let t = t?; r.py.check_signals()?;
            let start = command.getattr("_TICK_OFFSET")?;
            let pos = r.binary("add",&start,&r.binary("mul",&tick_index,&command.getattr("_TICK_SIZE")?)?)?;
            let method = r.module.getattr("unpack_from")?;
            let values = exact(method.call1(("<HffHH",&r.body,pos,r.format("tick_charts tick[{}][{}]",&[d.clone(),t])?))?,5)?;
            tick_index = r.calc("iadd",&tick_index,1)?;
            ticks.append(model!(r,"MacTick","time" => minute_clock(r,&values[0])?,"price" => &values[1],"avg" => &values[2],"vol" => &values[3]))?;
        }
        let ymd = dates.get_item(d)?;
        days.append(model!(r,"MacMultiTickDay","date" => {
            let constructor = r.module.getattr("date")?;
            constructor.call1((r.calc("floordiv",&ymd,10000)?,r.calc("floordiv",&r.calc("mod",&ymd,10000)?,100)?,r.calc("mod",&ymd,100)?))?
        },"pre_close" => closes.get_item(d)?,"ticks" => ticks))?;
    }
    let start = command.getattr("_TICK_OFFSET")?;
    let pos = r.binary("add",&start,&r.binary("mul",&total,&command.getattr("_TICK_SIZE")?)?)?;
    let method = r.module.getattr("unpack_from")?;
    let tail = exact(method.call1((command.getattr("_TAIL_FMT")?,&r.body,pos,"tick_charts tail"))?,18)?;
    chart_model(r,"MacMultiTickChart",&id,&tail,days.into_any())
}
#[pyfunction]
pub fn mac_unusual_description<'py>(py: Python<'py>,kind: Bound<'py,PyAny>,data: Bound<'py,PyAny>,module: Bound<'py,PyAny>) -> PyResult<Py<PyAny>> {
    let r = Reader {py,module,body:data,operator:py.import("operator")?.into_any()};
    let pair = describe(&r,&kind)?;
    Ok(pyo3::types::PyTuple::new(py,pair)?.into_any().unbind())
}
fn describe<'py>(r: &Reader<'py>,kind: &Bound<'py,PyAny>) -> PyResult<[Bound<'py,PyAny>;2]> {
    if r.size()?.lt(13)? {return Ok([r.s("")?,r.s("")?]);}
    let values = exact(r.module.getattr("struct")?.call_method1("unpack_from",("<B2fI",&r.body))?,4)?;
    let (a,b,c,d) = (&values[0],&values[1],&values[2],&values[3]);
    let mut value = r.s("")?;
    let desc;
    if kind.eq(3)? {
        desc = r.s(if a.eq(0)? {"主力买入"} else {"主力卖出"})?;
        value = r.format("{:.2f}/{:.2f}",&[b.clone(),c.clone()])?;
    } else if kind.eq(4)? {
        desc = r.s("加速拉升")?; value = r.format("{:.2f}%",&[r.calc("mul",b,100)?])?;
    } else if kind.eq(5)? {desc = r.s("加速下跌")?;}
    else if kind.eq(6)? {desc = r.s("低位反弹")?; value = r.format("{:.2f}%",&[r.calc("mul",b,100)?])?;}
    else if kind.eq(7)? {desc = r.s("高位回落")?; value = r.format("{:.2f}%",&[r.calc("mul",b,100)?])?;}
    else if kind.eq(8)? {desc = r.s("撑杆跳高")?; value = r.format("{:.2f}%",&[r.calc("mul",b,100)?])?;}
    else if kind.eq(9)? {desc = r.s("平台跳水")?; value = r.format("{:.2f}%",&[r.calc("mul",b,100)?])?;}
    else if kind.eq(10)? {
        desc = r.s(if b.lt(0)? {"单笔冲跌"} else {"单笔冲涨"})?; value = r.format("{:.2f}%",&[r.calc("mul",b,100)?])?;
    } else if kind.eq(11)? {
        desc = r.s(if c.eq(0)? {"区间放量平"} else if c.lt(0)? {"区间放量跌"} else {"区间放量涨"})?;
        let first = r.format("{:.1f}倍",&[b.clone()])?;
        let last = if c.eq(0)? {r.s("")?} else {r.format("{:.2f}%",&[r.calc("mul",c,100)?])?};
        value = r.binary("add",&first,&last)?;
    } else if kind.eq(12)? {desc = r.s("区间缩量")?;}
    else if kind.eq(16)? {desc = r.s("大单托盘")?; value = r.format("{:.2f}/{:.2f}",&[d.clone(),c.clone()])?;}
    else if kind.eq(17)? {desc = r.s("大单压盘")?; value = r.format("{:.2f}/{:.2f}",&[b.clone(),c.clone()])?;}
    else if kind.eq(18)? {desc = r.s("大单锁盘")?;}
    else if kind.eq(19)? {desc = r.s("竞价试买")?; value = r.format("{:.2f}/{:.2f}",&[b.clone(),c.clone()])?;}
    else if kind.eq(20)? {
        let direction = if a.eq(0)? {"涨"} else {"跌"};
        let alternate = if r.size()?.ge(10)? {exact(r.module.getattr("struct")?.call_method1("unpack_from",("<Bff",&r.body,1))?,3)?}
            else {vec![r.n(0)?,0.0.into_pyobject(r.py)?.into_any(),0.0.into_pyobject(r.py)?.into_any()]};
        let pattern = if alternate[0].eq(1)? {"逼近{}停"} else if alternate[0].eq(2)? {"封{}停板"}
            else if alternate[0].eq(4)? {"封{}大减"} else if alternate[0].eq(5)? {"打开{}停"} else {"涨跌停({})"};
        desc = r.format(pattern,&[r.s(direction)?])?;
        value = r.format("{:.2f}/{:.2f}",&[alternate[1].clone(),alternate[2].clone()])?;
    } else {desc = r.format("异动类型{:#04x}",&[kind.clone()])?;}
    Ok([desc,value])
}
fn unusual<'py>(r: &Reader<'py>) -> PyResult<Py<PyAny>> {
    let count = r.at("<H",0,"unusual count",1)?.remove(0);
    let results = PyList::empty(r.py);
    for index in r.range(&count)?.try_iter()? {
        let index = index?; r.py.check_signals()?; let pos = r.left("add",2,&r.calc("mul",&index,32)?)?;
        if r.calc("add",&pos,32)?.gt(r.size()?)? {break;}
        let values = r.row("<H6sBBBHH",&pos,"unusual record[{}]",&index,7)?;
        let method = r.module.getattr("_describe_unusual")?;
        let pair = exact(method.call1((&values[3],r.slice(&r.calc("add",&pos,15)?,Some(&r.calc("add",&pos,28)?))?))?,2)?;
        let method = r.module.getattr("unpack_from")?;
        let time = exact(method.call1(("<BH",&r.body,r.calc("add",&pos,29)?,r.format("unusual time[{}]",&[index])?))?,2)?;
        results.append(model!(r,"UnusualItem","index" => &values[5],"market" => &values[0],"code" => r.text(&values[1],"gbk")?,"name" => "",
            "time" => {let method = r.module.getattr("time")?; method.call1((&time[0],r.calc("floordiv",&time[1],100)?,r.calc("mod",&time[1],100)?))?},
            "desc" => &pair[0],"value" => &pair[1],"unusual_type" => &values[3]))?;
    }
    let end = r.left("add",2,&r.calc("mul",&count,32)?)?;
    let bytes = r.slice(&end,None)?;
    let kwargs = PyDict::new(r.py); kwargs.set_item("errors","ignore")?;
    let names = bytes.call_method("decode",("gbk",),Some(&kwargs))?.call_method1("strip",(",",))?.call_method1("split",(",",))?;
    let populated = PyList::empty(r.py);
    for pair in builtin(&r.module,"enumerate")?.call1((results,))?.try_iter()? {
        let pair = exact(pair?,2)?; let index = &pair[0]; let item = &pair[1];
        let name = if index.lt(r.len(&names)?)? {names.get_item(index)?} else {r.s("")?};
        populated.append(model!(r,"UnusualItem","index" => item.getattr("index")?,"market" => item.getattr("market")?,"code" => item.getattr("code")?,
            "name" => name,"time" => item.getattr("time")?,"desc" => item.getattr("desc")?,"value" => item.getattr("value")?,"unusual_type" => item.getattr("unusual_type")?))?;
    }
    Ok(populated.into_any().unbind())
}
fn trades<'py>(r: &Reader<'py>,kind: &str) -> PyResult<Py<PyAny>> {
    let auction = kind == "auction";
    let count = if auction {r.at("<H22sI",0,"auction header",3)?[2].clone()} else {r.at("<H",29,"transaction count",1)?[0].clone()};
    let results = PyList::empty(r.py);
    for index in r.range(&count)?.try_iter()? {
        let index = index?; r.py.check_signals()?;
        let pos = r.left("add",if auction {36} else {39},&r.calc("mul",&index,if auction {layout::AUCTION_RECORD_SIZE as i32} else {layout::TRANSACTION_RECORD_SIZE as i32})?)?;
        if auction && r.calc("add",&pos,16)?.gt(r.size()?)? {break;}
        let values = r.row(if auction {layout::AUCTION_RECORD_FORMAT} else {layout::TRANSACTION_RECORD_FORMAT},&pos,
            if auction {"auction item[{}]"} else {"transaction item[{}]"},&index,if auction {4} else {5})?;
        let item = if auction {
            model!(r,"AuctionItem","time" => r.clock(&values[0])?,"price" => &values[1],"matched" => &values[2],"unmatched" => &values[3])
        } else {
            model!(r,"MacTransaction","time" => r.clock(&values[0])?,"price" => &values[1],"vol" => &values[2],"trade_count" => &values[3],"bs_flag" => &values[4])
        };
        results.append(item)?;
    }
    Ok(results.into_any().unbind())
}
fn info<'py>(r: &Reader<'py>) -> PyResult<Py<PyAny>> {
    let id = r.at("<H22s44s",8,"symbol_info identity",3)?;
    let values = r.at(layout::INFO_CORE_FORMAT,96,"symbol_info core",13)?;
    let extra = r.at(layout::INFO_EXTRA_FORMAT,148,"symbol_info extra",7)?;
    let constructor = r.module.getattr("datetime")?;
    let dt = constructor.call1((r.calc("floordiv",&values[0],10000)?,r.calc("floordiv",&r.calc("mod",&values[0],10000)?,100)?,r.calc("mod",&values[0],100)?,
        r.calc("floordiv",&values[1],10000)?,r.calc("floordiv",&r.calc("mod",&values[1],10000)?,100)?,r.calc("mod",&values[1],100)?))?;
    Ok(model!(r,"MacSymbolInfo","market" => &id[0],"code" => r.decoded(&id[1])?,"name" => r.decoded(&id[2])?,"time" => dt,"activity" => &values[2],
        "pre_close" => &values[3],"open" => &values[4],"high" => &values[5],"low" => &values[6],"close" => &values[7],"momentum" => &values[8],
        "vol" => builtin(&r.module,"int")?.call1((&values[9],))?,"amount" => &values[10],"inside_volume" => &values[11],"outside_volume" => &values[12],
        "turnover" => &extra[5],"avg" => &extra[6]).unbind())
}
fn boards<'py>(r: &Reader<'py>) -> PyResult<Py<PyAny>> {
    let header = r.at("<HH",0,"board_list header",2)?;
    let count = r.calc("floordiv",&header[0],2)?; let results = PyList::empty(r.py);
    for index in r.range(&count)?.try_iter()? {
        let index = index?; r.py.check_signals()?;
        let pos = r.left("add",4,&r.binary("mul",&index,&r.module.getattr("_BOARD_RECORD_SIZE")?)?)?;
        let method = r.module.getattr("unpack_from")?;
        let fmt = r.module.getattr("_BOARD_RECORD_FMT")?;
        let values = exact(method.call1((fmt,&r.body,pos,r.format("board_list record[{}]",&[index])?))?,14)?;
        results.append(model!(r,"BoardInfo","market" => &values[0],"code" => r.text(&values[1],"gbk")?,"name" => r.text(&values[3],"gbk")?,
            "price" => &values[4],"rise_speed" => &values[5],"pre_close" => &values[6],"symbol_market" => &values[7],
            "symbol_code" => r.text(&values[8],"gbk")?,"symbol_name" => r.text(&values[10],"gbk")?,"symbol_price" => &values[11],
            "symbol_rise_speed" => &values[12],"symbol_pre_close" => &values[13]))?;
    }
    Ok(results.into_any().unbind())
}
fn json<'py>(r: &Reader<'py>,kind: &str) -> PyResult<Py<PyAny>> {
    let belong = kind == "belong";
    if r.size()?.lt(27)? {return Ok(if belong {PyList::empty(r.py).into_any().unbind()} else {r.py.None()});}
    let loads = r.module.getattr("json")?.getattr("loads")?;
    let raw = r.slice(&r.n(27)?,None)?;
    let text = if belong {let kwargs = PyDict::new(r.py); kwargs.set_item("errors","replace")?; raw.call_method("decode",("gbk",),Some(&kwargs))?}
        else {raw.call_method1("decode",("gbk",))?};
    let rows = loads.call1((text,))?;
    if !belong {
        if r.len(&rows)?.lt(2)? {return Ok(r.py.None());}
        let today = rows.get_item(0)?; let five = rows.get_item(1)?;
        let mut values = Vec::with_capacity(6);
        for (row,index) in [(&today,0),(&today,1),(&today,2),(&today,3),(&five,4),(&five,3)] {
            values.push(if r.len(row)?.gt(index)? {r.module.getattr("_to_float")?.call1((row.get_item(index)?,))?}
                else {0.0.into_pyobject(r.py)?.into_any()});
        }
        return Ok(model!(r,"CapitalFlowData","date" => "","main_in" => &values[0],"main_out" => &values[1],
            "main_net" => r.binary("sub",&values[0],&values[1])?,"small_in" => &values[2],"small_out" => &values[3],
            "small_net" => r.binary("sub",&values[2],&values[3])?,"mid_in" => 0.0,"mid_out" => 0.0,"mid_net" => &values[4],
            "large_in" => 0.0,"large_out" => 0.0,"large_net" => &values[5]).unbind());
    }
    let results = PyList::empty(r.py);
    for row in rows.try_iter()? {
        let row = row?; r.py.check_signals()?; let len = r.len(&row)?;
        if !(len.eq(9)? || len.eq(13)?) {continue;}
        let constructor = r.module.getattr("BelongBoardInfo")?; let kwargs = PyDict::new(r.py);
        kwargs.set_item("board_type",r.module.getattr("_to_int")?.call1((row.get_item(0)?,))?)?;
        kwargs.set_item("market",r.module.getattr("_to_int")?.call1((row.get_item(1)?,))?)?;
        kwargs.set_item("board_code",builtin(&r.module,"str")?.call1((row.get_item(2)?,))?)?;
        kwargs.set_item("board_name",builtin(&r.module,"str")?.call1((row.get_item(3)?,))?)?;
        for (key,index) in [("close",4),("pre_close",5)] {
            kwargs.set_item(key,if len.gt(index)? && row.get_item(index)?.is_truthy()? {
                r.module.getattr("_to_float")?.call1((row.get_item(index)?,))?
            } else {0.0.into_pyobject(r.py)?.into_any()})?;
        }
        results.append(constructor.call((),Some(&kwargs))?)?;
    }
    Ok(results.into_any().unbind())
}
fn offsets<'py>(r: &Reader<'py>) -> PyResult<Py<PyAny>> {
    if r.size()?.lt(8)? {return Ok(model!(r,"KlineOffsetInfo","total" => 0,"returned" => 0).unbind());}
    let method = r.module.getattr("struct")?.getattr("unpack")?;
    let total = method.call1((">I",r.body.get_item(r.py.get_type::<PySlice>().call1((r.py.None(),4))?)?))?.get_item(0)?;
    let method = r.module.getattr("struct")?.getattr("unpack")?;
    let returned = method.call1(("<I",r.slice(&r.n(4)?,Some(&r.n(8)?))?))?.get_item(0)?;
    Ok(model!(r,"KlineOffsetInfo","total" => total,"returned" => returned).unbind())
}
fn inventory<'py>(r: &Reader<'py>,command: &Bound<'py,PyAny>,kind: &str) -> PyResult<Py<PyAny>> {
    let sampling = kind == "sampling";
    let results = PyList::empty(r.py);
    let count = if sampling {
        if r.size()?.lt(command.getattr("_RESPONSE_HEADER_SIZE")?)? {return Ok(results.into_any().unbind());}
        let require = r.module.getattr("require_bytes")?;
        require.call1((&r.body,0,command.getattr("_RESPONSE_HEADER_SIZE")?,"ChartSamplingCmd header"))?;
        r.at("<H",40,"chart_sampling count",1)?[0].clone()
    } else {
        r.require(&r.n(0)?,&r.n(2)?,r.s("GoodsListCmd header")?)?;
        let count = r.at("<H",0,"GoodsListCmd total",1)?[0].clone(); command.setattr("total",&count)?; count
    };
    for index in r.range(&count)?.try_iter()? {
        let index = index?; r.py.check_signals()?;
        if sampling {
            let header = command.getattr("_RESPONSE_HEADER_SIZE")?;
            let pos = r.binary("add",&header,&r.calc("mul",&index,4)?)?;
            let method = r.module.getattr("require_bytes")?;
            method.call1((&r.body,&pos,4,r.format("ChartSamplingCmd price[{}]",&[index.clone()])?))?;
            let values = r.row("<f",&pos,"chart_sampling price[{}]",&index,1)?;
            results.append(&values[0])?;
        } else {
            let pos = r.left("add",2,&r.binary("mul",&index,&command.getattr("_RECORD_SIZE")?)?)?;
            let method = r.module.getattr("require_bytes")?;
            method.call1((&r.body,&pos,command.getattr("_RECORD_SIZE")?,r.format("GoodsListCmd record[{}]",&[index.clone()])?))?;
            let method = r.module.getattr("unpack_from")?;
            let fmt = command.getattr("_RECORD_FMT")?;
            let values = exact(method.call1((fmt,&r.body,pos,r.format("GoodsListCmd record[{}]",&[index])?))?,10)?;
            results.append(model!(r,"GoodsItem","name" => r.text(&values[1],"gbk")?,"category" => &values[0],"u" => &values[2],
                "index" => &values[3],"switch" => &values[4],"code" => PyList::new(r.py,[&values[5],&values[6],&values[7]])?,
                "c1" => &values[8],"c2" => &values[9]))?;
        }
    }
    Ok(results.into_any().unbind())
}
fn file_meta<'py>(r: &Reader<'py>) -> PyResult<Py<PyAny>> {
    r.require(&r.n(0)?,&r.n(41)?,r.s("FileListCmd")?)?;
    let values = r.at("<IIb",0,"FileListCmd meta",3)?;
    let hash = r.text(&r.slice(&r.n(9)?,Some(&r.n(41)?))?,"ascii")?;
    Ok(model!(r,"FileMeta","offset" => &values[0],"size" => &values[1],"flag" => &values[2],"hash" => hash).unbind())
}
fn server_date<'py>(r: &Reader<'py>,pos: i32) -> PyResult<Bound<'py,PyAny>> {
    let value = r.module.getattr("unpack_from")?.call1(("<I",&r.body,pos,"server_info date"))?.get_item(0)?;
    let year = r.format("{}",&[r.calc("floordiv",&value,10000)?])?;
    let month = r.format("{:02d}",&[r.calc("floordiv",&r.calc("mod",&value,10000)?,100)?])?;
    let day = r.format("{:02d}",&[r.calc("mod",&value,100)?])?;
    r.format("{}-{}-{}",&[year,month,day])
}
fn server_sessions<'py>(r: &Reader<'py>,pos: i32) -> PyResult<Bound<'py,PyAny>> {
    let values = r.module.getattr("unpack_from")?.call1(("<8H",&r.body,pos,"server_info session"))?;
    let sessions = PyList::empty(r.py);
    for index in builtin(&r.module,"range")?.call1((0,8,2))?.try_iter()? {
        let index = index?; r.py.check_signals()?; let row = PyDict::new(r.py);
        // The original expressions fetch vals[i] once for each arithmetic part.
        let hour = r.format("{}",&[r.calc("floordiv",&values.get_item(&index)?,60)?])?;
        let minute = r.format("{:02d}",&[r.calc("mod",&values.get_item(&index)?,60)?])?;
        row.set_item("open",r.format("{}:{}",&[hour,minute])?)?;
        let hour = r.format("{}",&[r.calc("floordiv",&values.get_item(r.calc("add",&index,1)?)?,60)?])?;
        let minute = r.format("{:02d}",&[r.calc("mod",&values.get_item(r.calc("add",&index,1)?)?,60)?])?;
        row.set_item("close",r.format("{}:{}",&[hour,minute])?)?;
        sessions.append(row)?;
    }
    Ok(sessions.into_any())
}
fn server<'py>(r: &Reader<'py>) -> PyResult<Py<PyAny>> {
    if r.size()?.lt(87)? {return Ok(model!(r,"ServerSession","today" => "","last_trading_day" => "").unbind());}
    r.module.getattr("unpack_from")?.call1(("<H",&r.body,0,"server_info count"))?.get_item(0)?;
    let today = server_date(r,22)?;
    let first = server_sessions(r,30)?; let second = server_sessions(r,46)?;
    let last = server_date(r,63)?;
    let (a,b) = if r.n(79)?.le(r.size()?)? {
        let first = r.module.getattr("unpack_from")?.call1(("<I",&r.body,71,"server_info param1"))?.get_item(0)?;
        let second = r.module.getattr("unpack_from")?.call1(("<I",&r.body,75,"server_info param2"))?.get_item(0)?;
        (first,second)
    } else {(r.n(0)?,r.n(0)?)};
    Ok(model!(r,"ServerSession","today" => today,"last_trading_day" => last,"sessions_1" => first,"sessions_2" => second,
        "market_param_1" => a,"market_param_2" => b).unbind())
}
