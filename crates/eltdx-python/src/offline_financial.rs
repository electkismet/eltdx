//! Python scalar/object projection around the shared financial row parser.
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyList};
use eltdx::offline::financial::{FinancialHeader, FinancialRecord, python_legacy::DatCallbacks};

#[pyfunction]
pub fn financial_file_list(py: Python<'_>, data: Vec<u8>, integer: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let result = PyList::empty(py);
    let text = String::from_utf8_lossy(&data);
    for (filename, hash, size) in eltdx::offline::financial::python_legacy::file_list_rows(&text) {
        py.check_signals()?;
        let size = match integer.call1((size,)) {
            Ok(size) => size,
            Err(error) if error.is_instance_of::<PyValueError>(py) => continue,
            Err(error) => return Err(error),
        };
        result.append((filename, hash, size))?;
    }
    Ok(result.into_any().unbind())
}

struct DatProjection<'py> {
    py: Python<'py>,
    module: Bound<'py, PyAny>,
    report_date: Bound<'py, PyAny>,
    result: Bound<'py, PyList>,
    error: Option<PyErr>,
}

impl DatProjection<'_> {
    fn remember(&mut self, result: PyResult<()>) -> eltdx::Result<()> {
        result.map_err(|error| {
            self.error = Some(error);
            eltdx::Error::new(eltdx::ErrorKind::Cancelled, "Python financial projection stopped")
        })
    }

    fn format_error(&self, message: String) -> PyErr {
        match self.module.getattr("ResourceFormatError").and_then(|kind| kind.call1((message,))) {
            Ok(value) => PyErr::from_value(value),
            Err(error) => error,
        }
    }
}

impl DatCallbacks for DatProjection<'_> {
    fn checkpoint(&mut self) -> eltdx::Result<()> {
        let result = self.py.check_signals();
        self.remember(result)
    }

    fn header(&mut self, header: &FinancialHeader) -> eltdx::Result<()> {
        let result = (|| {
            if self.report_date.eq(0)? {
                self.report_date = header.report_date_raw().into_pyobject(self.py)?.into_any();
            }
            Ok(())
        })();
        self.remember(result)
    }

    fn field_count(&mut self, header: &FinancialHeader, fields: u32) -> eltdx::Result<()> {
        let result = (|| {
            let size = header.report_size();
            if size % 4 != 0 {
                return Err(self.format_error(format!("财报记录长度非法: report_size={size}（必须是 4 的整数倍）")));
            }
            let limit = self.module.getattr("MAX_HISTORY_REPORT_FIELDS")?;
            if fields.into_pyobject(self.py)?.gt(&limit)? {
                let limit = self.module.getattr("MAX_HISTORY_REPORT_FIELDS")?.str()?;
                return Err(self.format_error(format!("财报记录字段数超出上限: {fields} > {limit}（report_size={size}）")));
            }
            Ok(())
        })();
        self.remember(result)
    }

    fn total_fields(&mut self, header: &FinancialHeader, index: usize, total: u64) -> eltdx::Result<()> {
        let result = (|| {
            let limit = self.module.getattr("MAX_HISTORY_TOTAL_FLOATS")?;
            if total.into_pyobject(self.py)?.gt(&limit)? {
                let limit = self.module.getattr("MAX_HISTORY_TOTAL_FLOATS")?.str()?;
                return Err(self.format_error(format!(
                    "财报总解析量超出上限: {total} > {limit} 个字段（已解析 {}/{} 条，report_size={}）",
                    index + 1, header.max_count(), header.report_size())));
            }
            Ok(())
        })();
        self.remember(result)
    }

    fn record(&mut self, record: &FinancialRecord) -> eltdx::Result<()> {
        let result = (|| {
            let fields = PyList::empty(self.py);
            for chunk in record.fields().chunks(4096) {
                self.py.check_signals()?;
                for value in chunk { fields.append(value.as_f64())?; }
            }
            self.result.append((record.code(), record.market_id(), &self.report_date, fields))
        })();
        self.remember(result)
    }
}

/// Exact bytes borrow their immutable buffer. Custom buffers/struct overrides
/// remain at the Python adapter; no complete Rust result table is allocated.
#[pyfunction]
pub fn financial_dat<'py>(py: Python<'py>, data: Bound<'py, PyBytes>,
    report_date: Bound<'py, PyAny>, module: Bound<'py, PyAny>) -> PyResult<Py<PyAny>>
{
    let mut projection = DatProjection {
        py, module, report_date, result: PyList::empty(py), error: None,
    };
    let result = eltdx::offline::financial::python_legacy::visit_dat(data.as_bytes(), &mut projection);
    if let Some(error) = projection.error.take() { return Err(error); }
    if let Err(error) = result { return Err(projection.format_error(error.message().to_owned())); }
    Ok(projection.result.into_any().unbind())
}
