//! Legacy extension connection policy. Python socket methods release the GIL
//! around their real blocking I/O; custom socket objects retain their hooks.
//! Exception/with shells remain Python adapters to preserve active exceptions.
use pyo3::prelude::*;
use pyo3::types::{PyBytes,PySlice,PyTuple};
use crate::f10_request::builtin;

fn make_error<'py>(module: &Bound<'py,PyAny>,name: &str,text: &str) -> PyResult<PyErr> {
    Ok(PyErr::from_value(module.getattr(name)?.call1((text,))?))
}
fn format<'py>(py: Python<'py>,value: &Bound<'py,PyAny>) -> PyResult<Bound<'py,PyAny>> {
    "{}".into_pyobject(py)?.call_method1("format",(value,))
}
#[pyfunction]
pub fn extension_connection_init<'py>(py: Python<'py>,domain: &str,connection: Bound<'py,PyAny>,args: Bound<'py,PyTuple>,module: Bound<'py,PyAny>) -> PyResult<()> {
    let host = args.get_item(0)?;
    connection.setattr("host",if domain=="ex" && host.is_none() {module.getattr("_first_host")?.call0()?} else {host})?;
    connection.setattr("port",args.get_item(1)?)?;
    connection.setattr("timeout",args.get_item(2)?)?;
    if domain=="ex" {
        connection.setattr("mac_ex_mode",args.get_item(3)?)?;
        let setup = args.get_item(4)?;
        let setup = if setup.is_truthy()? {(!args.get_item(3)?.is_truthy()?).into_pyobject(py)?.to_owned().into_any()} else {setup};
        connection.setattr("send_setup",setup)?;
    } else {
        let factory = args.get_item(3)?;
        connection.setattr("_socket_factory",if factory.is_truthy()? {factory} else {module.getattr("_default_socket_factory")?})?;
        connection.setattr("_setup_commands",args.get_item(4)?)?;
    }
    connection.setattr("_sock",py.None())?;
    if domain=="ex" {connection.setattr("_lock",module.getattr("threading")?.getattr("Lock")?.call0()?)?;}
    Ok(())
}
#[pyfunction]
pub fn extension_socket_factory<'py>(family: Bound<'py,PyAny>,kind: Bound<'py,PyAny>,module: Bound<'py,PyAny>) -> PyResult<Py<PyAny>> {
    Ok(module.getattr("socket")?.getattr("socket")?.call1((family,kind))?.unbind())
}
#[pyfunction]
pub fn extension_socket_prepare<'py>(domain: &str,connection: Bound<'py,PyAny>,module: Bound<'py,PyAny>) -> PyResult<Py<PyAny>> {
    let factory = if domain=="ex" {module.getattr("socket")?.getattr("socket")?} else {connection.getattr("_socket_factory")?};
    let family = module.getattr("socket")?.getattr("AF_INET")?;
    let kind = module.getattr("socket")?.getattr("SOCK_STREAM")?;
    let sock = factory.call1((family,kind))?;
    let method = sock.getattr("settimeout")?; method.call1((connection.getattr("timeout")?,))?;
    if domain=="ex" {
        connection.setattr("_sock",&sock)?;
        if let Err(error) = crate::extension_lifecycle::publish(connection.py(),&connection,&sock) {
            // New holder allocation/publication failed after the original
            // socket publication point. Drop the just-created resource only.
            let cleanup = module.getattr("_native_safe_close").and_then(|close| close.call1((&sock,)));
            let _ = connection.setattr("_sock",connection.py().None());
            if let Err(cleanup) = cleanup { error.set_cause(connection.py(),Some(cleanup)); }
            return Err(error);
        }
    }
    Ok(sock.unbind())
}
#[pyfunction]
pub fn extension_socket_connect<'py>(domain: &str,connection: Bound<'py,PyAny>,sock: Bound<'py,PyAny>,module: Bound<'py,PyAny>) -> PyResult<()> {
    let method = sock.getattr("connect")?; method.call1(((connection.getattr("host")?,connection.getattr("port")?),))?;
    if domain=="ex" && connection.getattr("send_setup")?.is_truthy()? {
        sock.getattr("sendall")?.call1((module.getattr("_EX_SETUP_PKG")?,))?;
        let parse = module.getattr("parse_header")?; let recv = connection.getattr("_recv_exact")?;
        let header = parse.call1((recv.call1((module.getattr("HEADER_SIZE")?,))?,))?;
        let recv = connection.getattr("_recv_exact")?; recv.call1((header.getattr("zipsize")?,))?;
    }
    Ok(())
}
#[pyfunction]
pub fn extension_socket_assign<'py>(connection: Bound<'py,PyAny>,sock: Bound<'py,PyAny>) -> PyResult<()> {
    connection.setattr("_sock",&sock)?;
    crate::extension_lifecycle::publish(connection.py(),&connection,&sock)
}
#[pyfunction]
pub fn extension_require_socket<'py>(connection: Bound<'py,PyAny>,module: Bound<'py,PyAny>,assert_only: bool,enabled: bool) -> PyResult<Py<PyAny>> {
    if (!assert_only || enabled) && connection.getattr("_sock")?.is_none() {
        if assert_only {return Err(PyErr::from_value(builtin(&module,"AssertionError")?.call0()?));}
        return Err(make_error(&module,"TransportError","未连接，请先调用 connect()")?);
    }
    Ok(connection.getattr("_sock")?.unbind())
}
#[pyfunction]
pub fn extension_socket_connected(connection: Bound<'_,PyAny>) -> PyResult<bool> {Ok(!connection.getattr("_sock")?.is_none())}

#[pyfunction]
pub fn extension_connection_receive<'py>(domain: &str,connection: Bound<'py,PyAny>,count: Bound<'py,PyAny>,module: Bound<'py,PyAny>,assert_enabled: bool) -> PyResult<Py<PyAny>> {
    let sock = if domain=="ex" {connection.call_method0("_require_sock")?} else {
        if assert_enabled && connection.getattr("_sock")?.is_none() {return Err(PyErr::from_value(builtin(&module,"AssertionError")?.call0()?));}
        let method = module.getattr("_recv_exact")?;
        return Ok(method.call1((connection.getattr("_sock")?,count))?.unbind());
    };
    Ok(module.getattr("_recv_exact_sock")?.call1((sock,count))?.unbind())
}

#[pyfunction]
pub fn extension_connection_close<'py>(py: Python<'py>,domain: &str,connection: Bound<'py,PyAny>,module: Bound<'py,PyAny>) -> PyResult<()> {
    if !connection.getattr("_sock")?.is_none() {
        let close = module.getattr(if domain=="mac" {"_safe_close"} else {"_native_safe_close"})?;
        close.call1((connection.getattr("_sock")?,))?;
        connection.setattr("_sock",py.None())?;
    }
    crate::extension_lifecycle::retire(&connection)?;
    Ok(())
}

#[pyfunction]
pub fn extension_recv_exact<'py>(py: Python<'py>,domain: &str,sock: Bound<'py,PyAny>,target: Bound<'py,PyAny>,module: Bound<'py,PyAny>) -> PyResult<Py<PyAny>> {
    let buffer = builtin(&module,"bytearray")?.call0()?;
    let op = py.import("operator")?;
    let buffer = eltdx::ex::legacy::receive_exact_with(buffer,&target,
        |buffer,target| builtin(&module,"len")?.call1((buffer,))?.lt(target),
        |buffer,target| {
            let recv = sock.getattr("recv")?;
            let remaining = op.getattr("sub")?.call1((target,builtin(&module,"len")?.call1((buffer,))?))?;
            let chunk = recv.call1((remaining,))?;
            if !chunk.is_truthy()? {return Err(make_error(&module,if domain=="mac" {"ConnectionClosedError"} else {"TransportError"},"连接被服务器关闭")?);}
            Ok(chunk)
        },
        |buffer,chunk| {buffer.call_method1("extend",(chunk,))?;Ok(())})?;
    Ok(builtin(&module,"bytes")?.call1((buffer,))?.unbind())
}
#[pyfunction]
pub fn extension_request_prepare<'py>(py: Python<'py>,domain: &str,connection: Bound<'py,PyAny>,command: Bound<'py,PyAny>,module: Bound<'py,PyAny>) -> PyResult<Py<PyAny>> {
    if domain=="mac" && connection.getattr("_sock")?.is_none() {return Err(make_error(&module,"TransportError","未连接，请先调用 connect()")?);}
    let request = command.call_method0("build_request")?;
    if domain=="ex" && connection.getattr("mac_ex_mode")?.is_truthy()? && request.is_truthy()? && request.get_item(0)?.eq(0x1c)? {
        let tail = request.get_item(py.get_type::<PySlice>().call1((1,py.None()))?)?;
        return Ok(py.import("operator")?.getattr("add")?.call1((PyBytes::new(py,b"\x01"),tail))?.unbind());
    }
    Ok(request.unbind())
}
#[pyfunction]
pub fn extension_exchange<'py>(py: Python<'py>,domain: &str,connection: Bound<'py,PyAny>,sock: Bound<'py,PyAny>,request: Bound<'py,PyAny>,module: Bound<'py,PyAny>) -> PyResult<Py<PyAny>> {
    let sock = if domain=="mac" {connection.getattr("_sock")?} else {sock};
    sock.call_method1("sendall",(request,))?;
    let header = if domain=="mac" {
        let recv = connection.getattr("_recv_exact")?;
        let bytes = recv.call1((module.getattr("HEADER_SIZE")?,))?;
        module.getattr("parse_mac_header")?.call1((bytes,))?
    } else {
        let parse = module.getattr("parse_header")?;
        let recv = connection.getattr("_recv_exact")?;
        parse.call1((recv.call1((module.getattr("HEADER_SIZE")?,))?,))?
    };
    let recv = connection.getattr("_recv_exact")?; let raw = recv.call1((header.getattr("zipsize")?,))?;
    Ok((header,raw).into_pyobject(py)?.into_any().unbind())
}
#[pyfunction]
pub fn extension_response_finish<'py>(domain: &str,command: Bound<'py,PyAny>,header: Bound<'py,PyAny>,raw: Bound<'py,PyAny>,module: Bound<'py,PyAny>) -> PyResult<Py<PyAny>> {
    let method = module.getattr(if domain=="mac" {"decompress_mac_body"} else {"decompress_body"})?;
    let body = method.call1((header,raw))?;
    Ok(command.call_method1("parse_response",(body,))?.unbind())
}
#[pyfunction]
pub fn extension_transport_error<'py>(py: Python<'py>,kind: &str,connection: Bound<'py,PyAny>,cause: Bound<'py,PyAny>,module: Bound<'py,PyAny>) -> PyResult<Py<PyAny>> {
    let (name,prefix) = match kind {
        "mac_connect" => ("TransportError","无法连接 "),"ex_connect" => ("TransportError","扩展行情连接/握手失败 "),
        "mac_timeout" => ("ResponseTimeoutError","请求超时: "),"mac_io" => ("ConnectionClosedError","通信错误: "),
        _ => ("TransportError","通信错误: "),
    };
    let constructor = module.getattr(name)?;
    let text = if kind.ends_with("_connect") {
        let host = format(py,&connection.getattr("host")?)?; let port = format(py,&connection.getattr("port")?)?; let cause = format(py,&cause)?;
        "{}{}:{}: {}".into_pyobject(py)?.call_method1("format",(prefix,host,port,cause))?
    } else {"{}{}".into_pyobject(py)?.call_method1("format",(prefix,format(py,&cause)?))?};
    Ok(constructor.call1((text,))?.unbind())
}
#[pyfunction]
pub fn mac_setup_commands<'py>(py: Python<'py>,connection: Bound<'py,PyAny>,module: Bound<'py,PyAny>,assert_enabled: bool) -> PyResult<()> {
    for command in connection.getattr("_setup_commands")?.try_iter()? {
        let command = command?; py.check_signals()?;
        if assert_enabled && connection.getattr("_sock")?.is_none() {return Err(PyErr::from_value(builtin(&module,"AssertionError")?.call0()?));}
        connection.getattr("_sock")?.call_method1("sendall",(command,))?;
        module.getattr("_native_setup_response")?.call1((&connection,))?;
    }
    Ok(())
}
#[pyfunction]
pub fn mac_setup_response<'py>(connection: Bound<'py,PyAny>,module: Bound<'py,PyAny>) -> PyResult<()> {
    let method = connection.getattr("_recv_exact")?; let bytes = method.call1((module.getattr("HEADER_SIZE")?,))?;
    let header = module.getattr("parse_mac_header")?.call1((bytes,))?;
    if header.getattr("zipsize")?.gt(0)? {let method = connection.getattr("_recv_exact")?; method.call1((header.getattr("zipsize")?,))?;}
    Ok(())
}
