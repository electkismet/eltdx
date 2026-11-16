#![forbid(unsafe_code)]

mod error;
mod corporate;
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
    transport::register(module)?;
    Ok(())
}
