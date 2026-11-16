//! Python object adaptation for the SDK's corporate traversal policies.
use eltdx::__python_compat::{legacy_capital_collect, legacy_finance_recover,
    LegacyCapitalCollection, LegacyCapitalPage, LegacyCapitalSource,
    LegacyFinanceError, LegacyFinancePage, LegacyFinanceSource};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};

struct CapitalSource<'py> {
    codes: Bound<'py, PyList>, query: Bound<'py, PyAny>,
    block_type: Bound<'py, PyAny>, batch_type: Bound<'py, PyAny>,
    blocks: Vec<Py<PyAny>>, payloads: Vec<Py<PyAny>>,
}

struct CapitalPage {
    original: Py<PyAny>,
    blocks: Option<Py<PyAny>>,
    payloads: Option<Py<PyAny>>,
}

impl LegacyCapitalSource for CapitalSource<'_> {
    type Response = CapitalPage;
    type Error = PyErr;
    fn fetch(&mut self, start: usize) -> PyResult<LegacyCapitalPage<Self::Response>> {
        let response = self.query.call1((self.codes.get_slice(start, self.codes.len()),))?;
        if response.is_instance(&self.block_type)? {
            let py = self.codes.py();
            let raw = response.getattr("raw_payload")?;
            let payloads = if raw.is_truthy()? { PyTuple::new(py, [response.getattr("raw_payload")?])? } else { PyTuple::empty(py) };
            let page = CapitalPage { blocks: Some(PyTuple::new(py, [&response])?.unbind().into_any()),
                payloads: Some(payloads.unbind().into_any()), original: response.unbind() };
            Ok(LegacyCapitalPage::Blocks { response: page, count: 1 })
        } else if response.is_instance(&self.batch_type)? {
            let blocks = response.getattr("blocks")?;
            let payloads = response.getattr("raw_payloads")?;
            let count = blocks.len()?;
            let page = CapitalPage { blocks: Some(blocks.unbind()), payloads: Some(payloads.unbind()),
                original: response.unbind() };
            Ok(LegacyCapitalPage::Blocks { response: page, count })
        } else { Ok(LegacyCapitalPage::Custom(CapitalPage { original: response.unbind(), blocks: None,
            payloads: None })) }
    }
    fn matches(&mut self, response: &Self::Response, start: usize, count: usize) -> PyResult<bool> {
        let py = self.codes.py();
        let actual = PyList::empty(py);
        let blocks = response.blocks.as_ref().expect("recognized capital response").bind(py);
        for block in blocks.try_iter()? { actual.append(block?.getattr("full_code")?)?; }
        actual.eq(self.codes.get_slice(start, start.saturating_add(count)))
    }
    fn protocol_error(&self, message: &str) -> PyErr { PyRuntimeError::new_err(message.to_owned()) }
    fn accept(&mut self, response: &Self::Response) -> PyResult<()> {
        let py = self.codes.py();
        for block in response.blocks.as_ref().expect("recognized capital response").bind(py).try_iter()? {
            self.blocks.push(block?.unbind());
        }
        for raw in response.payloads.as_ref().expect("recognized capital response").bind(py).try_iter()? {
            self.payloads.push(raw?.unbind());
        }
        Ok(())
    }
}

#[pyfunction]
pub fn collect_capital<'py>(py: Python<'py>, codes: Bound<'py, PyList>, query: Bound<'py, PyAny>,
    block_type: Bound<'py, PyAny>, batch_type: Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    let total = codes.len();
    let mut source = CapitalSource { codes, query, block_type, batch_type, blocks: Vec::new(), payloads: Vec::new() };
    match legacy_capital_collect(total, &mut source)? {
        LegacyCapitalCollection::Custom(response) => return Ok(response.original),
        LegacyCapitalCollection::Pages(_) => {},
    };
    Ok(source.batch_type.call1((PyTuple::new(py, source.blocks)?, PyTuple::new(py, source.payloads)?))?.unbind())
}

struct FinanceSource<'py> {
    codes: Bound<'py, PyList>, query: Bound<'py, PyAny>,
    batch_type: Bound<'py, PyAny>, normalize: Bound<'py, PyAny>,
    protocol_error: Bound<'py, PyAny>, missing: Vec<Py<PyAny>>,
}

impl LegacyFinanceSource for FinanceSource<'_> {
    type Response = Py<PyAny>;
    type Error = PyErr;
    fn fetch(&mut self, indices: &[usize]) -> PyResult<LegacyFinancePage<Self::Response>> {
        let py = self.codes.py();
        py.check_signals()?;
        let part = PyList::new(py, indices.iter().map(|&index| self.codes.get_item(index)).collect::<PyResult<Vec<_>>>()?)?;
        let response = self.query.call1((part,))?;
        let rows = if response.is_instance(&self.batch_type)? { response.getattr("records")? } else { response.clone() };
        if rows.is_instance_of::<PyList>() || rows.is_instance_of::<PyTuple>() {
            Ok(LegacyFinancePage::Rows { count: rows.len()?, response: response.unbind() })
        } else { Ok(LegacyFinancePage::Custom(response.unbind())) }
    }
    fn classify(&self, error: &PyErr) -> PyResult<LegacyFinanceError> {
        let py = self.codes.py();
        if !error.is_instance(py, &self.protocol_error) {
            return Ok(LegacyFinanceError { retryable: false, absent: false });
        }
        Ok(LegacyFinanceError::from_message(error.value(py).str()?.to_str()?))
    }
    fn missing(&mut self, index: usize) -> PyResult<()> {
        self.missing.push(self.normalize.call1((self.codes.get_item(index)?,))?.unbind());
        Ok(())
    }
    fn protocol_error(&self, message: &str) -> PyErr {
        match self.protocol_error.call1((message,)) {
            Ok(value) => PyErr::from_value(value),
            Err(error) => error,
        }
    }
}

#[pyfunction]
pub fn recover_finance<'py>(py: Python<'py>, codes: Bound<'py, PyList>, query: Bound<'py, PyAny>,
    batch_type: Bound<'py, PyAny>, normalize: Bound<'py, PyAny>, protocol_error: Bound<'py, PyAny>)
    -> PyResult<(Vec<Py<PyAny>>, Vec<Py<PyAny>>, bool)> {
    let indices = (0..codes.len()).collect::<Vec<_>>();
    let mut source = FinanceSource { codes, query, batch_type, normalize, protocol_error, missing: Vec::new() };
    py.check_signals()?;
    let result = legacy_finance_recover(&indices, &mut source)?;
    Ok((result.responses, source.missing, result.recovered))
}

#[pyfunction]
pub fn assemble_finance<'py>(py: Python<'py>, codes: Bound<'py, PyList>, responses: Vec<Py<PyAny>>,
    normalize: Bound<'py, PyAny>, batch_type: Bound<'py, PyAny>, deque_type: Bound<'py, PyAny>)
    -> PyResult<Py<PyAny>> {
    let by_code = PyDict::new(py);
    // Preserve original Python hashing/equality and property exception behavior.
    let mut rows = Vec::new();
    for response in responses {
        for row in response.bind(py).getattr("records")?.try_iter()? {
            rows.push(row?.unbind());
        }
    }
    for row in rows {
            let row = row.bind(py);
            let key = row.getattr("full_code")?;
            let bucket = match by_code.get_item(&key)? {
                Some(bucket) => bucket,
                None => { let bucket = deque_type.call0()?; by_code.set_item(&key, &bucket)?; bucket }
            };
            bucket.call_method1("append", (row,))?;
    }
    let ordered = PyList::empty(py);
    for code in codes.iter() {
        py.check_signals()?;
        let key = normalize.call1((code,))?;
        if let Some(bucket) = by_code.get_item(key)? {
            if bucket.is_truthy()? { ordered.append(bucket.call_method0("popleft")?)?; }
        }
    }
    Ok(batch_type.call1((PyTuple::new(py, ordered.iter())?, pyo3::types::PyBytes::new(py, b"")))?.unbind())
}

#[pyfunction]
pub fn project_finance_fields<'py>(py: Python<'py>, records: Bound<'py, PyAny>,
    fields: Bound<'py, PyAny>, aliases: Bound<'py, PyDict>) -> PyResult<Py<PyAny>> {
    let builtins = py.import("builtins")?;
    let hasattr = builtins.getattr("hasattr")?;
    let getattr = builtins.getattr("getattr")?;
    let result = PyList::empty(py);
    for record in records.try_iter()? {
        let record = record?;
        let selected = PyDict::new(py);
        selected.set_item("full_code", record.getattr("full_code")?)?;
        for field in fields.try_iter()? {
            let field = field?;
            let attr = aliases.get_item(&field)?.unwrap_or_else(|| field.clone());
            if !hasattr.call1((&record, &attr))?.is_truthy()? {
                return Err(pyo3::exceptions::PyValueError::new_err(format!("unknown finance field: {}", field.str()?.to_str()?)));
            }
            // hasattr/getattr intentionally remain separate property accesses.
            selected.set_item(field.str()?, getattr.call1((&record, attr))?)?;
        }
        result.append(selected)?;
    }
    Ok(result.unbind().into_any())
}
