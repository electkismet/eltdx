use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use eltdx_runtime::execution::AttemptAuthorization;
use http_body_util::{BodyExt, Full};
use hyper::client::conn::http1;
use hyper::{Request, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use url::Url;

use super::control::{connection, Control};
use super::io::{BoxStream, Decoder, ObservedStream, WireCapture};
use super::{F10Config, TlsPolicy};
use crate::{Error, ErrorKind, Result, TimeoutPhase};

pub(crate) struct WireReply {
    pub body: Vec<u8>, pub encoded: Vec<u8>, pub wire: Vec<u8>, pub endpoint: SocketAddr, pub url: String,
}

fn header_error(message: &str) -> Error { Error::new(ErrorKind::Protocol, message) }

fn host_name(url: &Url) -> Result<String> {
    match url.host() {
        Some(url::Host::Domain(host)) => Ok(host.to_owned()),
        Some(url::Host::Ipv4(address)) => Ok(address.to_string()),
        Some(url::Host::Ipv6(address)) => Ok(address.to_string()),
        None => Err(header_error("HTTP endpoint has no host")),
    }
}

/// One physical connection attempt consumes the existing shared authorization.
async fn open(config: &F10Config, target: &Url, authorization: &Arc<dyn AttemptAuthorization>, attempt: &mut u32,
    control: &Control) -> Result<(BoxStream, SocketAddr)> {
    let via = config.proxy.as_ref().map_or(target, |proxy| proxy.url());
    let host = host_name(via)?;
    let port = via.port_or_known_default().ok_or_else(|| header_error("HTTP endpoint has no port"))?;
    let addresses = super::resolver::addresses(&host, port, config.prefer_ipv4, &config.resolver, control).await?;
    let mut failure: Option<Error> = None;
    for address in addresses {
        control.check(TimeoutPhase::Connect)?;
        *attempt = attempt.checked_add(1).ok_or_else(|| header_error("connection attempt overflow"))?;
        let previous = failure.clone().map(crate::policy::retries::to_runtime);
        let reserved = authorization.reserve(*attempt, previous.as_ref()).map_err(|error| {
            let error = Error::from_runtime(error);
            match &failure { Some(previous) => error.with_source(previous.clone()), None => error }
        })?;
        reserved.start_before(control.deadline).map_err(Error::from_runtime)?;
        match control.connect_wait(async { TcpStream::connect(address).await.map_err(|error| connection(error, "F10 TCP connection failed")) }).await {
            Ok(stream) => {
                let _ = stream.set_nodelay(true);
                let mut stream: BoxStream = Box::new(stream);
                if via.scheme() == "https" { stream = tls(stream, &host, &config.tls, control).await?; }
                if config.proxy.is_some() && target.scheme() == "https" {
                    connect_tunnel(&mut stream, target, config.max_header_bytes.get(), control).await?;
                }
                if target.scheme() == "https" && config.proxy.is_some() {
                    stream = tls(stream, &host_name(target)?, &config.tls, control).await?;
                }
                return Ok((stream, address));
            }
            Err(error) if error.kind() == ErrorKind::Connection => {
                failure = Some(error.with_context("endpoint", address.to_string()));
            }
            Err(error) => return Err(error),
        }
    }
    Err(failure.unwrap_or_else(|| Error::new(ErrorKind::Connection, "F10 has no connectable address")))
}

async fn tls(stream: BoxStream, host: &str, policy: &TlsPolicy, control: &Control) -> Result<BoxStream> {
    control.check(TimeoutPhase::Handshake)?;
    let mut roots = rustls::RootCertStore::empty();
    if matches!(policy, TlsPolicy::SystemRoots | TlsPolicy::SystemAndCustomRoots(_)) {
        let native = rustls_native_certs::load_native_certs();
        if !native.errors.is_empty() || native.certs.is_empty() {
            return Err(Error::new(ErrorKind::Connection, "cannot load complete system TLS trust roots"));
        }
        for certificate in native.certs {
            roots.add(certificate).map_err(|error| connection(error, "invalid system TLS certificate"))?;
        }
    }
    if let TlsPolicy::CustomRoots(certificates) | TlsPolicy::SystemAndCustomRoots(certificates) = policy {
        if certificates.is_empty() && matches!(policy, TlsPolicy::CustomRoots(_)) {
            return Err(Error::new(ErrorKind::Connection, "custom TLS trust store is empty"));
        }
        for certificate in certificates {
            control.check(TimeoutPhase::Handshake)?;
            roots.add(rustls::pki_types::CertificateDer::from(certificate.clone()))
                .map_err(|error| connection(error, "invalid custom TLS certificate"))?;
        }
    }
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions().map_err(|error| connection(error, "cannot configure TLS protocol versions"))?
        .with_root_certificates(roots).with_no_client_auth();
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    let name = rustls::pki_types::ServerName::try_from(host.to_owned())
        .map_err(|error| connection(error, "invalid TLS server name"))?;
    let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
    let secured = control.wait(TimeoutPhase::Handshake, async {
        connector.connect(name, stream).await.map_err(|error| connection(error, "F10 TLS handshake or certificate validation failed"))
    }).await?;
    Ok(Box::new(secured))
}

fn authority(url: &Url) -> Result<String> {
    let host = url.host_str().ok_or_else(|| header_error("missing HTTP host"))?;
    let host = if host.contains(':') && !host.starts_with('[') { format!("[{host}]") } else { host.to_owned() };
    Ok(match url.port() { Some(port) => format!("{host}:{port}"), None => host })
}

async fn connect_tunnel(stream: &mut BoxStream, target: &Url, cap: usize, control: &Control) -> Result<()> {
    let host = authority(target)?;
    let host = if target.port().is_none() { format!("{host}:443") } else { host };
    let request = format!("CONNECT {host} HTTP/1.1\r\nHost: {host}\r\n\r\n");
    control.wait(TimeoutPhase::Send, async { stream.write_all(request.as_bytes()).await.map_err(|error| connection(error, "proxy CONNECT write failed")) }).await?;
    let mut bytes = Vec::new();
    // Read through exactly CRLF CRLF, leaving TLS bytes in the socket.
    while !bytes.ends_with(b"\r\n\r\n") {
        if bytes.len() == cap { return Err(Error::new(ErrorKind::LimitExceeded, "proxy CONNECT headers exceed limit")); }
        let mut byte = [0_u8];
        control.wait(TimeoutPhase::Response, async { stream.read_exact(&mut byte).await.map(|_| ()).map_err(|error| connection(error, "proxy CONNECT response truncated")) }).await?;
        bytes.try_reserve(1).map_err(|error| Error::new(ErrorKind::LimitExceeded, "cannot reserve proxy headers").with_source(error))?;
        bytes.push(byte[0]);
    }
    let line = bytes.split(|byte| *byte == b'\n').next().unwrap_or_default();
    let status = std::str::from_utf8(line).map_err(|error| header_error("invalid CONNECT status").with_source(error))?;
    let mut fields = status.split_ascii_whitespace();
    if !matches!(fields.next(), Some("HTTP/1.0" | "HTTP/1.1")) || fields.next() != Some("200") {
        return Err(Error::new(ErrorKind::Connection, "proxy CONNECT did not return HTTP 200"));
    }
    Ok(())
}

pub(crate) async fn connect(config: &F10Config, target: &Url, authorization: Arc<dyn AttemptAuthorization>,
    control: &Control) -> Result<WireReply> {
    let (stream, endpoint) = open(config, target, &authorization, &mut 0, control).await?;
    drop(stream);
    Ok(WireReply { body: Vec::new(), encoded: Vec::new(), wire: Vec::new(), endpoint, url: target.as_str().to_owned() })
}

pub(crate) async fn post(config: &F10Config, initial: Url, body: Bytes,
    authorization: Arc<dyn AttemptAuthorization>, control: &Control, retain_payload: bool, retain_wire: bool) -> Result<WireReply> {
    let mut url = initial;
    let mut attempt = 0_u32;
    for redirects in 0..=config.max_redirects {
        let reply = post_once(config, &url, body.clone(), Arc::clone(&authorization), &mut attempt, control, retain_payload, retain_wire).await?;
        match reply {
            HttpResult::Body(reply) => return Ok(reply),
            HttpResult::Redirect(status, location) => {
                if redirects == config.max_redirects { return Err(Error::new(ErrorKind::Protocol, "F10 redirect limit reached").with_context("http_status", status.as_u16().to_string())); }
                // Preserve POST semantics only for redirects which explicitly
                // preserve the method/body. Never silently turn a query into GET.
                if !matches!(status, StatusCode::TEMPORARY_REDIRECT | StatusCode::PERMANENT_REDIRECT) {
                    return Err(header_error("F10 redirect does not preserve POST semantics"));
                }
                let next = url.join(&location).map_err(|error| header_error("invalid F10 redirect URL").with_source(error))?;
                if !matches!(next.scheme(), "http" | "https") || !next.username().is_empty() || next.password().is_some()
                    || next.fragment().is_some() || (url.scheme() == "https" && next.scheme() != "https") {
                    return Err(header_error("unsafe F10 redirect URL"));
                }
                if next.origin() != url.origin() { return Err(header_error("cross-origin F10 redirect is not permitted")); }
                url = next;
            }
        }
    }
    Err(header_error("unreachable F10 redirect state"))
}

enum HttpResult { Body(WireReply), Redirect(StatusCode, String) }

async fn post_once(config: &F10Config, url: &Url, body: Bytes,
    authorization: Arc<dyn AttemptAuthorization>, attempt: &mut u32, control: &Control, retain_payload: bool, retain_wire: bool) -> Result<HttpResult> {
    let uri = if config.proxy.is_some() && url.scheme() == "http" { url.as_str().to_owned() }
        else { let mut value = url.path().to_owned(); if let Some(query) = url.query() { value.push('?'); value.push_str(query); } value };
    let request_headers = ordered_headers(config, &uri, url, body.len())?;
    let (stream, endpoint) = open(config, url, &authorization, attempt, control).await?;
    let overhead = config.max_header_bytes.get().checked_mul(18).and_then(|value| value.checked_add(64 * 1024))
        .ok_or_else(|| Error::new(ErrorKind::LimitExceeded, "HTTP framing budget overflow"))?;
    let cap = usize::try_from(config.max_encoded_bytes.get()).ok().and_then(|value| value.checked_add(overhead))
        .ok_or_else(|| Error::new(ErrorKind::LimitExceeded, "HTTP wire budget overflow"))?;
    let capture = WireCapture::new(cap, config.max_header_bytes.get(), retain_wire, control.clone());
    let stream = ObservedStream { stream, capture: Arc::clone(&capture), control: control.clone(),
        headers: Some(request_headers), generated: Vec::new(), pending: Vec::new(), written: 0 };
    let mut builder = http1::Builder::new();
    builder.max_header_size(config.max_header_bytes.get()).max_headers(1024).max_buf_size(config.max_header_bytes.get().max(8192)).writev(false);
    let (mut sender, connection) = control.wait(TimeoutPhase::Handshake, async {
        builder.handshake::<_, Full<Bytes>>(TokioIo::new(stream)).await
            .map_err(|error| map_http(error, &capture))
    }).await?;
    let driver = tokio::spawn(connection);
    let result = async {
        let mut request = Request::builder().method("POST").uri(uri);
        // Hyper owns framing. These headers cannot conflict with a fixed body
        // length or turn a verified request into another host's message.
        for header in &config.headers {
            if ["Host", "Content-Length", "Transfer-Encoding", "Connection"].iter().any(|name| header.name().eq_ignore_ascii_case(name)) {
                return Err(Error::new(ErrorKind::InvalidArgument, "F10 framing headers are transport-owned").with_context("header", header.name()));
            }
            request = request.header(header.name(), header.value());
        }
        request = request.header("Host", authority(url)?).header("Content-Length", body.len()).header("Connection", "close");
        if !config.headers.iter().any(|header| header.name().eq_ignore_ascii_case("Accept-Encoding")) {
            request = request.header("Accept-Encoding", "gzip, deflate");
        }
        let request = request.body(Full::new(body)).map_err(|error| Error::new(ErrorKind::InvalidArgument, "invalid HTTP request headers").with_source(error))?;
        let response = control.wait(TimeoutPhase::Response, async { sender.send_request(request).await.map_err(|error| map_http(error, &capture)) }).await?;
        let status = response.status();
        if status.is_redirection() {
            let location = response.headers().get("location").and_then(|value| value.to_str().ok())
                .ok_or_else(|| header_error("F10 redirect has no valid Location"))?.to_owned();
            return Ok(HttpResult::Redirect(status, location));
        }
        if !status.is_success() {
            return Err(Error::new(ErrorKind::Connection, "F10 HTTP status failure").with_context("http_status", status.as_u16().to_string()));
        }
        let encodings = response.headers().get_all("content-encoding");
        let mut encodings = encodings.iter();
        let encoding = encodings.next().map(|value| value.to_str()).transpose()
            .map_err(|error| header_error("invalid Content-Encoding").with_source(error))?.unwrap_or("");
        if encodings.next().is_some() {
            return Err(header_error("multiple F10 Content-Encoding headers are not supported"));
        }
        if let Some(length) = response.headers().get("content-length") {
            let length: u64 = length.to_str().ok().and_then(|value| value.parse().ok()).ok_or_else(|| header_error("invalid Content-Length"))?;
            if length > config.max_encoded_bytes.get() { return Err(Error::new(ErrorKind::LimitExceeded, "F10 encoded response exceeds limit")); }
        }
        let mut decoder = Decoder::new(encoding, config.max_response_bytes.get() as usize, control.clone())?;
        let mut encoded = 0_u64;
        let mut retained = Vec::new();
        let mut incoming = response.into_body();
        loop {
            let frame = control.wait(TimeoutPhase::Response, async { incoming.frame().await.transpose().map_err(|error| map_http(error, &capture)) }).await?;
            let Some(frame) = frame else { break; };
            if let Ok(data) = frame.into_data() {
                encoded = encoded.checked_add(data.len() as u64).ok_or_else(|| Error::new(ErrorKind::LimitExceeded, "encoded byte counter overflow"))?;
                if encoded > config.max_encoded_bytes.get() { return Err(Error::new(ErrorKind::LimitExceeded, "F10 encoded response exceeds limit")); }
                if retain_payload {
                    control.sponsor.record_bytes(data.len())?;
                    retained.try_reserve(data.len()).map_err(|error| Error::new(ErrorKind::LimitExceeded, "cannot reserve encoded HTTP payload").with_source(error))?;
                    retained.extend_from_slice(&data);
                }
                for bytes in data.chunks(4096) { control.check(TimeoutPhase::Decode)?; decoder.write(bytes)?; }
            }
        }
        let body = decoder.finish()?;
        let wire = capture.lock().map_err(|_| Error::new(ErrorKind::Internal, "F10 capture lock poisoned"))?.take_bytes();
        Ok(HttpResult::Body(WireReply { body, encoded: retained, wire, endpoint, url: url.as_str().to_owned() }))
    }.await;
    // The driver owns the socket. Abort plus await confirms its release before
    // returning even when decoding, cancellation or status validation failed.
    drop(sender);
    driver.abort();
    let _ = driver.await;
    result
}

fn ordered_headers(config: &F10Config, uri: &str, url: &Url, length: usize) -> Result<Vec<u8>> {
    let mut output = format!("POST {uri} HTTP/1.1\r\nHost: {}\r\n", authority(url)?);
    for header in &config.headers {
        output.push_str(header.name()); output.push_str(": "); output.push_str(header.value()); output.push_str("\r\n");
    }
    if !config.headers.iter().any(|header| header.name().eq_ignore_ascii_case("Accept-Encoding")) {
        output.push_str("Accept-Encoding: gzip, deflate\r\n");
    }
    output.push_str(&format!("Content-Length: {length}\r\nConnection: close\r\n\r\n"));
    if output.len() > config.max_header_bytes.get() { return Err(Error::new(ErrorKind::LimitExceeded, "F10 request headers exceed limit")); }
    Ok(output.into_bytes())
}

fn map_http(error: hyper::Error, capture: &Mutex<WireCapture>) -> Error {
    if let Ok(capture) = capture.lock() { if let Some(failure) = capture.failure() { return failure; } }
    Error::new(ErrorKind::Protocol, "invalid or truncated F10 HTTP response").with_source(error)
}
