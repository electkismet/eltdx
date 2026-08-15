//! Stateless protocol functions used by the public Python compatibility facade.

use eltdx_protocol::frame::{
    decode_response as decode_frame, RequestFrame as NativeRequestFrame, ResponseFrame,
};
use eltdx_protocol::{CommandResponse, ProtocolError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyTuple};

use crate::{error, request, response};

fn protocol_error(error_value: ProtocolError) -> PyErr {
    error::from_runtime(error_value.into())
}

fn frame_tuple<'py>(py: Python<'py>, frame: &ResponseFrame) -> PyResult<Py<PyAny>> {
    Ok(PyTuple::new(
        py,
        [
            frame.control.into_pyobject(py)?.into_any(),
            frame.msg_id.into_pyobject(py)?.into_any(),
            frame.msg_type.into_pyobject(py)?.into_any(),
            frame.zip_length.into_pyobject(py)?.into_any(),
            frame.length.into_pyobject(py)?.into_any(),
            PyBytes::new(py, &frame.data).into_any(),
            PyBytes::new(py, &frame.raw).into_any(),
            frame.response_header_reserved.into_pyobject(py)?.into_any(),
        ],
    )?
    .into_any()
    .unbind())
}

#[pyfunction]
#[pyo3(signature = (msg_id, msg_type, data, control=1))]
pub fn encode_request_frame(
    py: Python<'_>,
    msg_id: u32,
    msg_type: u16,
    data: &[u8],
    control: u8,
) -> PyResult<Py<PyAny>> {
    let frame = NativeRequestFrame::with_control(msg_id, msg_type, data.to_vec(), control);
    let encoded = frame.encode().map_err(protocol_error)?;
    Ok(PyBytes::new(py, &encoded).into_any().unbind())
}

#[pyfunction]
#[pyo3(signature = (command, payload, msg_id))]
pub fn build_command_frame(
    py: Python<'_>,
    command: u16,
    payload: Option<&Bound<'_, PyDict>>,
    msg_id: u32,
) -> PyResult<Py<PyAny>> {
    let request = request::from_python(py, command, payload)?;
    let frame = request.frame(msg_id).map_err(protocol_error)?;
    let _bytes = frame.encode().map_err(protocol_error)?;
    Ok(PyTuple::new(
        py,
        [
            frame.msg_id.into_pyobject(py)?.into_any(),
            frame.msg_type.into_pyobject(py)?.into_any(),
            PyBytes::new(py, &frame.data).into_any(),
            frame.control.into_pyobject(py)?.into_any(),
        ],
    )?
    .into_any()
    .unbind())
}

#[pyfunction]
#[pyo3(signature = (raw, max_payload_size=65535))]
pub fn decode_response(py: Python<'_>, raw: &[u8], max_payload_size: u16) -> PyResult<Py<PyAny>> {
    let frame = decode_frame(raw, usize::from(max_payload_size)).map_err(protocol_error)?;
    frame_tuple(py, &frame)
}

#[pyfunction]
#[pyo3(signature = (command, response, payload=None))]
pub fn parse_command_response(
    py: Python<'_>,
    command: u16,
    response: &[u8],
    payload: Option<&Bound<'_, PyDict>>,
) -> PyResult<Py<PyAny>> {
    let request = request::from_python(py, command, payload)?;
    let frame = decode_frame(response, 0xFFFF).map_err(protocol_error)?;
    let parsed = CommandResponse::parse(request, &frame.data).map_err(protocol_error)?;
    response::to_python(py, parsed)
}
