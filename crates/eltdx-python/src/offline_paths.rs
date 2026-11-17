//! Python path/object policies over the shared offline selection algorithms.
use pyo3::exceptions::{PyFileNotFoundError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyString};

fn formatted(value: &Bound<'_, PyAny>) -> PyResult<String> {
    value
        .py()
        .import("builtins")?
        .getattr("format")?
        .call1((value, ""))?
        .extract()
}

#[pyfunction]
pub fn offline_detect_home(py: Python<'_>, module: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let env = module
        .getattr("os")?
        .call_method1("getenv", ("TDX_HOME",))?;
    if env.is_truthy()? {
        let candidate = module.getattr("Path")?.call1((env,))?;
        let found =
            eltdx::offline::paths::python_legacy::first_directory([Ok(candidate)], |candidate| {
                candidate.call_method0("is_dir")?.is_truthy()
            })?;
        if let Some(candidate) = found {
            return Ok(candidate.unbind());
        }
    }
    let name = if module.getattr("sys")?.getattr("platform")?.eq("win32")? {
        "_WIN_CANDIDATES"
    } else {
        "_UNIX_CANDIDATES"
    };
    let candidates = module.getattr(name)?;
    let found = eltdx::offline::paths::python_legacy::first_directory(
        candidates.try_iter()?,
        |candidate| {
            py.check_signals()?;
            candidate.call_method0("is_dir")?.is_truthy()
        },
    )?;
    Ok(found.map_or_else(|| py.None(), Bound::unbind))
}

#[pyfunction]
pub fn offline_resolve_vipdoc(
    py: Python<'_>,
    path: Bound<'_, PyAny>,
    module: Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    if !path.is_none() {
        let candidate = module.getattr("Path")?.call1((path,))?;
        if candidate.call_method0("is_dir")?.is_truthy()? {
            return Ok(candidate.unbind());
        }
        return Err(PyFileNotFoundError::new_err(format!(
            "指定的 vipdoc 路径不存在: {}",
            formatted(&candidate)?
        )));
    }
    let home = module.call_method0("detect_tdx_home")?;
    if home.is_none() {
        return Err(PyFileNotFoundError::new_err(
            "无法定位通达信安装目录，请设置 TDX_HOME 环境变量或显式传入 vipdoc 路径",
        ));
    }
    let vipdoc = py
        .import("operator")?
        .getattr("truediv")?
        .call1((home, "vipdoc"))?;
    if !vipdoc.call_method0("is_dir")?.is_truthy()? {
        return Err(PyFileNotFoundError::new_err(format!(
            "vipdoc 目录不存在: {}",
            formatted(&vipdoc)?
        )));
    }
    Ok(vipdoc.unbind())
}

#[pyfunction]
pub fn offline_market_exchange(market: Bound<'_, PyAny>) -> PyResult<&'static str> {
    let exchange =
        eltdx::offline::paths::python_legacy::market_to_exchange_with(|id| market.eq(id))?;
    exchange
        .map(|exchange| exchange.as_str())
        .ok_or_else(|| match formatted(&market) {
            Ok(value) => PyValueError::new_err(format!("不支持的市场代码: {value}")),
            Err(error) => error,
        })
}

#[pyfunction]
pub fn offline_security_type(
    filename: Bound<'_, PyAny>,
    module: Bound<'_, PyAny>,
) -> PyResult<&'static str> {
    let name = module
        .getattr("Path")?
        .call1((filename,))?
        .getattr("name")?
        .call_method0("lower")?;
    let name = name.cast::<PyString>()?.to_str()?;
    use eltdx::offline::SecurityType::*;
    Ok(
        match eltdx::offline::daily::python_legacy::classify_leaf(name) {
            ShA => "SH_A_STOCK",
            ShB => "SH_B_STOCK",
            ShIndex => "SH_INDEX",
            ShFund => "SH_FUND",
            ShBond => "SH_BOND",
            SzA => "SZ_A_STOCK",
            SzB => "SZ_B_STOCK",
            SzIndex => "SZ_INDEX",
            SzFund => "SZ_FUND",
            SzBond => "SZ_BOND",
            Unknown => "UNKNOWN",
        },
    )
}

#[pyfunction]
pub fn offline_security_coefficients(
    py: Python<'_>,
    filename: Bound<'_, PyAny>,
    module: Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let get = module.getattr("_SECURITY_COEFFICIENTS")?.getattr("get")?;
    let kind = module.getattr("detect_security_type")?.call1((filename,))?;
    Ok(get.call1((kind, (0.01, 0.01).into_pyobject(py)?))?.unbind())
}

#[pyfunction]
pub fn offline_find_bar(
    py: Python<'_>,
    market: Bound<'_, PyAny>,
    code: Bound<'_, PyAny>,
    vipdoc: Bound<'_, PyAny>,
    module: Bound<'_, PyAny>,
    folder: &str,
    extension: &str,
) -> PyResult<Py<PyAny>> {
    let root = module.getattr("resolve_vipdoc")?.call1((vipdoc,))?;
    let exchange = module.getattr("market_to_exchange")?.call1((market,))?;
    let divide = py.import("operator")?.getattr("truediv")?;
    let path = divide.call1((root, &exchange))?;
    let path = divide.call1((path, folder))?;
    let leaf = format!("{}{}.{extension}", formatted(&exchange)?, formatted(&code)?);
    Ok(divide.call1((path, leaf))?.unbind())
}

#[pyfunction]
pub fn offline_industry(py: Python<'_>, content: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let result = PyDict::new(py);
    let args = PyDict::new(py);
    args.set_item("errors", "replace")?;
    let text = content.call_method("decode", ("gbk",), Some(&args))?;
    let text = text.cast::<PyString>()?.to_str()?;
    eltdx::offline::industry::python_legacy::visit_rows(
        text,
        |code, industry, sw| result.set_item(code, (industry, sw)),
        &mut || py.check_signals(),
    )?;
    Ok(result.into_any().unbind())
}
