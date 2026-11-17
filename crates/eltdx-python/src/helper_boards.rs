//! Legacy board orchestration. Python Path/JSON objects preserve the existing
//! directory discovery, cache publication order and extension hooks.
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes, PyDict, PyList, PyTuple};
use crate::helper_numeric::callback;

fn get<'py>(value: &Bound<'py, PyAny>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    value.py().import("builtins")?.getattr("getattr")?.call1((value, name, value.py().None()))
}

fn path<'py>(service: &Bound<'py, PyAny>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    service.getattr("_data_dir")?.call_method1("__truediv__", (name,))
}

fn tuple<'py>(value: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    value.py().import("builtins")?.getattr("tuple")?.call1((value,))
}

fn prepared<'py>(py: Python<'py>, day: &Bound<'py, PyAny>, boards: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyDict>> {
    let value = PyDict::new(py);
    value.set_item("date", day)?; value.set_item("boards", boards)?;
    Ok(value)
}

fn publish(service: &Bound<'_, PyAny>, boards: &Bound<'_, PyAny>, security: &Bound<'_, PyAny>, day: &Bound<'_, PyAny>) -> PyResult<()> {
    service.setattr("_boards", tuple(boards)?)?;
    service.setattr("_security", security)?;
    service.setattr("_prepared_date", day)
}

fn quote_map<'py>(py: Python<'py>, quotes: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyDict>> {
    let result = PyDict::new(py);
    for row in quotes.try_iter()? {
        let row = row?;
        let code = py.import("builtins")?.getattr("getattr")?.call1((&row, "full_code", ""))?;
        result.set_item(code.str()?, row)?;
    }
    Ok(result)
}

#[pyfunction]
pub fn board_quotes(py: Python<'_>, service: Bound<'_, PyAny>, category: Bound<'_, PyAny>, refresh: Bound<'_, PyAny>,
    scope: Bound<'_, PyDict>) -> PyResult<Py<PyAny>> {
    let categories = callback(&scope, "BOARD_CATEGORIES")?;
    if !categories.contains(&category)? {
        return Err(pyo3::exceptions::PyValueError::new_err(format!("unsupported board category: {}; choose from {}",
            category.repr()?.to_str()?, categories.str()?.to_str()?)));
    }
    let kwargs = PyDict::new(py); kwargs.set_item("refresh", refresh)?;
    let data = service.call_method("_prepare", (), Some(&kwargs))?;
    let selected = PyList::empty(py);
    let codes = PyList::empty(py);
    for item in data.get_item("boards")?.try_iter()? {
        py.check_signals()?;
        let item = item?;
        if callback(&scope, "_matches_category")?.call1((&item, &category))?.is_truthy()? {
            selected.append(&item)?;
        }
    }
    for item in selected.iter() { codes.append(item.get_item("full_code")?)?; }
    let quotes = service.call_method1("_snapshot_batches", (codes,))?;
    let by_code = quote_map(py, &quotes)?;
    let rows = PyList::empty(py);
    for board in selected.iter() {
        rows.append(service.call_method1("_quote_row", (&board, by_code.call_method1("get", (board.get_item("full_code")?,))?))?)?;
    }
    let fields = PyDict::new(py);
    fields.set_item("rows", tuple(rows.as_any())?)?;
    fields.set_item("prepared_date", data.get_item("date")?)?;
    Ok(callback(&scope, "BoardQuoteTable")?.call((), Some(&fields))?.unbind())
}

#[pyfunction]
pub fn board_member_quotes(py: Python<'_>, service: Bound<'_, PyAny>, code: Bound<'_, PyAny>, refresh: Bound<'_, PyAny>,
    scope: Bound<'_, PyDict>) -> PyResult<Py<PyAny>> {
    if !code.is_instance_of::<pyo3::types::PyString>() || !code.call_method0("strip")?.is_truthy()? {
        return Err(pyo3::exceptions::PyValueError::new_err("board_code is required"));
    }
    let kwargs = PyDict::new(py); kwargs.set_item("refresh", refresh)?;
    let data = service.call_method("_prepare", (), Some(&kwargs))?;
    let key = code.call_method0("strip")?.call_method0("lower")?;
    let mut selected = None;
    for item in data.get_item("boards")?.try_iter()? {
        let item = item?;
        if item.get_item("board_code")?.call_method0("lower")?.eq(&key)?
            || item.get_item("full_code")?.call_method0("lower")?.eq(&key)? { selected = Some(item); break; }
    }
    let Some(board) = selected else { return Err(pyo3::exceptions::PyValueError::new_err(format!("unknown board code: {}", code.repr()?.to_str()?))); };
    let detail = service.call_method1("_members_for", (&board,))?;
    let codes = PyList::empty(py);
    for item in detail.get_item("display_members")?.try_iter()? {
        let item = item?;
        codes.append(service.call_method1("_full_security_code", (item.get_item("market")?, item.get_item("code")?))?)?;
    }
    let quotes = service.call_method1("_snapshot_batches", (&codes,))?;
    let by_code = quote_map(py, &quotes)?;
    let rows = PyList::empty(py);
    for code in codes.iter() {
        let item = PyDict::new(py);
        item.set_item("board_code", board.get_item("board_code")?)?;
        item.set_item("board_name", board.get_item("board_name")?)?; item.set_item("full_code", &code)?;
        rows.append(service.call_method1("_quote_row", (item, by_code.call_method1("get", (code,))?))?)?;
    }
    let fields = PyDict::new(py);
    fields.set_item("board_code", board.get_item("board_code")?)?; fields.set_item("board_name", board.get_item("board_name")?)?;
    for field in ["raw_members", "display_members", "excluded_members"] { fields.set_item(field, tuple(&detail.get_item(field)?)?)?; }
    fields.set_item("rows", tuple(rows.as_any())?)?; fields.set_item("prepared_date", data.get_item("date")?)?;
    Ok(callback(&scope, "BoardMemberQuoteTable")?.call((), Some(&fields))?.unbind())
}

#[pyfunction]
pub fn board_prepare(py: Python<'_>, service: Bound<'_, PyAny>, refresh: Bound<'_, PyAny>, scope: Bound<'_, PyDict>) -> PyResult<Py<PyAny>> {
    let today = service.call_method0("_market_date")?;
    if !refresh.is_truthy()? && service.getattr("_prepared_date")?.eq(&today)?
        && !service.getattr("_boards")?.is_none() && !service.getattr("_security")?.is_none() {
        return Ok(prepared(py, &today, &service.getattr("_boards")?)?.into_any().unbind());
    }
    let options = PyDict::new(py); options.set_item("parents", true)?; options.set_item("exist_ok", true)?;
    service.getattr("_data_dir")?.call_method("mkdir", (), Some(&options))?;
    let metadata_path = path(&service, ".eltdx_board_cache.json")?;
    let metadata = callback(&scope, "_read_json")?.call1((&metadata_path,))?;
    if !refresh.is_truthy()? && metadata.call_method1("get", ("last_success_date",))?.eq(today.call_method0("isoformat")?)?
        && service.call_method0("_cache_files_ready")?.is_truthy()? {
        let boards = service.call_method0("_parse_boards")?;
        let security = service.call_method0("_load_security_file")?;
        if boards.is_truthy()? && !security.is_none() {
            publish(&service, &boards, &security, &today)?;
            return Ok(prepared(py, &today, &service.getattr("_boards")?)?.into_any().unbind());
        }
    }
    let update = if refresh.is_truthy()? { refresh }
        else {
            let changed = metadata.call_method1("get", ("last_success_date",))?.ne(today.call_method0("isoformat")?)?;
            changed.into_pyobject(py)?.to_owned().into_any()
        };
    let kwargs = PyDict::new(py); kwargs.set_item("refresh", update)?;
    service.call_method("_download_missing_or_refresh", (), Some(&kwargs))?;
    let boards = service.call_method0("_parse_boards")?;
    if !boards.is_truthy()? { return Err(pyo3::exceptions::PyRuntimeError::new_err("no board definitions found; configure definitions_dir or ELTDX_BOARD_DATA_DIR")); }
    let mut security = service.call_method0("_load_security_from_044d")?;
    if !security.is_truthy()? {
        security = service.call_method0("_load_security_file")?;
        if !security.is_truthy()? { security = PyDict::new(py).into_any(); }
    } else { service.call_method1("_write_security_file", (&security,))?; }
    publish(&service, &boards, &security, &today)?;
    // Publish in-memory state before serializing metadata, preserving legacy
    // behavior when the metadata write itself fails.
    let metadata = PyDict::new(py);
    metadata.set_item("last_success_date", today.call_method0("isoformat")?)?;
    metadata.set_item("downloaded_at", callback(&scope, "datetime")?.call_method1("now", (callback(&scope, "timezone")?.getattr("utc")?,))?.call_method0("isoformat")?)?;
    let hashes = PyDict::new(py);
    for name in callback(&scope, "BOARD_FILES")?.try_iter()? {
        let name = name?;
        let file = service.getattr("_data_dir")?.call_method1("__truediv__", (&name,))?;
        if file.call_method0("exists")?.is_truthy()? { hashes.set_item(&name, callback(&scope, "_sha256")?.call1((file,))?)?; }
    }
    metadata.set_item("sha256", hashes)?;
    let options = PyDict::new(py); options.set_item("ensure_ascii", false)?; options.set_item("indent", 2)?;
    let json = callback(&scope, "json")?.call_method("dumps", (metadata,), Some(&options))?;
    let text = py.import("operator")?.getattr("add")?.call1((json, "\n"))?;
    let options = PyDict::new(py); options.set_item("encoding", "utf-8")?;
    metadata_path.call_method("write_text", (text,), Some(&options))?;
    Ok(prepared(py, &today, &service.getattr("_boards")?)?.into_any().unbind())
}

#[pyfunction]
pub fn board_download(py: Python<'_>, service: Bound<'_, PyAny>, refresh: Bound<'_, PyAny>, names: Bound<'_, PyAny>) -> PyResult<()> {
    for name in names.try_iter()? {
        py.check_signals()?;
        let name = name?;
        let target = service.getattr("_data_dir")?.call_method1("__truediv__", (&name,))?;
        if target.call_method0("exists")?.is_truthy()? && !refresh.is_truthy()? { continue; }
        let payload = match service.getattr("_client").and_then(|client| client.getattr("resources"))
            .and_then(|resources| resources.call_method1("download_file", (name,))) {
            Ok(payload) => payload,
            Err(error) if error.is_instance_of::<PyException>(py) => continue,
            Err(error) => return Err(error),
        };
        if (payload.is_instance_of::<PyBytes>() || payload.is_instance_of::<PyByteArray>()) && payload.is_truthy()? {
            target.call_method1("write_bytes", (py.import("builtins")?.getattr("bytes")?.call1((payload,))?,))?;
        }
    }
    Ok(())
}

#[pyfunction]
pub fn board_security(py: Python<'_>, service: Bound<'_, PyAny>, markets: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let result = PyDict::new(py);
    for market in ["sz", "sh", "bj"] {
        let rows = match service.getattr("_client").and_then(|client| client.getattr("codes"))
            .and_then(|codes| codes.call_method1("all", (market,))) {
            Ok(rows) => rows,
            Err(error) if error.is_instance_of::<PyException>(py) => PyTuple::empty(py).into_any(),
            Err(error) => return Err(error),
        };
        if rows.is_truthy()? {
            for row in rows.try_iter()? {
                let row = row?;
                let id = py.import("builtins")?.getattr("getattr")?.call1((&row, "market_id", markets.get_item(market)?))?;
                let code = py.import("builtins")?.getattr("getattr")?.call1((&row, "code", ""))?.str()?;
                if code.is_truthy()? {
                    result.set_item((py.import("builtins")?.getattr("int")?.call1((id,))?, code), row)?;
                }
            }
        }
    }
    Ok(result.into_any().unbind())
}

#[pyfunction]
pub fn board_quote_row(py: Python<'_>, board: Bound<'_, PyAny>, quote: Bound<'_, PyAny>, scope: Bound<'_, PyDict>) -> PyResult<Py<PyAny>> {
    let code = board.get_item("full_code")?.str()?;
    let last = get(&quote, "last_price")?;
    let pre = get(&quote, "pre_close_price")?;
    let op = py.import("operator")?;
    let change = if !last.is_none() && !pre.is_none() { op.getattr("sub")?.call1((&last, &pre))? } else { py.None().into_bound(py) };
    let pct = if !change.is_none() && pre.is_truthy()? {
        let ratio = eltdx::__python_compat::ratio_with(change.clone().unbind(), pre.clone().unbind(),
            |left, right| Ok::<_, PyErr>(op.getattr("truediv")?.call1((left.bind(py), right.bind(py)))?.unbind()))?;
        op.getattr("mul")?.call1((ratio, 100))?
    } else { py.None().into_bound(py) };
    let fields = PyDict::new(py);
    fields.set_item("board_code", board.get_item("board_code")?)?; fields.set_item("board_name", board.get_item("board_name")?)?;
    fields.set_item("full_code", &code)?;
    let exchange = code.get_item(pyo3::types::PySlice::new(py, 0, 2, 1))?;
    fields.set_item("exchange", &exchange)?;
    fields.set_item("market_id", callback(&scope, "MARKET_TO_ID")?.call_method1("get", (&exchange, board.call_method1("get", ("market_id", 0))?))?)?;
    fields.set_item("last_price", last)?; fields.set_item("pre_close_price", pre)?;
    for name in ["open_price", "high_price", "low_price"] { fields.set_item(name, get(&quote, name)?)?; }
    fields.set_item("change", change)?; fields.set_item("change_pct", pct)?; fields.set_item("amount", get(&quote, "amount")?)?;
    fields.set_item("volume_hand", get(&quote, "total_hand")?)?; fields.set_item("quote", quote)?;
    Ok(callback(&scope, "BoardQuoteRow")?.call((), Some(&fields))?.unbind())
}

#[pyfunction]
pub fn board_date(py: Python<'_>, service: Bound<'_, PyAny>, day_type: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let result: PyResult<Option<Py<PyAny>>> = (|| {
        let handshake = service.getattr("_client")?.getattr("session")?.call_method0("handshake")?;
        let dates = PyList::empty(py);
        for name in ["server_date_1", "server_date_2"] {
            let value = get(&handshake, name)?;
            if value.is_instance(&day_type)? { dates.append(value)?; }
        }
        if dates.is_truthy()? { Ok(Some(py.import("builtins")?.getattr("max")?.call1((dates,))?.unbind())) }
        else { Ok(None) }
    })();
    match result {
        Ok(Some(value)) => Ok(value),
        Ok(None) => Ok(day_type.call_method0("today")?.unbind()),
        Err(error) if error.is_instance_of::<PyException>(py) => Ok(day_type.call_method0("today")?.unbind()),
        Err(error) => Err(error),
    }
}

#[pyfunction]
pub fn board_definition_paths(py: Python<'_>, service: Bound<'_, PyAny>, scope: Bound<'_, PyDict>) -> PyResult<Py<PyAny>> {
    let candidates = PyList::empty(py);
    let definitions = service.getattr("_definitions_dir")?;
    if definitions.is_truthy()? { candidates.append(service.getattr("_definitions_dir")?)?; }
    candidates.append(service.getattr("_data_dir")?)?;
    candidates.append(callback(&scope, "Path")?.call_method0("cwd")?.call_method1("__truediv__", ("downloads",))?)?;
    candidates.append(callback(&scope, "Path")?.call1((r"C:\APP\tdx\T0002\hq_cache",))?)?;
    for root in candidates.iter() {
        let mut all = true;
        for name in callback(&scope, "DEFINITION_FILES")?.try_iter()? {
            if !root.call_method1("__truediv__", (name?,))?.call_method0("exists")?.is_truthy()? { all = false; break; }
        }
        if all {
            let result = PyList::empty(py);
            for name in callback(&scope, "DEFINITION_FILES")?.try_iter()? { result.append(root.call_method1("__truediv__", (name?,))?)?; }
            return Ok(result.into_any().unbind());
        }
    }
    Ok(PyList::empty(py).into_any().unbind())
}

#[pyfunction]
pub fn board_cache_ready(py: Python<'_>, service: Bound<'_, PyAny>, names: Bound<'_, PyAny>) -> PyResult<bool> {
    for name in names.try_iter()? {
        py.check_signals()?;
        if service.getattr("_data_dir")?.call_method1("__truediv__", (name?,))?.call_method0("exists")?.is_truthy()? {
            return service.call_method0("_definition_paths")?.is_truthy();
        }
    }
    Ok(false)
}

#[pyfunction]
pub fn board_market(py: Python<'_>, code: Bound<'_, PyAny>, declared: Bound<'_, PyAny>, mapping: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    if code.call_method1("startswith", ("399",))?.is_truthy()? { return Ok("sz".into_pyobject(py)?.into_any().unbind()); }
    if code.call_method1("startswith", ("880",))?.is_truthy()? || code.call_method1("startswith", ("881",))?.is_truthy()? {
        return Ok("sh".into_pyobject(py)?.into_any().unbind());
    }
    if pyo3::types::PySet::new(py, ["0", "1", "2"])?.contains(&declared)? {
        return Ok(mapping.get_item(py.import("builtins")?.getattr("int")?.call1((declared,))?)?.unbind());
    }
    Ok("sh".into_pyobject(py)?.into_any().unbind())
}

#[pyfunction]
pub fn board_matches(py: Python<'_>, board: Bound<'_, PyAny>, category: Bound<'_, PyAny>, re: Bound<'_, PyAny>) -> PyResult<bool> {
    let value = py.import("builtins")?.getattr("int")?.call1((board.get_item("category")?,))?;
    if category.eq("全部")? { return Ok(true); }
    for (name, number) in [("概念", 4), ("风格", 5), ("地区", 3), ("880行业", 2)] {
        if category.eq(name)? { return value.eq(number); }
    }
    for (name, pattern) in [("二级行业", r"X\d{4}"), ("一级行业", r"X\d{2}"), ("三级行业", r"X\d{6}")] {
        if category.eq(name)? { return Ok(value.eq(12)? && !re.call_method1("fullmatch", (pattern, board.get_item("membership_key")?))?.is_none()); }
    }
    Ok(false)
}

#[pyfunction]
pub fn board_infoharbor(py: Python<'_>, source: Bound<'_, PyAny>, scope: Bound<'_, PyDict>) -> PyResult<Py<PyAny>> {
    let result = PyList::empty(py);
    for line in callback(&scope, "_read_text")?.call1((source,))?.call_method0("splitlines")?.try_iter()? {
        let line = line?;
        if !line.call_method1("startswith", ("#",))?.is_truthy()? { continue; }
        let fields = line.call_method1("split", (",",))?;
        if fields.len()? < 3 || callback(&scope, "re")?.call_method1("fullmatch", (r"\d{6}", fields.get_item(2)?.call_method0("strip")?))?.is_none() { continue; }
        let name = fields.get_item(0)?.get_item(pyo3::types::PySlice::new(py, 1, isize::MAX, 1))?.call_method0("strip")?;
        let kind = if name.call_method1("startswith", ("FG_",))?.is_truthy()? { Some(5) }
            else if name.call_method1("startswith", ("GN_",))?.is_truthy()? { Some(4) } else { None };
        if let Some(kind) = kind {
            result.append((fields.get_item(2)?.call_method0("strip")?, name.get_item(pyo3::types::PySlice::new(py, 3, isize::MAX, 1))?, kind))?;
        }
    }
    Ok(result.into_any().unbind())
}

#[pyfunction]
pub fn board_parse(py: Python<'_>, service: Bound<'_, PyAny>, scope: Bound<'_, PyDict>) -> PyResult<Py<PyAny>> {
    let definitions = PyDict::new(py);
    for path in service.call_method0("_definition_paths")?.try_iter()? {
        for line in callback(&scope, "_read_text")?.call1((path?,))?.call_method0("splitlines")?.try_iter()? {
            py.check_signals()?;
            let fields = line?.call_method1("split", ("|",))?;
            if fields.len()? >= 6 && !callback(&scope, "re")?.call_method1("fullmatch", (r"\d{6}", fields.get_item(1)?.call_method0("strip")?))?.is_none() {
                definitions.call_method1("setdefault", (fields.get_item(1)?.call_method0("strip")?, fields))?;
            }
        }
    }
    let headers = callback(&scope, "_infoharbor_headers")?.call1((path(&service, "infoharbor_block.dat")?,))?;
    for row in headers.try_iter()? {
        let row = tuple(&row?)?;
        if row.len()? != 3 { return Err(pyo3::exceptions::PyValueError::new_err("expected board code, name and category")); }
        let code = row.get_item(0)?; let name = row.get_item(1)?; let category = row.get_item(2)?.str()?;
        let fields = PyList::empty(py);
        for value in [name.into_any(), code.clone(), category.into_any(), "1".into_pyobject(py)?.into_any(), "0".into_pyobject(py)?.into_any(), "".into_pyobject(py)?.into_any()] { fields.append(value)?; }
        definitions.call_method1("setdefault", (code, fields))?;
    }
    let result = PyList::empty(py);
    for (code, fields) in definitions.iter() {
        let category = fields.get_item(2)?;
        let category = if category.call_method0("isdigit")?.is_truthy()? { py.import("builtins")?.getattr("int")?.call1((category,))? }
            else { 4_i32.into_pyobject(py)?.into_any() };
        let name = fields.get_item(0)?.call_method0("strip")?;
        let name = if name.is_truthy()? { name } else { code.clone() };
        let declared = if fields.len()? > 3 { fields.get_item(3)? } else { py.None().into_bound(py) };
        let market = callback(&scope, "_board_market")?.call1((&code, declared))?;
        let key = if fields.len()? > 5 { fields.get_item(5)?.call_method0("strip")? } else { "".into_pyobject(py)?.into_any() };
        let row = PyDict::new(py);
        row.set_item("board_code", &code)?; row.set_item("board_name", name)?; row.set_item("category", category)?;
        row.set_item("membership_key", key)?;
        row.set_item("full_code", py.import("operator")?.getattr("add")?.call1((&market, code))?)?;
        row.set_item("market_id", callback(&scope, "MARKET_TO_ID")?.get_item(market)?)?;
        result.append(row)?;
    }
    Ok(result.into_any().unbind())
}

#[pyfunction]
pub fn board_members(py: Python<'_>, service: Bound<'_, PyAny>, board: Bound<'_, PyAny>, scope: Bound<'_, PyDict>) -> PyResult<Py<PyAny>> {
    let code = board.get_item("board_code")?;
    if service.getattr("_members")?.contains(&code)? { return Ok(service.getattr("_members")?.get_item(code)?.unbind()); }
    let category = board.get_item("category")?;
    let key = board.get_item("membership_key")?;
    let members = PyList::empty(py);
    let int = py.import("builtins")?.getattr("int")?;
    if PyTuple::new(py, [4, 5])?.contains(&category)? {
        let mut active = false;
        let mut expected = py.None();
        for line in callback(&scope, "_read_text")?.call1((path(&service, "infoharbor_block.dat")?,))?.call_method0("splitlines")?.try_iter()? {
            py.check_signals()?;
            let line = line?;
            if line.call_method1("startswith", ("#",))?.is_truthy()? {
                let parts = line.call_method1("split", (",",))?;
                active = parts.len()? > 2 && parts.get_item(2)?.call_method0("strip")?.eq(&code)?;
                expected = if active && parts.get_item(1)?.call_method0("isdigit")?.is_truthy()? { int.call1((parts.get_item(1)?,))?.unbind() } else { py.None() };
            } else if active {
                for pair in callback(&scope, "re")?.call_method1("findall", (r"([012])#(\d{6})", line))?.try_iter()? {
                    let pair = pair?;
                    members.append((int.call1((pair.get_item(0)?,))?, pair.get_item(1)?))?;
                }
            }
        }
        if !expected.is_none(py) {
            let count = py.import("builtins")?.getattr("set")?.call1((&members,))?.len()?.into_pyobject(py)?.into_any();
            if !count.eq(expected.bind(py))? {
                return Err(pyo3::exceptions::PyValueError::new_err(format!("incomplete board membership: {}", code.str()?.to_str()?)));
            }
        }
    } else if PyTuple::new(py, [2, 12])?.contains(&category)? {
        let column = if category.eq(2)? { 2 } else { 5 };
        for line in callback(&scope, "_read_text")?.call1((path(&service, "tdxhy.cfg")?,))?.call_method0("splitlines")?.try_iter()? {
            let fields = line?.call_method1("split", ("|",))?;
            if fields.len()? > column && fields.get_item(column)?.call_method1("startswith", (&key,))?.is_truthy()? {
                members.append((int.call1((fields.get_item(0)?,))?, fields.get_item(1)?))?;
            }
        }
    } else if category.eq(3)? {
        let source = callback(&scope, "_read_dbf_members")?.call1((path(&service, "base.dbf")?, key))?;
        members.call_method1("extend", (source,))?;
    } else { return Err(pyo3::exceptions::PyValueError::new_err(format!("unsupported board category: {}", category.str()?.to_str()?))); }
    let unique = PyList::empty(py); let seen = pyo3::types::PySet::empty(py)?;
    for item in members.iter() { if !seen.contains(&item)? { seen.add(&item)?; unique.append(item)?; } }
    let raw = PyList::empty(py);
    for item in unique.iter() {
        let row = PyDict::new(py); row.set_item("market", item.get_item(0)?)?; row.set_item("code", item.get_item(1)?)?; raw.append(row)?;
    }
    let security = service.getattr("_security")?;
    let security = if security.is_truthy()? { security } else { PyDict::new(py).into_any() };
    let display = PyList::empty(py); let excluded = PyList::empty(py);
    for item in unique.iter() {
        if security.contains(&item)? { let row = PyDict::new(py); row.set_item("market", item.get_item(0)?)?; row.set_item("code", item.get_item(1)?)?; display.append(row)?; }
    }
    for item in unique.iter() {
        if !security.contains(&item)? { let row = PyDict::new(py); row.set_item("market", item.get_item(0)?)?; row.set_item("code", item.get_item(1)?)?;
            row.set_item("reason", "absent_from_current_security_list")?; excluded.append(row)?; }
    }
    let result = PyDict::new(py);
    result.set_item("raw_members", raw)?; result.set_item("display_members", display)?; result.set_item("excluded_members", excluded)?;
    service.getattr("_members")?.set_item(code, &result)?;
    Ok(result.into_any().unbind())
}

#[pyfunction]
pub fn board_read_text(py: Python<'_>, path: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    for encoding in ["gb18030", "gbk", "utf-8"] {
        let options = PyDict::new(py); options.set_item("encoding", encoding)?;
        match path.call_method("read_text", (), Some(&options)) {
            Ok(value) => return Ok(value.unbind()),
            Err(error) if error.is_instance_of::<pyo3::exceptions::PyUnicodeDecodeError>(py)
                || error.is_instance_of::<pyo3::exceptions::PyOSError>(py) => {},
            Err(error) => return Err(error),
        }
    }
    Ok("".into_pyobject(py)?.into_any().unbind())
}

#[pyfunction]
pub fn board_read_json(py: Python<'_>, path: Bound<'_, PyAny>, json: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let result: PyResult<Py<PyAny>> = (|| {
        let options = PyDict::new(py); options.set_item("encoding", "utf-8")?;
        let value = json.call_method1("loads", (path.call_method("read_text", (), Some(&options))?,))?;
        if value.is_instance_of::<PyDict>() { Ok(value.unbind()) } else { Ok(PyDict::new(py).into_any().unbind()) }
    })();
    match result {
        Err(error) if error.is_instance_of::<pyo3::exceptions::PyOSError>(py)
            || error.is_instance_of::<pyo3::exceptions::PyValueError>(py) => Ok(PyDict::new(py).into_any().unbind()),
        other => other,
    }
}

#[pyfunction]
pub fn board_read_security(py: Python<'_>, service: Bound<'_, PyAny>, json: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let file = path(&service, "security_list.json")?;
    if !file.call_method0("exists")?.is_truthy()? { return Ok(py.None()); }
    let result: PyResult<Py<PyAny>> = (|| {
        let options = PyDict::new(py); options.set_item("encoding", "utf-8")?;
        let rows = json.call_method1("loads", (file.call_method("read_text", (), Some(&options))?,))?;
        let result = PyDict::new(py);
        for item in rows.try_iter()? {
            let item = item?;
            let market = py.import("builtins")?.getattr("int")?.call1((item.get_item("market")?,))?;
            let code = item.get_item("code")?.str()?;
            result.set_item((market, code), item)?;
        }
        Ok(result.into_any().unbind())
    })();
    match result {
        Err(error) if error.is_instance_of::<pyo3::exceptions::PyOSError>(py)
            || error.is_instance_of::<pyo3::exceptions::PyValueError>(py)
            || error.is_instance_of::<pyo3::exceptions::PyKeyError>(py)
            || error.is_instance_of::<pyo3::exceptions::PyTypeError>(py) => Ok(py.None()),
        other => other,
    }
}

#[pyfunction]
pub fn board_write_security(py: Python<'_>, service: Bound<'_, PyAny>, security: Bound<'_, PyAny>, json: Bound<'_, PyAny>) -> PyResult<()> {
    let items = py.import("builtins")?.getattr("sorted")?.call1((security.call_method0("items")?,))?;
    let rows = PyList::empty(py);
    for pair in items.try_iter()? {
        let pair = pair?; let key = pair.get_item(0)?;
        let row = PyDict::new(py);
        row.set_item("market", key.get_item(0)?)?; row.set_item("code", key.get_item(1)?)?;
        row.set_item("name", get(&pair.get_item(1)?, "name")?)?; rows.append(row)?;
    }
    let options = PyDict::new(py); options.set_item("ensure_ascii", false)?; options.set_item("indent", 2)?;
    let json = json.call_method("dumps", (rows,), Some(&options))?;
    let text = py.import("operator")?.getattr("add")?.call1((json, "\n"))?;
    let options = PyDict::new(py); options.set_item("encoding", "utf-8")?;
    path(&service, "security_list.json")?.call_method("write_text", (text,), Some(&options))?;
    Ok(())
}

fn slice<'py>(data: &Bound<'py, PyAny>, start: Bound<'py, PyAny>, end: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    data.get_item(data.py().import("builtins")?.getattr("slice")?.call1((start, end))?)
}

#[pyfunction]
pub fn board_dbf_members(py: Python<'_>, path: Bound<'_, PyAny>, key: Bound<'_, PyAny>, scope: Bound<'_, PyDict>) -> PyResult<Py<PyAny>> {
    let data = path.call_method0("read_bytes")?;
    let header = callback(&scope, "struct")?.call_method1("unpack_from", ("<IHH", &data, 4))?;
    let count = header.get_item(0)?; let header_size = header.get_item(1)?; let record_size = header.get_item(2)?;
    let fields = PyDict::new(py); let mut offset = 1_usize;
    let builtin = py.import("builtins")?; let op = py.import("operator")?;
    let end = op.getattr("sub")?.call1((&header_size, 1))?;
    for pos in builtin.getattr("range")?.call1((32, end, 32))?.try_iter()? {
        let pos = pos?;
        if data.get_item(&pos)?.eq(13)? { break; }
        let end = op.getattr("add")?.call1((&pos, 11))?;
        let name = slice(&data, pos.clone(), end)?.call_method1("split", (PyBytes::new(py, b"\0"),))?.get_item(0)?.call_method1("decode", ("ascii",))?;
        let size = data.get_item(op.getattr("add")?.call1((pos, 16))?)?.extract::<usize>()?;
        fields.set_item(name, (offset, size))?; offset += size;
    }
    let result = pyo3::types::PySet::empty(py)?;
    for index in builtin.getattr("range")?.call1((count,))?.try_iter()? {
        py.check_signals()?;
        let index = index?;
        let start = op.getattr("add")?.call1((&header_size, op.getattr("mul")?.call1((&index, &record_size))?))?;
        let end = op.getattr("add")?.call1((&header_size, op.getattr("mul")?.call1((op.getattr("add")?.call1((index, 1))?, &record_size))?))?;
        let record = slice(&data, start, end)?;
        if !record.is_truthy()? || record.get_item(0)?.eq(42)? { continue; }
        let values = PyDict::new(py);
        for (name, span) in fields.iter() {
            if name.eq("SC")? || name.eq("GPDM")? || name.eq("DY")? {
                let start = span.get_item(0)?;
                let end = op.getattr("add")?.call1((&start, span.get_item(1)?))?;
                values.set_item(name, slice(&record, start, end)?.call_method1("decode", ("ascii",))?.call_method0("strip")?)?;
            }
        }
        let same: PyResult<bool> = (|| {
            let region = builtin.getattr("int")?.call1((values.call_method1("get", ("DY", "-1"))?,))?;
            region.eq(builtin.getattr("int")?.call1((&key,))?)
        })();
        let same = match same { Err(error) if error.is_instance_of::<pyo3::exceptions::PyValueError>(py) => false, other => other? };
        if same && !callback(&scope, "re")?.call_method1("fullmatch", (r"\d{6}", values.call_method1("get", ("GPDM", ""))?))?.is_none() {
            let market = builtin.getattr("int")?.call1((values.call_method1("get", ("SC", "0"))?,))?;
            result.add((market, values.get_item("GPDM")?.ok_or_else(|| pyo3::exceptions::PyKeyError::new_err("GPDM"))?))?;
        }
    }
    Ok(result.into_any().unbind())
}

#[pyfunction]
pub fn board_full_code(py: Python<'_>, market: Bound<'_, PyAny>, code: Bound<'_, PyAny>, mapping: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let market = py.import("builtins")?.getattr("int")?.call1((market,))?;
    let prefix = mapping.get_item(market)?;
    let number = code.str()?.call_method1("zfill", (6,))?;
    Ok(py.import("operator")?.getattr("add")?.call1((prefix, number))?.unbind())
}

#[pyfunction]
pub fn board_snapshot_batches(py: Python<'_>, service: Bound<'_, PyAny>, codes: Bound<'_, PyAny>, size: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let result = PyList::empty(py);
    let builtin = py.import("builtins")?;
    for start in builtin.getattr("range")?.call1((0, codes.len()?, &size))?.try_iter()? {
        py.check_signals()?;
        let start = start?;
        let end = py.import("operator")?.getattr("add")?.call1((&start, &size))?;
        let part = slice(&codes, start, end)?;
        let page = service.getattr("_client")?.getattr("quotes")?.call_method1("get_snapshots", (part,))?;
        if page.is_instance_of::<PyList>() || page.is_instance_of::<PyTuple>() { result.call_method1("extend", (page,))?; }
    }
    Ok(result.into_any().unbind())
}
