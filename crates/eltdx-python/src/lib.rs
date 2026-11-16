#![forbid(unsafe_code)]

mod error;
mod corporate;
mod calendar;
mod resource;
mod protocol;
mod request;
mod response;
mod service;
mod transport;

use pyo3::prelude::*;

pub const ABI_VERSION: u32 = 1;

#[pymodule]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add("ABI_VERSION", ABI_VERSION)?;
    error::register(module)?;
    module.add_function(wrap_pyfunction!(protocol::build_command_frame, module)?)?;
    module.add_function(wrap_pyfunction!(protocol::decode_response, module)?)?;
    module.add_function(wrap_pyfunction!(protocol::encode_request_frame, module)?)?;
    module.add_function(wrap_pyfunction!(protocol::parse_command_response, module)?)?;
    module.add_function(wrap_pyfunction!(service::select_security_rows, module)?)?;
    module.add_function(wrap_pyfunction!(service::recover_money_flow, module)?)?;
    module.add_function(wrap_pyfunction!(corporate::collect_capital, module)?)?;
    module.add_function(wrap_pyfunction!(corporate::recover_finance, module)?)?;
    module.add_function(wrap_pyfunction!(corporate::assemble_finance, module)?)?;
    module.add_function(wrap_pyfunction!(corporate::project_finance_fields, module)?)?;
    module.add_function(wrap_pyfunction!(corporate::adjustment_factors, module)?)?;
    module.add_function(wrap_pyfunction!(calendar::calendar_window, module)?)?;
    module.add_function(wrap_pyfunction!(calendar::calendar_days, module)?)?;
    module.add_function(wrap_pyfunction!(calendar::calendar_contains, module)?)?;
    module.add_function(wrap_pyfunction!(calendar::calendar_neighbor, module)?)?;
    module.add_function(wrap_pyfunction!(calendar::weekday_range, module)?)?;
    module.add_function(wrap_pyfunction!(calendar::weekday_neighbor, module)?)?;
    module.add_function(wrap_pyfunction!(calendar::weekday, module)?)?;
    module.add_function(wrap_pyfunction!(resource::download_resource, module)?)?;
    transport::register(module)?;
    Ok(())
}
