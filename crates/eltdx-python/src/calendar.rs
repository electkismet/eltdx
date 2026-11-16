use pyo3::exceptions::{PyOverflowError, PyValueError};
use pyo3::prelude::*;

#[pyfunction]
pub fn calendar_days(days: Vec<u32>) -> Vec<u32> { eltdx::__python_compat::calendar_days(days) }

#[pyfunction]
pub fn calendar_contains(days: Vec<u32>, day: u32) -> bool {
    let sorted = eltdx::__python_compat::calendar_days(days);
    eltdx::__python_compat::calendar_contains(&sorted, &day)
}

#[pyfunction]
pub fn calendar_window(days: Vec<u32>, start: u32, end: u32) -> (usize, usize) {
    eltdx::__python_compat::calendar_window(&days, &start, &end)
}

#[pyfunction]
pub fn calendar_neighbor(days: Vec<u32>, day: u32, include_self: bool, forward: bool) -> Option<usize> {
    eltdx::__python_compat::calendar_neighbor(&days, &day, include_self, forward)
}

#[pyfunction]
pub fn weekday_range(start: u32, end: u32) -> PyResult<Vec<u32>> {
    eltdx::__python_compat::legacy_weekday_range(start, end).map_err(PyOverflowError::new_err)
}

#[pyfunction]
pub fn weekday_neighbor(day: u32, include_self: bool, forward: bool) -> PyResult<u32> {
    eltdx::__python_compat::legacy_weekday_neighbor(day, include_self, forward).map_err(PyOverflowError::new_err)
}

#[pyfunction]
pub fn weekday(day: u32) -> PyResult<bool> {
    if !(1..=3_652_059).contains(&day) { return Err(PyValueError::new_err("ordinal must be >= 1")); }
    Ok(eltdx::__python_compat::legacy_weekday(day))
}
