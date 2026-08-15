#![forbid(unsafe_code)]

mod error;
mod protocol;
mod request;
mod response;
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
    transport::register(module)?;
    Ok(())
}
