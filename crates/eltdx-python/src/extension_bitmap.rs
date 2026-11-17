//! Legacy Python objects over the SDK's single bit traversal and board plan.
//! No strict SDK input/count/deadline policy is applied to this facade.
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyInt, PyList, PySlice, PyTuple};
use eltdx::mac::{bitmap, enums::{board_code_plan, BoardCodePlan}};
use crate::f10_request::builtin;

#[pyfunction]
pub fn mac_board_code(py: Python<'_>, value: Bound<'_, PyAny>, module: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let text = value.call_method0("strip")?;
    let plan = board_code_plan(|prefix| text.call_method1("startswith",(prefix,))?.is_truthy(),
        || builtin(&module,"len")?.call1((&text,))?.eq(6))?;
    let operator = py.import("operator")?;
    let result = match plan {
        BoardCodePlan::Prefix { skip,add } => {
            // s[2:] has an actual None stop/step, observable by a custom
            // strip result implementing __getitem__.
            let integer = builtin(&module,"int")?;
            let tail = text.get_item(py.get_type::<PySlice>().call1((skip,py.None()))?)?;
            operator.getattr("add")?.call1((add,integer.call1((tail,))?))?
        }
        BoardCodePlan::Full { subtract,add } => {
            let value = builtin(&module,"int")?.call1((text,))?;
            if subtract == 0 { operator.getattr("add")?.call1((add,value))? }
            else { operator.getattr("add")?.call1((operator.getattr("sub")?.call1((value,subtract))?,add))? }
        }
        BoardCodePlan::Plain => builtin(&module,"int")?.call1((text,))?,
    };
    Ok(result.unbind())
}

#[pyfunction]
pub fn mac_field_selection(py: Python<'_>, parts: Bound<'_, PyAny>, module: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let seen = builtin(&module,"set")?.call0()?; let result = PyList::empty(py);
    for part in parts.try_iter()? {
        let part = part?;
        let source = if builtin(&module,"isinstance")?.call1((&part,module.getattr("PresetField")?))?.is_truthy()? { part.getattr("value")? }
        else if builtin(&module,"isinstance")?.call1((&part,module.getattr("FieldBit")?))?.is_truthy()? { PyTuple::new(py,[part])?.into_any() }
        else { part.getattr("_fields")? };
        for bit in source.try_iter()? {
            let bit = bit?;
            if !seen.contains(&bit)? { seen.call_method1("add",(&bit,))?; result.append(bit)?; }
        }
    }
    Ok(builtin(&module,"tuple")?.call1((result,))?.unbind())
}

#[pyfunction]
pub fn mac_normalize_fields(py: Python<'_>, fields: Bound<'_, PyAny>, module: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    if fields.is_none() { return Ok(module.getattr("FieldSelection")?.call0()?.unbind()); }
    if builtin(&module,"isinstance")?.call1((&fields,module.getattr("FieldSelection")?))?.is_truthy()? { return Ok(fields.unbind()); }
    if builtin(&module,"isinstance")?.call1((&fields,module.getattr("PresetField")?))?.is_truthy()? {
        let constructor = module.getattr("FieldSelection")?;
        let args = py.get_type::<PyTuple>().call1((fields.getattr("value")?,))?.cast_into::<PyTuple>()?;
        return Ok(constructor.call(args,None)?.unbind());
    }
    if builtin(&module,"isinstance")?.call1((&fields,module.getattr("FieldBit")?))?.is_truthy()? {
        return Ok(module.getattr("FieldSelection")?.call1((fields,))?.unbind());
    }
    // Star expansion uses Python's iterable/length-hint protocol and eagerly
    // materializes arguments before the constructor, as in the old facade.
    let constructor = module.getattr("FieldSelection")?;
    let args = py.get_type::<PyTuple>().call1((fields,))?.cast_into::<PyTuple>()?;
    Ok(constructor.call(args,None)?.unbind())
}

fn formatted(value: &Bound<'_, PyAny>, spec: &str) -> PyResult<String> {
    let template = format!("{{:{spec}}}");
    template.into_pyobject(value.py())?.call_method1("format",(value,))?.extract()
}
fn wire_integer<'py>(py: Python<'py>, bytes: &[u8]) -> PyResult<Bound<'py, PyAny>> {
    py.get_type::<PyInt>().call_method1("from_bytes",(PyBytes::new(py,bytes),"little"))
}

#[pyfunction]
pub fn mac_build_bitmap(py: Python<'_>, fields: Bound<'_, PyAny>, width: Bound<'_, PyAny>,
    module: Bound<'_, PyAny>) -> PyResult<Py<PyAny>>
{
    if !(1.into_pyobject(py)?.le(&width)? && width.le(module.getattr("BITMAP_SIZE")?)?) {
        let error = builtin(&module,"ValueError")?;
        let maximum = formatted(&module.getattr("BITMAP_SIZE")?,"")?;
        let width = formatted(&width,"")?;
        return Err(PyErr::from_value(error.call1((format!("width must be between 1 and {maximum}, got {width}"),))?));
    }
    if builtin(&module,"isinstance")?.call1((&fields,module.getattr("PresetField")?))?.is_truthy()?
        && fields.is(&module.getattr("PresetField")?.getattr("DEBUG")?) {
        let constructor = builtin(&module,"bytearray")?;
        let bytes = py.import("operator")?.getattr("mul")?.call1((PyBytes::new(py,&[255]),width))?;
        return Ok(constructor.call1((bytes,))?.unbind());
    }
    let selection = module.getattr("normalize_fields")?.call1((fields,))?;
    let op = py.import("operator")?;
    let limit = op.getattr("mul")?.call1((&width,8))?;
    let mut buffer = [0;20];
    let mut custom: Option<Bound<'_,PyAny>> = None;
    for bit in selection.try_iter()? {
        py.check_signals()?;
        let bit = bit?;
        if bit.getattr("value")?.ge(&limit)? {
            let error = builtin(&module,"ValueError")?;
            // Each f-string field is evaluated/formatted before the next one.
            let name = formatted(&bit.getattr("name")?,"")?;
            let value = formatted(&bit.getattr("value")?,"02X")?;
            let width = formatted(&width,"")?;
            let maximum = formatted(&limit,"")?;
            let last = formatted(&op.getattr("sub")?.call1((&limit,1))?,"")?;
            return Err(PyErr::from_value(error.call1((format!(
                "字段 {name}(0x{value}) 超出 {width} 字节位图（最多 {maximum} 位，即 0..{last}）"),))?));
        }
        // The source reads .value twice. Keep that read even when the normal
        // exact integer can use the strict SDK's common wire insertion.
        let value = bit.getattr("value")?;
        if custom.is_none() && value.is_exact_instance_of::<PyInt>() {
            if let Ok(position) = value.extract::<usize>() {
                if bitmap::write_bit(&mut buffer,position) { continue; }
            }
        }
        let prior = match custom.take() { Some(value) => value,None => wire_integer(py,&buffer)? };
        let mask = op.getattr("lshift")?.call1((1,value))?;
        custom = Some(op.getattr("ior")?.call1((prior,mask))?);
    }
    let integer = match custom { Some(value) => value,None => wire_integer(py,&buffer)? };
    let constructor = builtin(&module,"bytearray")?;
    let bytes = integer.call_method1("to_bytes",(width,"little"))?;
    Ok(constructor.call1((bytes,))?.unbind())
}

#[pyfunction]
pub fn mac_exclude_flags(value: Bound<'_, PyAny>, module: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    Ok(value.call_method1("to_bytes",(module.getattr("CONTROL_SIZE")?,"little"))?.unbind())
}

#[pyfunction]
pub fn mac_post_ah_code(py: Python<'_>, value: Bound<'_, PyAny>, market: Bound<'_, PyAny>,
    module: Bound<'_, PyAny>) -> PyResult<Py<PyAny>>
{
    if !value.is_truthy()? { return Ok("".into_pyobject(py)?.into_any().unbind()); }
    let width = eltdx::mac::routing::ah_code_width(PyTuple::new(py,[0,1])?.contains(market)?);
    Ok(builtin(&module,"str")?.call1((value,))?.call_method1("zfill",(width,))?.unbind())
}

#[pyfunction]
pub fn mac_field_bits(py: Python<'_>, bytes: Bound<'_, PyAny>, module: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let op = py.import("operator")?;
    let mask = op.getattr("sub")?.call1((op.getattr("lshift")?.call1((1,module.getattr("FIELD_BIT_LIMIT")?))?,1))?;
    let integer = builtin(&module,"int")?.call_method1("from_bytes",(bytes,"little"))?;
    Ok(op.getattr("and_")?.call1((integer,mask))?.unbind())
}

#[pyfunction]
pub fn mac_count_bits(bytes: Bound<'_, PyAny>, module: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    Ok(module.getattr("_field_bits")?.call1((bytes,))?.call_method0("bit_count")?.unbind())
}

/// A normal iterator exhausted/retired like the original generator. It does
/// not inspect the bitmap until first next(), and resolves the live registry
/// after each yield. Custom non-int/negative _field_bits results keep Python
/// numeric protocols; the standard nonnegative integer uses the shared scan.
#[pyclass]
pub struct MacBitSlots {
    module: Py<PyAny>, input: Option<Py<PyAny>>, bytes: Option<Vec<u8>>,
    remaining: Option<Py<PyAny>>, cursor: usize, slot: usize, done: bool,
}
impl MacBitSlots {
    fn advance(&mut self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        let module = self.module.bind(py);
        if let Some(input) = self.input.take() {
            let integer = module.getattr("_field_bits")?.call1((input,))?;
            if integer.is_exact_instance_of::<PyInt>() && integer.ge(0)? {
                let bits: usize = integer.call_method0("bit_length")?.extract()?;
                let count = bits/8+usize::from(bits%8 != 0);
                let raw = integer.call_method1("to_bytes",(count,"little"))?.cast_into::<PyBytes>()?;
                self.bytes = Some(raw.as_bytes().to_vec());
            } else { self.remaining = Some(integer.unbind()); }
        }
        let position = if let Some(bytes) = &self.bytes {
            match bitmap::next_set_bit(bytes,&mut self.cursor,usize::MAX) {
                Some(position) => position.into_pyobject(py)?.into_any(),
                None => { self.done = true; return Ok(None); }
            }
        } else {
            let remaining = self.remaining.as_ref().expect("installed legacy bit state").bind(py);
            if !remaining.is_truthy()? { self.done = true; self.remaining = None; return Ok(None); }
            let op = py.import("operator")?;
            let low = op.getattr("and_")?.call1((remaining,op.getattr("neg")?.call1((remaining,))?))?;
            let position = op.getattr("sub")?.call1((low.call_method0("bit_length")?,1))?;
            self.remaining = Some(op.getattr("ixor")?.call1((remaining,&low))?.unbind()); position
        };
        let class = module.getattr("FieldBit")?;
        let field = class.getattr("_value2member_map_")?.call_method1("get",(position,))?;
        let (field,format) = if builtin(module,"isinstance")?.call1((&field,module.getattr("FieldBit")?))?.is_truthy()? {
            let format = field.getattr("fmt")?; (field,format)
        } else { (py.None().into_bound(py),py.None().into_bound(py)) };
        let slot = self.slot; self.slot += 1;
        Ok(Some(PyTuple::new(py,[slot.into_pyobject(py)?.into_any(),field,format])?.into_any().unbind()))
    }
}
#[pymethods]
impl MacBitSlots {
    fn __iter__(slf: PyRef<'_,Self>) -> PyRef<'_,Self> { slf }
    fn __next__(&mut self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        if self.done { return Ok(None); }
        let result = py.check_signals().and_then(|()| self.advance(py));
        if result.is_err() { self.done = true; self.input = None; self.bytes = None; self.remaining = None; }
        result
    }
}
#[pyfunction]
pub fn mac_bit_slots(bytes: Bound<'_, PyAny>, module: Bound<'_, PyAny>) -> MacBitSlots {
    MacBitSlots { module: module.unbind(),input: Some(bytes.unbind()),bytes: None,remaining: None,cursor: 0,slot: 0,done: false }
}

#[pyfunction]
pub fn mac_active_fields(py: Python<'_>, bytes: Bound<'_, PyAny>, module: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let result = PyList::empty(py);
    for row in module.getattr("iter_set_bits")?.call1((bytes,))?.try_iter()? {
        let values = crate::offline_financial::unpack(row?,3)?;
        if !values[1].is_none() { result.append(PyTuple::new(py,[values[1].clone(),values[2].clone()])?)?; }
    }
    Ok(result.into_any().unbind())
}
