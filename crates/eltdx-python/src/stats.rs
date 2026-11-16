use pyo3::prelude::*;

#[pyfunction]
pub fn stats_members(py: Python<'_>, stat: Vec<u8>, stat2: Vec<u8>)
    -> PyResult<(Vec<eltdx::__python_compat::LegacyStatRow>, Vec<eltdx::__python_compat::LegacyStat2Row>)> {
    let mut interrupted = None;
    let result = eltdx::__python_compat::legacy_stats_members(&stat, &stat2, &mut || {
        py.check_signals().map_err(|error| {
            interrupted = Some(error);
            eltdx::Error::new(eltdx::ErrorKind::Cancelled, "statistics parsing interrupted")
        })
    });
    if let Some(error) = interrupted { return Err(error); }
    result.map_err(|error| crate::error::from_sdk(py, error))
}
