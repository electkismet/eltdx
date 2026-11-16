//! Python scalar/object projection around the shared financial row parser.
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PySet};
use eltdx::offline::financial::{FinancialHeader, FinancialRecord,
    python_legacy::{DatArchiveSelection, DatCallbacks, select_dat_entry}};

fn exception(module: &Bound<'_, PyAny>, message: String) -> PyErr {
    match module.getattr("ResourceFormatError").and_then(|kind| kind.call1((message,))) {
        Ok(value) => PyErr::from_value(value),
        Err(error) => error,
    }
}

fn formatted(value: &Bound<'_, PyAny>) -> PyResult<String> {
    value.py().import("builtins")?.getattr("format")?.call1((value, ""))?.extract()
}

struct ZipSelection<'py> {
    py: Python<'py>, module: Bound<'py, PyAny>, maximum: Bound<'py, PyAny>, source: Bound<'py, PyAny>,
}
impl<'py> DatArchiveSelection<Bound<'py, PyAny>> for ZipSelection<'py> {
    type Error = PyErr;
    // The Python inventory length is checked before iterating its objects.
    fn check_count(&mut self, _: usize) -> PyResult<()> { Ok(()) }
    fn is_candidate(&mut self, entry: &Bound<'py, PyAny>) -> PyResult<bool> {
        self.py.check_signals()?;
        entry.getattr("filename")?.call_method1("endswith", (".dat",))?.is_truthy()
    }
    fn check_candidates(&mut self, entries: &[Bound<'py, PyAny>]) -> PyResult<()> {
        if entries.is_empty() {
            return Err(exception(&self.module, format!("zip 中未找到 .dat 文件: {}", formatted(&self.source)?)));
        }
        let names = PyList::empty(self.py);
        for entry in entries { names.append(entry.getattr("filename")?)?; }
        let duplicates = PySet::empty(self.py)?;
        for name in names.iter() {
            self.py.check_signals()?;
            if names.call_method1("count", (&name,))?.gt(1)? { duplicates.add(name)?; }
        }
        let duplicates = self.py.import("builtins")?.getattr("sorted")?.call1((duplicates,))?;
        if duplicates.is_truthy()? {
            return Err(exception(&self.module, format!(
                "zip 中存在重复的 .dat 文件名: {}（{}）", formatted(&duplicates)?, formatted(&self.source)?)));
        }
        Ok(())
    }
    fn choose(&mut self, _: &[Bound<'py, PyAny>]) -> PyResult<usize> { Ok(0) }
    fn check_selected(&mut self, entry: &Bound<'py, PyAny>) -> PyResult<()> {
        if self.py.import("operator")?.getattr("and_")?.call1((entry.getattr("flag_bits")?, 1))?.is_truthy()? {
            return Err(exception(&self.module, format!("zip 条目已加密: {}", formatted(&entry.getattr("filename")?)?)));
        }
        if entry.getattr("file_size")?.gt(&self.maximum)? {
            return Err(exception(&self.module, format!(
                "{} 解压后超出上限 {} 字节（声明 {} 字节，{}）",
                formatted(&entry.getattr("filename")?)?, formatted(&self.maximum)?,
                formatted(&entry.getattr("file_size")?)?, formatted(&self.source)?)));
        }
        Ok(())
    }
    fn invalid_selection(&mut self) -> PyErr {
        pyo3::exceptions::PyIndexError::new_err("financial archive selection is out of range")
    }
}

/// `zipfile` is an I/O/codec adapter here: its exact ZipInfo is retained and
/// passed to read, while inventory traversal shares the Rust selection core.
#[pyfunction]
pub fn financial_zip_member<'py>(py: Python<'py>, archive: Bound<'py, PyAny>,
    maximum: Bound<'py, PyAny>, max_entries: Bound<'py, PyAny>, source: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>) -> PyResult<Py<PyAny>>
{
    let infos = archive.call_method0("infolist")?;
    let count = infos.len()?;
    if count.into_pyobject(py)?.gt(&max_entries)? {
        return Err(exception(&module, format!("zip 条目过多: {count} > {}（{}）", formatted(&max_entries)?, formatted(&source)?)));
    }
    let mut selection = ZipSelection { py, module, maximum, source };
    let info = select_dat_entry(infos.try_iter()?, count, &mut selection)?;
    let data = archive.call_method1("read", (&info,))?;
    let size = data.len()?;
    if size.into_pyobject(py)?.gt(&selection.maximum)? {
        return Err(exception(&selection.module, format!(
            "{} 实际解压长度 {size} 超出上限 {} 字节（{}）", formatted(&info.getattr("filename")?)?,
            formatted(&selection.maximum)?, formatted(&selection.source)?)));
    }
    Ok(data.unbind())
}

// Match Python iterable unpacking, including requesting only the one extra
// element required to report excess values from a user-supplied generator.
fn unpack<'py>(value: Bound<'py, PyAny>, count: usize) -> PyResult<Vec<Bound<'py, PyAny>>> {
    let mut iterator = value.try_iter()?;
    let mut items = Vec::with_capacity(count);
    for _ in 0..count {
        let Some(item) = iterator.next() else {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "not enough values to unpack (expected {count}, got {})", items.len())));
        };
        items.push(item?);
    }
    if let Some(extra) = iterator.next() {
        extra?;
        return Err(pyo3::exceptions::PyValueError::new_err(format!(
            "too many values to unpack (expected {count})")));
    }
    Ok(items)
}

#[pyfunction]
pub fn financial_project_records(py: Python<'_>, records: Bound<'_, PyAny>,
    module: Bound<'_, PyAny>) -> PyResult<Py<PyAny>>
{
    let output = PyList::empty(py);
    for record in records.try_iter()? {
        py.check_signals()?;
        let row = unpack(record?, 4)?;
        // Fetch the live constructor before tuple conversion, matching Python
        // call evaluation if a custom fields iterator changes module state.
        let constructor = module.getattr("FinancialRecord")?;
        let args = PyDict::new(py);
        args.set_item("code", &row[0])?;
        args.set_item("market_id", &row[1])?;
        args.set_item("report_date", &row[2])?;
        let tuple = py.import("builtins")?.getattr("tuple")?;
        args.set_item("fields", tuple.call1((&row[3],))?)?;
        output.append(constructor.call((), Some(&args))?)?;
    }
    Ok(output.into_any().unbind())
}

#[pyfunction]
pub fn financial_list_archives(py: Python<'_>, client: Bound<'_, PyAny>,
    module: Bound<'_, PyAny>) -> PyResult<Py<PyAny>>
{
    let downloader = client.getattr("resources")?.getattr("download_file")?;
    let data = downloader.call1((module.getattr("FINANCIAL_LIST_PATH")?,))?;
    let rows = module.getattr("parse_financial_file_list")?.call1((data,))?;
    let output = PyList::empty(py);
    for row in rows.try_iter()? {
        py.check_signals()?;
        let row = unpack(row?, 3)?;
        let constructor = module.getattr("FinancialFileInfo")?;
        let args = PyDict::new(py);
        args.set_item("filename", &row[0])?;
        args.set_item("hash", &row[1])?;
        args.set_item("filesize", &row[2])?;
        output.append(constructor.call((), Some(&args))?)?;
    }
    Ok(output.into_any().unbind())
}

#[pyfunction]
pub fn financial_download_archive(py: Python<'_>, client: Bound<'_, PyAny>,
    filename: Bound<'_, PyAny>, module: Bound<'_, PyAny>) -> PyResult<Py<PyAny>>
{
    let downloader = client.getattr("resources")?.getattr("download_file")?;
    let args = PyDict::new(py);
    args.set_item("max_bytes", module.getattr("MAX_FINANCIAL_ZIP_BYTES")?)?;
    Ok(downloader.call((filename,), Some(&args))?.unbind())
}

#[pyfunction]
pub fn financial_filename_date(py: Python<'_>, filename: Bound<'_, PyAny>,
    module: Bound<'_, PyAny>) -> PyResult<Py<PyAny>>
{
    // Keep Python runtime Unicode regex/date matching and platform Path rules.
    let search = module.getattr("_REPORT_DATE_RE")?.getattr("search")?;
    let leaf = module.getattr("Path")?.call1((filename,))?.getattr("name")?;
    let found = search.call1((leaf,))?;
    if !found.is_truthy()? { return Ok(0_u32.into_pyobject(py)?.into_any().unbind()); }
    let integer = py.import("builtins")?.getattr("getattr")?.call1((
        &module, "int", py.import("builtins")?.getattr("int")?))?;
    Ok(integer.call1((found.call_method1("group", (1,))?,))?.unbind())
}

#[pyfunction]
pub fn financial_archive_payload(py: Python<'_>, client: Bound<'_, PyAny>,
    filename: Bound<'_, PyAny>, module: Bound<'_, PyAny>) -> PyResult<Py<PyAny>>
{
    let data = module.getattr("download_archive")?.call1((client, &filename))?;
    if !data.is_truthy()? { return Ok(py.None()); }
    let size = data.len()?;
    if size.into_pyobject(py)?.gt(module.getattr("MAX_FINANCIAL_ZIP_BYTES")?)? {
        return Err(exception(&module, format!(
            "财报 zip 超出压缩体积上限 {} 字节: {} (实际 {} 字节)",
            formatted(&module.getattr("MAX_FINANCIAL_ZIP_BYTES")?)?, formatted(&filename)?, data.len()?)));
    }
    Ok(data.unbind())
}

#[pyfunction]
pub fn financial_finish_records(py: Python<'_>, data: Bound<'_, PyAny>,
    filename: Bound<'_, PyAny>, module: Bound<'_, PyAny>) -> PyResult<Py<PyAny>>
{
    let date = financial_filename_date(py, filename, module.clone())?;
    let rows = module.getattr("parse_financial_dat")?.call1((data, date))?;
    financial_project_records(py, rows, module)
}

#[pyfunction]
pub fn financial_read_local(py: Python<'_>, filepath: Bound<'_, PyAny>,
    module: Bound<'_, PyAny>) -> PyResult<Py<PyAny>>
{
    let filepath = module.getattr("Path")?.call1((filepath,))?;
    if !filepath.call_method0("is_file")?.is_truthy()? {
        return Err(pyo3::exceptions::PyFileNotFoundError::new_err(format!(
            "历史财务数据文件不存在: {}", formatted(&filepath)?)));
    }
    let data = if filepath.getattr("suffix")?.call_method0("lower")?.eq(".zip")? {
        module.getattr("_read_from_zip")?.call1((filepath,))?
    } else { filepath.call_method0("read_bytes")? };
    let rows = module.getattr("parse_financial_dat")?.call1((data,))?;
    financial_project_records(py, rows, module)
}

#[pyfunction]
pub fn financial_zip_path_guard(py: Python<'_>, filepath: Bound<'_, PyAny>,
    module: Bound<'_, PyAny>) -> PyResult<()>
{
    py.check_signals()?;
    let size = filepath.call_method0("stat")?.getattr("st_size")?;
    if size.gt(module.getattr("MAX_HISTORY_ZIP_BYTES")?)? {
        return Err(exception(&module, format!(
            "历史财务 zip 超出体积上限 {} 字节: {}（实际 {} 字节）",
            formatted(&module.getattr("MAX_HISTORY_ZIP_BYTES")?)?, formatted(&filepath)?, formatted(&size)?)));
    }
    Ok(())
}

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
        exception(&self.module, message)
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
                let limit = formatted(&self.module.getattr("MAX_HISTORY_REPORT_FIELDS")?)?;
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
                let limit = formatted(&self.module.getattr("MAX_HISTORY_TOTAL_FLOATS")?)?;
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
