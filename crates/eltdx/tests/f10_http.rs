//! Authored public F10 transport cases. Run only at the final validation gate.
#![cfg(feature = "f10")]

use std::error::Error as StdError;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::num::{NonZeroU64, NonZeroUsize};
use std::sync::{mpsc, Arc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use eltdx::f10::{
    CallQuery, EntryName, F10Client, F10ClientBuilder, JsonValue, ProxyUrl, RequestPayload,
    ResolverPolicy, TlsPolicy,
};
use eltdx::{CancellationToken, ErrorKind, OperationOptions, RawPolicy};

type TestResult = Result<(), Box<dyn StdError + Send + Sync>>;
const TABLE: &[u8] = br#"{"ErrorCode":0,"ResultSets":[{"ResultSetKey":"table","ColName":["price","name"],"Content":[[12.50,"TEST"]]}]}"#;

struct Server {
    address: SocketAddr,
    thread: JoinHandle<TestResult>,
}
impl Server {
    fn start(
        handler: impl FnOnce(TcpStream) -> TestResult + Send + 'static,
    ) -> Result<Self, Box<dyn StdError + Send + Sync>> {
        Self::at("127.0.0.1:0", handler)
    }
    fn at(
        address: &str,
        handler: impl FnOnce(TcpStream) -> TestResult + Send + 'static,
    ) -> Result<Self, Box<dyn StdError + Send + Sync>> {
        let listener = TcpListener::bind(address)?;
        let address = listener.local_addr()?;
        listener.set_nonblocking(true)?;
        let thread = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                match listener.accept() {
                    Ok((stream, _)) => {
                        stream.set_nonblocking(false)?;
                        stream.set_read_timeout(Some(Duration::from_secs(3)))?;
                        stream.set_write_timeout(Some(Duration::from_secs(3)))?;
                        return handler(stream);
                    }
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(2))
                    }
                    Err(error) => return Err(Box::new(error)),
                }
            }
        });
        Ok(Self { address, thread })
    }
    fn url(&self) -> String {
        format!("http://{}/TQLEX", self.address)
    }
    fn finish(self) -> TestResult {
        self.thread.join().map_err(|_| "HTTP fixture panicked")?
    }
}

fn read_request(
    stream: &mut impl Read,
) -> Result<(String, Vec<u8>), Box<dyn StdError + Send + Sync>> {
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") {
        let mut byte = [0_u8];
        stream.read_exact(&mut byte)?;
        header.push(byte[0]);
        if header.len() > 64 * 1024 {
            return Err("request header exceeded fixture cap".into());
        }
    }
    let header = String::from_utf8(header)?;
    let count = header
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .map(|(_, value)| value.trim().parse::<usize>())
        .transpose()?
        .unwrap_or(0);
    let mut body = vec![0_u8; count];
    stream.read_exact(&mut body)?;
    Ok((header, body))
}
fn answer(stream: &mut impl Write, status: &str, extra: &str, body: &[u8]) -> TestResult {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n",
        body.len()
    )?;
    stream.write_all(body)?;
    stream.flush()?;
    Ok(())
}
fn eof(stream: &mut impl Read) -> TestResult {
    let mut byte = [0_u8];
    assert_eq!(
        stream.read(&mut byte)?,
        0,
        "client must retire its connection"
    );
    Ok(())
}
fn query(entry: &str) -> eltdx::Result<CallQuery> {
    Ok(CallQuery::new(
        EntryName::new(entry)?,
        RequestPayload::Params(vec![JsonValue::from("中文"), JsonValue::from(7_i64)]),
    ))
}
fn client(url: String) -> eltdx::Result<F10Client> {
    F10Client::builder().with_base_url(url).build()
}

#[test]
fn public_http_call_preserves_unicode_numbers_tables_and_actual_raw() -> TestResult {
    let server = Server::start(|mut stream| {
        let (header, body) = read_request(&mut stream)?;
        assert!(header.starts_with("POST /TQLEX?Entry=CWServ.sample HTTP/1.1\r\n"));
        assert!(!header.contains("QSID"));
        assert_eq!(body, "{\"Params\":[\"中文\",7]}".as_bytes());
        answer(&mut stream, "200 OK", "", TABLE)?;
        eof(&mut stream)
    })?;
    let client = client(server.url())?;
    let response = client.call(
        &query("CWServ.sample")?,
        &OperationOptions::default().with_raw(RawPolicy::RetainWire),
    )?;
    assert!(response.ok());
    let table = response.first_table().ok_or("missing result table")?;
    assert_eq!(
        table.rows()[0].cells()[0]
            .value()
            .as_number()
            .map(|number| number.lexeme()),
        Some("12.50")
    );
    let raw = response.meta().raw().ok_or("missing raw")?;
    assert_eq!(raw.payloads()[0].as_ref(), TABLE);
    assert!(raw.wire_frames().ok_or("missing wire")?[0].starts_with(b"HTTP/1.1 200 OK\r\n"));
    assert!(response.meta().source().endpoint().is_some());
    assert_eq!(client.diagnostics()?.completed_requests(), 1);
    client.close()?;
    server.finish()
}

#[test]
fn public_route_overrides_are_independent_and_business_failure_is_normal_response() -> TestResult {
    for entry in [
        "CWServ.cfg_fx_lbtt",
        "CWServ.skef10_hk_info",
        "CWServ.qhf9_info",
    ] {
        let server = Server::start(|mut stream| {
            read_request(&mut stream)?;
            answer(
                &mut stream,
                "200 OK",
                "",
                br#"{"ErrorCode":19,"ResultSets":[]}"#,
            )?;
            eof(&mut stream)
        })?;
        let client = F10Client::builder()
            .with_base_url("http://127.0.0.1:1/unused")
            .with_ladder_url(server.url())
            .with_international_url(server.url())
            .build()?;
        let response = client.call(&query(entry)?, &OperationOptions::default())?;
        assert_eq!(response.error_code(), Some(19));
        assert!(!response.ok());
        assert!(response.meta().raw().is_none());
        client.close()?;
        server.finish()?;
    }
    Ok(())
}

#[test]
fn public_http_rejects_bad_json_nonobject_status_and_truncated_bodies() -> TestResult {
    for (status, content, expected) in [
        ("200 OK", &b"{broken"[..], ErrorKind::Protocol),
        ("200 OK", &b"[]"[..], ErrorKind::Protocol),
        ("200 OK", &b"\xff"[..], ErrorKind::Protocol),
        ("503 Unavailable", &b"failure"[..], ErrorKind::Connection),
        ("302 Found", &b""[..], ErrorKind::Protocol),
    ] {
        let content = content.to_vec();
        let server = Server::start(move |mut stream| {
            read_request(&mut stream)?;
            answer(
                &mut stream,
                status,
                if status.starts_with("302") {
                    "Location: /next\r\n"
                } else {
                    ""
                },
                &content,
            )?;
            Ok(())
        })?;
        let client = client(server.url())?;
        let error = client
            .call(&query("sample")?, &OperationOptions::default())
            .unwrap_err();
        assert_eq!(error.kind(), expected);
        assert!(error
            .context()
            .iter()
            .any(|(key, value)| key == "domain" && value == "f10"));
        client.close()?;
        server.finish()?;
    }
    let server = Server::start(|mut stream| {
        read_request(&mut stream)?;
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 20\r\n\r\n{}")?;
        Ok(())
    })?;
    let client = client(server.url())?;
    assert_eq!(
        client
            .call(&query("sample")?, &OperationOptions::default())
            .unwrap_err()
            .kind(),
        ErrorKind::Protocol
    );
    client.close()?;
    server.finish()
}

#[test]
fn partial_headers_chunked_bodies_bom_and_trailers_are_supported() -> TestResult {
    let server = Server::start(|mut stream| {
        read_request(&mut stream)?;
        for part in [
            &b"HTTP/1.1 200 O"[..],
            &b"K\r\nTransfer-Encoding: chunked\r\n"[..],
            &b"Connection: close\r\n\r\n"[..],
        ] {
            stream.write_all(part)?;
        }
        stream.write_all(b"3\r\n\xef\xbb\xbf\r\n")?;
        write!(stream, "{:x}\r\n", TABLE.len())?;
        stream.write_all(TABLE)?;
        stream.write_all(b"\r\n0\r\nX-Trailer: yes\r\n\r\n")?;
        eof(&mut stream)
    })?;
    let client = client(server.url())?;
    assert!(client
        .call(&query("sample")?, &OperationOptions::default())?
        .ok());
    client.close()?;
    server.finish()
}

#[test]
fn compressed_payload_retention_differs_from_decoded_json_and_enforces_expansion_cap() -> TestResult
{
    use flate2::{write::GzEncoder, Compression};
    for limited in [false, true] {
        let mut compressor = GzEncoder::new(Vec::new(), Compression::default());
        compressor.write_all(TABLE)?;
        let compressed = compressor.finish()?;
        let expected_raw = compressed.clone();
        let server = Server::start(move |mut stream| {
            read_request(&mut stream)?;
            answer(
                &mut stream,
                "200 OK",
                "Content-Encoding: gzip\r\n",
                &compressed,
            )?;
            Ok(())
        })?;
        let builder = F10Client::builder().with_base_url(server.url());
        let client = if limited {
            builder
                .with_max_response_bytes(NonZeroU64::new(12).ok_or("nonzero")?)
                .build()?
        } else {
            builder.build()?
        };
        let result = client.call(
            &query("sample")?,
            &OperationOptions::default().with_raw(RawPolicy::RetainPayload),
        );
        if limited {
            assert_eq!(result.unwrap_err().kind(), ErrorKind::LimitExceeded);
        } else {
            assert_eq!(
                result?.meta().raw().ok_or("missing raw")?.payloads()[0].as_ref(),
                expected_raw
            );
        }
        client.close()?;
        server.finish()?;
    }
    Ok(())
}

#[test]
fn operation_bytes_include_retained_copies_before_the_response_is_accepted() -> TestResult {
    for raw in [
        RawPolicy::Omit,
        RawPolicy::RetainPayload,
        RawPolicy::RetainWire,
    ] {
        let server = Server::start(|mut stream| {
            read_request(&mut stream)?;
            answer(&mut stream, "200 OK", "", TABLE)
        })?;
        let client = F10Client::builder()
            .with_base_url(server.url())
            .with_max_operation_bytes(NonZeroU64::new(TABLE.len() as u64).ok_or("nonzero")?)
            .build()?;
        let result = client.call(
            &query("sample")?,
            &OperationOptions::default().with_raw(raw),
        );
        if raw == RawPolicy::Omit {
            assert!(result?.ok());
        } else {
            assert_eq!(result.unwrap_err().kind(), ErrorKind::LimitExceeded);
        }
        client.close()?;
        server.finish()?;
    }
    Ok(())
}

#[test]
fn report_prerequisite_retains_both_raw_sources_and_shares_streamed_byte_limit() -> TestResult {
    use eltdx::f10::{AshareCode, BusinessCompositionQuery, F10MarketCode};
    const PERIOD: &[u8] = br#"{"ResultSets":[{"ColName":["T002"],"Content":[["2024-06-30"]]}]}"#;
    for limited in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let address = listener.local_addr()?;
        listener.set_nonblocking(true)?;
        let worker = thread::spawn(move || -> TestResult {
            for index in 0..2 {
                let deadline = Instant::now() + Duration::from_secs(5);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error)
                            if error.kind() == std::io::ErrorKind::WouldBlock
                                && Instant::now() < deadline =>
                        {
                            thread::sleep(Duration::from_millis(2))
                        }
                        Err(error) => return Err(error.into()),
                    }
                };
                stream.set_nonblocking(false)?;
                stream.set_read_timeout(Some(Duration::from_secs(2)))?;
                let (_, body) = read_request(&mut stream)?;
                if index == 0 {
                    assert_eq!(body, br#"{"Params":["zygcfx","600000"]}"#);
                    answer(&mut stream, "200 OK", "", PERIOD)?;
                } else {
                    assert_eq!(body, br#"{"Params":["600000","zygc","20240630"]}"#);
                    answer(&mut stream, "200 OK", "", TABLE)?;
                }
            }
            Ok(())
        });
        let total = 2 * (PERIOD.len() + TABLE.len());
        let limit = if limited { total - 1 } else { total };
        let client = F10Client::builder()
            .with_base_url(format!("http://{address}/TQLEX"))
            .with_max_operation_bytes(NonZeroU64::new(limit as u64).ok_or("nonzero")?)
            .build()?;
        let query =
            BusinessCompositionQuery::new(AshareCode::new(F10MarketCode::new(1), "600000")?);
        let result = client.business_composition(
            &query,
            &OperationOptions::default().with_raw(RawPolicy::RetainPayload),
        );
        if limited {
            assert_eq!(result.unwrap_err().kind(), ErrorKind::LimitExceeded);
        } else {
            let response = result?;
            assert_eq!(response.meta().sources().len(), 2);
            let raw = response.meta().raw().ok_or("missing composed raw")?;
            assert_eq!(raw.payloads().len(), 2);
            assert_eq!(raw.payloads()[0].as_ref(), PERIOD);
            assert_eq!(raw.payloads()[1].as_ref(), TABLE);
        }
        client.close()?;
        worker.join().map_err(|_| "report fixture panicked")??;
    }
    Ok(())
}

#[test]
fn encoded_chunked_body_and_response_header_caps_apply_while_reading() -> TestResult {
    for headers in [false, true] {
        let server = Server::start(move |mut stream| {
            read_request(&mut stream)?;
            if headers {
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nX-Large: {}\r\nContent-Length: 2\r\n\r\n{{}}",
                    "x".repeat(1000)
                )?;
            } else {
                stream.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n10\r\n1234567890123456\r\n0\r\n\r\n")?;
            }
            Ok(())
        })?;
        let builder = F10Client::builder().with_base_url(server.url());
        let client = if headers {
            builder
                .with_max_header_bytes(NonZeroUsize::new(512).ok_or("nonzero")?)
                .build()?
        } else {
            builder
                .with_max_encoded_bytes(NonZeroU64::new(14).ok_or("nonzero")?)
                .build()?
        };
        // Request body must fit the encoded cap too.
        let query = CallQuery::new(
            EntryName::new("sample")?,
            RequestPayload::Body(JsonValue::Object(Vec::new())),
        );
        let error = client
            .call(&query, &OperationOptions::default())
            .unwrap_err();
        assert_eq!(
            error.kind(),
            ErrorKind::LimitExceeded,
            "headers={headers}: {error:?}"
        );
        client.close()?;
        server.finish()?;
    }
    Ok(())
}

#[test]
fn cancellation_timeout_and_close_during_response_confirm_connection_release() -> TestResult {
    for mode in ["cancel", "timeout", "close"] {
        let (ready_tx, ready_rx) = mpsc::channel();
        let server = Server::start(move |mut stream| {
            read_request(&mut stream)?;
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\n{")?;
            ready_tx.send(())?;
            eof(&mut stream)
        })?;
        let client = client(server.url())?;
        let worker_client = client.clone();
        let token = CancellationToken::default();
        let options = OperationOptions::default()
            .with_cancellation(token.clone())
            .with_timeout(if mode == "timeout" {
                Duration::from_millis(200)
            } else {
                Duration::from_secs(2)
            });
        let worker = thread::spawn(move || worker_client.call(&query("sample")?, &options));
        ready_rx.recv_timeout(Duration::from_secs(2))?;
        if mode == "cancel" {
            token.cancel();
        }
        if mode == "close" {
            client.close()?;
        }
        let error = worker
            .join()
            .map_err(|_| "client worker panic")?
            .unwrap_err();
        assert_eq!(
            error.kind(),
            match mode {
                "cancel" => ErrorKind::Cancelled,
                "timeout" => ErrorKind::Timeout,
                _ => ErrorKind::Closed,
            }
        );
        client.close()?;
        server.finish()?;
    }
    Ok(())
}

fn certificate() -> Result<(Vec<u8>, Arc<rustls::ServerConfig>), Box<dyn StdError + Send + Sync>> {
    let generated = rcgen::generate_simple_self_signed(vec!["f10.fixture.test".to_owned()])?;
    let der = generated.cert.der().to_vec();
    let key = rustls::pki_types::PrivatePkcs8KeyDer::from(generated.key_pair.serialize_der());
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()?
    .with_no_client_auth()
    .with_single_cert(
        vec![rustls::pki_types::CertificateDer::from(der.clone())],
        key.into(),
    )?;
    Ok((der, Arc::new(config)))
}

/// Minimal independent UDP DNS A/AAAA authority for f10.fixture.test. It returns
/// a refused IPv4 address first and a listening IPv6 address second when asked.
fn dns_server(
    address: SocketAddr,
) -> Result<(SocketAddr, mpsc::Sender<()>, JoinHandle<TestResult>), Box<dyn StdError + Send + Sync>>
{
    let socket = UdpSocket::bind("127.0.0.1:0")?;
    socket.set_read_timeout(Some(Duration::from_millis(30)))?;
    let local = socket.local_addr()?;
    let (stop_tx, stop_rx) = mpsc::channel();
    let thread = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        while stop_rx.try_recv().is_err() && Instant::now() < deadline {
            let mut bytes = [0_u8; 2048];
            let (count, peer) = match socket.recv_from(&mut bytes) {
                Ok(value) => value,
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    continue
                }
                Err(error) => return Err(Box::new(error) as Box<dyn StdError + Send + Sync>),
            };
            if count < 17 {
                return Err("short DNS query".into());
            }
            let mut end = 12;
            while bytes[end] != 0 {
                end += bytes[end] as usize + 1;
            }
            end += 1;
            let kind = u16::from_be_bytes([bytes[end], bytes[end + 1]]);
            let payload = match kind {
                1 => Some(vec![127, 0, 0, 1]),
                28 if address.is_ipv6() => Some(std::net::Ipv6Addr::LOCALHOST.octets().to_vec()),
                _ => None,
            };
            let mut response = Vec::from(&bytes[..2]);
            response.extend_from_slice(&[
                0x81,
                0x80,
                0,
                1,
                0,
                u8::from(payload.is_some()),
                0,
                0,
                0,
                0,
            ]);
            response.extend_from_slice(&bytes[12..end + 4]);
            if let Some(payload) = payload {
                response.extend_from_slice(&[0xc0, 0x0c]);
                response.extend_from_slice(&kind.to_be_bytes());
                response.extend_from_slice(&[0, 1, 0, 0, 0, 0, 0, payload.len() as u8]);
                response.extend_from_slice(&payload);
            }
            socket.send_to(&response, peer)?;
        }
        Ok(())
    });
    Ok((local, stop_tx, thread))
}

#[test]
fn https_uses_custom_roots_sni_and_ipv4_failure_then_ipv6_fallback() -> TestResult {
    let (der, tls) = certificate()?;
    let server = Server::at("[::1]:0", move |stream| {
        let session = rustls::ServerConnection::new(tls)?;
        let mut stream = rustls::StreamOwned::new(session, stream);
        let (header, _) = read_request(&mut stream)?;
        assert!(header.contains("f10.fixture.test:"));
        assert_eq!(stream.conn.server_name(), Some("f10.fixture.test"));
        answer(&mut stream, "200 OK", "", TABLE)?;
        Ok(())
    })?;
    let (dns, stop, dns_thread) = dns_server(server.address)?;
    let client = F10Client::builder()
        .with_base_url(format!(
            "https://f10.fixture.test:{}/TQLEX",
            server.address.port()
        ))
        .with_resolver(ResolverPolicy::Nameservers(vec![dns]))
        .with_tls(TlsPolicy::CustomRoots(vec![der]))
        .build()?;
    assert!(client
        .call(&query("sample")?, &OperationOptions::default())?
        .ok());
    client.close()?;
    stop.send(())?;
    dns_thread.join().map_err(|_| "DNS fixture panic")??;
    server.finish()
}

#[test]
fn explicit_connect_proxy_keeps_tls_hostname_and_rejects_untrusted_certificate() -> TestResult {
    for trusted in [true, false] {
        let (der, tls) = certificate()?;
        let server = Server::start(move |mut socket| {
            let (request, _) = read_request(&mut socket)?;
            assert!(request.starts_with("CONNECT f10.fixture.test:443 HTTP/1.1\r\n"));
            socket.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")?;
            let session = rustls::ServerConnection::new(tls)?;
            let mut stream = rustls::StreamOwned::new(session, socket);
            if trusted {
                read_request(&mut stream)?;
                assert_eq!(stream.conn.server_name(), Some("f10.fixture.test"));
                answer(&mut stream, "200 OK", "", TABLE)?;
            } else {
                assert!(read_request(&mut stream).is_err());
            }
            Ok(())
        })?;
        let client = F10Client::builder()
            .with_base_url("https://f10.fixture.test/TQLEX")
            .with_proxy(Some(ProxyUrl::new(&server.url().replace("/TQLEX", "/"))?))
            .with_tls(TlsPolicy::CustomRoots(if trusted {
                vec![der]
            } else {
                vec![certificate()?.0]
            }))
            .build()?;
        let response = client.call(&query("sample")?, &OperationOptions::default());
        if trusted {
            assert!(response?.ok());
        } else {
            assert_eq!(response.unwrap_err().kind(), ErrorKind::Connection);
        }
        client.close()?;
        server.finish()?;
    }
    Ok(())
}

#[test]
fn dns_cancellation_retires_pending_resolver_without_connecting() -> TestResult {
    let socket = UdpSocket::bind("127.0.0.1:0")?;
    socket.set_read_timeout(Some(Duration::from_secs(2)))?;
    let client = F10ClientBuilder::new()
        .with_base_url("http://never.fixture.test:9000/TQLEX")
        .with_resolver(ResolverPolicy::Nameservers(vec![socket.local_addr()?]))
        .build()?;
    let worker_client = client.clone();
    let token = CancellationToken::default();
    let options = OperationOptions::default().with_cancellation(token.clone());
    let worker = thread::spawn(move || worker_client.call(&query("sample")?, &options));
    socket.recv_from(&mut [0_u8; 2048])?;
    token.cancel();
    assert_eq!(
        worker
            .join()
            .map_err(|_| "DNS client panic")?
            .unwrap_err()
            .kind(),
        ErrorKind::Cancelled
    );
    client.close()?;
    Ok(())
}

#[test]
fn cancelled_tls_and_connect_proxy_waits_confirm_socket_release() -> TestResult {
    for proxy in [false, true] {
        let (ready_tx, ready_rx) = mpsc::channel();
        let server = Server::start(move |mut stream| {
            if proxy {
                let (header, _) = read_request(&mut stream)?;
                assert!(header.starts_with("CONNECT "));
            } else {
                // Receive one TLS record without responding with ServerHello.
                let mut header = [0_u8; 5];
                stream.read_exact(&mut header)?;
                let count = u16::from_be_bytes([header[3], header[4]]) as usize;
                let mut body = vec![0_u8; count];
                stream.read_exact(&mut body)?;
            }
            ready_tx.send(())?;
            eof(&mut stream)
        })?;
        let builder = F10Client::builder().with_tls(TlsPolicy::CustomRoots(vec![certificate()?.0]));
        let client = if proxy {
            builder
                .with_base_url("https://f10.fixture.test/TQLEX")
                .with_proxy(Some(ProxyUrl::new(&server.url().replace("/TQLEX", "/"))?))
                .build()?
        } else {
            builder
                .with_base_url(server.url().replacen("http:", "https:", 1))
                .build()?
        };
        let copy = client.clone();
        let token = CancellationToken::default();
        let options = OperationOptions::default().with_cancellation(token.clone());
        let worker = thread::spawn(move || copy.call(&query("sample")?, &options));
        ready_rx.recv_timeout(Duration::from_secs(2))?;
        token.cancel();
        assert_eq!(
            worker
                .join()
                .map_err(|_| "cancel client panic")?
                .unwrap_err()
                .kind(),
            ErrorKind::Cancelled
        );
        client.close()?;
        server.finish()?;
    }
    Ok(())
}

#[test]
fn environment_proxy_variables_do_not_affect_the_explicit_none_policy() -> TestResult {
    const CHILD: &str = "ELTDX_F10_PROXY_ISOLATION_CHILD";
    if let Ok(url) = std::env::var(CHILD) {
        let client = client(url)?;
        assert!(client
            .call(&query("sample")?, &OperationOptions::default())?
            .ok());
        return Ok(client.close()?);
    }
    let server = Server::start(|mut stream| {
        read_request(&mut stream)?;
        answer(&mut stream, "200 OK", "", TABLE)?;
        eof(&mut stream)
    })?;
    // Configure only the subprocess environment; do not race other tests by
    // mutating this process's HTTP(S)_PROXY or NO_PROXY settings.
    let status = std::process::Command::new(std::env::current_exe()?)
        .arg("--exact")
        .arg("environment_proxy_variables_do_not_affect_the_explicit_none_policy")
        .env(CHILD, server.url())
        .env("HTTP_PROXY", "http://127.0.0.1:1")
        .env("HTTPS_PROXY", "http://127.0.0.1:1")
        .env("ALL_PROXY", "http://127.0.0.1:1")
        .env("http_proxy", "http://127.0.0.1:1")
        .env("https_proxy", "http://127.0.0.1:1")
        .env("NO_PROXY", "")
        .env("no_proxy", "")
        .status()?;
    assert!(status.success());
    server.finish()
}

#[test]
fn successful_connect_is_idempotent_and_closed_owner_rejects_before_network() -> TestResult {
    let server = Server::start(|mut stream| eof(&mut stream))?;
    let client = client(server.url())?;
    client.connect(&OperationOptions::default())?;
    client.connect(&OperationOptions::default())?;
    server.finish()?;
    assert_eq!(client.diagnostics()?.completed_requests(), 1);
    client.close()?;
    assert_eq!(
        client
            .connect(&OperationOptions::default())
            .unwrap_err()
            .kind(),
        ErrorKind::Closed
    );
    Ok(())
}

#[test]
fn simultaneous_connect_calls_share_one_transport_probe() -> TestResult {
    let (der, tls) = certificate()?;
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let server = Server::start(move |stream| {
        // Observe a real ClientHello without consuming bytes from rustls.
        let mut first = [0_u8];
        assert_eq!(stream.peek(&mut first)?, 1);
        entered_tx.send(())?;
        release_rx.recv_timeout(Duration::from_secs(2))?;
        let session = rustls::ServerConnection::new(tls)?;
        let mut secured = rustls::StreamOwned::new(session, stream);
        while secured.conn.is_handshaking() {
            secured.conn.complete_io(&mut secured.sock)?;
        }
        Ok(())
    })?;
    let (dns, stop, dns_thread) = dns_server(server.address)?;
    let client = F10Client::builder()
        .with_base_url(format!(
            "https://f10.fixture.test:{}/TQLEX",
            server.address.port()
        ))
        .with_resolver(ResolverPolicy::Nameservers(vec![dns]))
        .with_tls(TlsPolicy::CustomRoots(vec![der]))
        .build()?;
    let first_client = client.clone();
    let first = thread::spawn(move || first_client.connect(&OperationOptions::default()));
    entered_rx.recv_timeout(Duration::from_secs(2))?;
    let (waiting_tx, waiting_rx) = mpsc::channel();
    let second_client = client.clone();
    let second = thread::spawn(move || {
        waiting_tx.send(()).unwrap();
        second_client.connect(&OperationOptions::default())
    });
    waiting_rx.recv_timeout(Duration::from_secs(2))?;
    release_tx.send(())?;
    first.join().map_err(|_| "first connect panic")??;
    second.join().map_err(|_| "second connect panic")??;
    assert_eq!(client.diagnostics()?.completed_requests(), 1);
    stop.send(())?;
    dns_thread.join().map_err(|_| "DNS fixture panic")??;
    client.close()?;
    server.finish()
}

#[test]
fn connect_waiter_cancellation_keeps_the_first_probe_running() -> TestResult {
    let dns = UdpSocket::bind("127.0.0.1:0")?;
    dns.set_read_timeout(Some(Duration::from_secs(2)))?;
    let client = F10Client::builder()
        .with_base_url("https://pending.fixture.test/TQLEX")
        .with_resolver(ResolverPolicy::Nameservers(vec![dns.local_addr()?]))
        .build()?;
    let first_token = CancellationToken::default();
    let first_options = OperationOptions::default().with_cancellation(first_token.clone());
    let first_client = client.clone();
    let first = thread::spawn(move || first_client.connect(&first_options));
    dns.recv_from(&mut [0_u8; 2048])?;
    let waiter_token = CancellationToken::default();
    let waiter_options = OperationOptions::default().with_cancellation(waiter_token.clone());
    let waiter_client = client.clone();
    let waiter = thread::spawn(move || waiter_client.connect(&waiter_options));
    waiter_token.cancel();
    assert_eq!(
        waiter
            .join()
            .map_err(|_| "connect waiter panic")?
            .unwrap_err()
            .kind(),
        ErrorKind::Cancelled
    );
    assert!(!first.is_finished());
    first_token.cancel();
    assert_eq!(
        first
            .join()
            .map_err(|_| "first connect panic")?
            .unwrap_err()
            .kind(),
        ErrorKind::Cancelled
    );
    client.close()?;
    Ok(())
}

#[test]
fn invalid_certificate_hostname_is_rejected_even_when_root_is_trusted() -> TestResult {
    let (der, tls) = certificate()?;
    let server = Server::start(move |stream| {
        let session = rustls::ServerConnection::new(tls)?;
        let mut stream = rustls::StreamOwned::new(session, stream);
        assert!(read_request(&mut stream).is_err());
        Ok(())
    })?;
    // The certificate names f10.fixture.test, not this numeric loopback address.
    let client = F10Client::builder()
        .with_base_url(server.url().replacen("http:", "https:", 1))
        .with_tls(TlsPolicy::CustomRoots(vec![der]))
        .build()?;
    assert_eq!(
        client
            .call(&query("sample")?, &OperationOptions::default())
            .unwrap_err()
            .kind(),
        ErrorKind::Connection
    );
    client.close()?;
    server.finish()
}

#[test]
fn request_header_order_keeps_separated_duplicate_names() -> TestResult {
    use eltdx::f10::Header;
    let server = Server::start(|mut stream| {
        let (headers, _) = read_request(&mut stream)?;
        let lines: Vec<_> = headers.lines().collect();
        let first = lines
            .iter()
            .position(|line| *line == "X-Order: first")
            .ok_or("missing first header")?;
        assert_eq!(lines[first + 1], "X-Between: middle");
        assert_eq!(lines[first + 2], "X-Order: last");
        assert!(lines
            .iter()
            .any(|line| *line == "Accept: application/custom"));
        answer(&mut stream, "200 OK", "", TABLE)?;
        eof(&mut stream)
    })?;
    let client = F10Client::builder()
        .with_base_url(server.url())
        .with_headers(vec![
            Header::new("Accept", "application/custom")?,
            Header::new("X-Order", "first")?,
            Header::new("X-Between", "middle")?,
            Header::new("X-Order", "last")?,
        ])
        .build()?;
    assert!(client
        .call(&query("sample")?, &OperationOptions::default())?
        .ok());
    client.close()?;
    server.finish()
}

#[test]
fn truncated_compression_invalid_chunks_and_short_headers_are_protocol_errors() -> TestResult {
    for bytes in [
        &b"HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: 3\r\n\r\nxyz"[..],
        &b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nnot-hex\r\n"[..],
        &b"HTTP/1.1 200 OK\r\nContent-Len"[..],
    ] {
        let bytes = bytes.to_vec();
        let server = Server::start(move |mut stream| {
            read_request(&mut stream)?;
            stream.write_all(&bytes)?;
            Ok(())
        })?;
        let client = client(server.url())?;
        assert_eq!(
            client
                .call(&query("sample")?, &OperationOptions::default())
                .unwrap_err()
                .kind(),
            ErrorKind::Protocol
        );
        client.close()?;
        server.finish()?;
    }
    Ok(())
}

#[test]
fn queue_admission_honors_cancellation_before_a_fifth_worker_starts() -> TestResult {
    let dns = UdpSocket::bind("127.0.0.1:0")?;
    dns.set_read_timeout(Some(Duration::from_secs(2)))?;
    let client = F10Client::builder()
        .with_base_url("http://pending.fixture.test:9000/TQLEX")
        .with_resolver(ResolverPolicy::Nameservers(vec![dns.local_addr()?]))
        .build()?;
    let token = CancellationToken::default();
    let mut workers = Vec::new();
    for _ in 0..4 {
        let client = client.clone();
        let options = OperationOptions::default().with_cancellation(token.clone());
        workers.push(thread::spawn(move || {
            client.call(&query("sample")?, &options)
        }));
        dns.recv_from(&mut [0_u8; 2048])?;
    }
    let queued_token = CancellationToken::default();
    let queued_client = client.clone();
    let options = OperationOptions::default().with_cancellation(queued_token.clone());
    let queued = thread::spawn(move || queued_client.call(&query("sample")?, &options));
    queued_token.cancel();
    assert_eq!(
        queued
            .join()
            .map_err(|_| "queue client panic")?
            .unwrap_err()
            .kind(),
        ErrorKind::Cancelled
    );
    token.cancel();
    for worker in workers {
        assert_eq!(
            worker
                .join()
                .map_err(|_| "pending client panic")?
                .unwrap_err()
                .kind(),
            ErrorKind::Cancelled
        );
    }
    client.close()?;
    Ok(())
}

#[test]
fn close_during_dns_releases_the_owner_and_rejects_future_calls() -> TestResult {
    let dns = UdpSocket::bind("127.0.0.1:0")?;
    dns.set_read_timeout(Some(Duration::from_secs(2)))?;
    let client = F10Client::builder()
        .with_base_url("http://pending.fixture.test:9000/TQLEX")
        .with_resolver(ResolverPolicy::Nameservers(vec![dns.local_addr()?]))
        .build()?;
    let copy = client.clone();
    let worker = thread::spawn(move || copy.call(&query("sample")?, &OperationOptions::default()));
    dns.recv_from(&mut [0_u8; 2048])?;
    client.close()?;
    assert_eq!(
        worker
            .join()
            .map_err(|_| "close client panic")?
            .unwrap_err()
            .kind(),
        ErrorKind::Closed
    );
    assert_eq!(
        client
            .call(&query("sample")?, &OperationOptions::default())
            .unwrap_err()
            .kind(),
        ErrorKind::Closed
    );
    Ok(())
}

#[test]
fn zlib_requires_checksum_end_and_rejects_trailing_bytes() -> TestResult {
    use flate2::{write::ZlibEncoder, Compression};
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(TABLE)?;
    let compressed = encoder.finish()?;
    for variant in [0, 1, 2] {
        let mut data = compressed.clone();
        if variant == 1 {
            data.truncate(data.len() - 2);
        }
        if variant == 2 {
            data.extend_from_slice(b"extra");
        }
        let server = Server::start(move |mut stream| {
            read_request(&mut stream)?;
            answer(
                &mut stream,
                "200 OK",
                "Content-Encoding: deflate\r\n",
                &data,
            )
        })?;
        let client = client(server.url())?;
        let result = client.call(&query("sample")?, &OperationOptions::default());
        if variant == 0 {
            assert!(result?.ok());
        } else {
            assert_eq!(result.unwrap_err().kind(), ErrorKind::Protocol);
        }
        client.close()?;
        server.finish()?;
    }
    Ok(())
}

#[test]
fn gzip_requires_footer_and_preserves_concatenated_member_semantics() -> TestResult {
    use flate2::{write::GzEncoder, Compression};
    fn gzip(bytes: &[u8]) -> Result<Vec<u8>, std::io::Error> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(bytes)?;
        encoder.finish()
    }
    let split = TABLE.len() / 2;
    let mut members = gzip(&TABLE[..split])?;
    members.extend_from_slice(&gzip(&TABLE[split..])?);
    for (mut data, valid) in [(members, true), (gzip(TABLE)?, false)] {
        if !valid {
            data.truncate(data.len() - 4);
        }
        let server = Server::start(move |mut stream| {
            read_request(&mut stream)?;
            answer(&mut stream, "200 OK", "Content-Encoding: gzip\r\n", &data)
        })?;
        let client = client(server.url())?;
        let result = client.call(&query("sample")?, &OperationOptions::default());
        if valid {
            assert!(result?.ok());
        } else {
            assert_eq!(result.unwrap_err().kind(), ErrorKind::Protocol);
        }
        client.close()?;
        server.finish()?;
    }
    Ok(())
}

#[test]
fn ambiguous_or_unsupported_content_codings_are_rejected() -> TestResult {
    for headers in [
        "Content-Encoding: identity\r\nContent-Encoding: gzip\r\n",
        "Content-Encoding: identity, gzip\r\n",
        "Content-Encoding: br\r\n",
    ] {
        let server = Server::start(move |mut stream| {
            read_request(&mut stream)?;
            answer(&mut stream, "200 OK", headers, TABLE)
        })?;
        let client = client(server.url())?;
        let result = client.call(&query("sample")?, &OperationOptions::default());
        let closed = client.close();
        let joined = server.finish();
        assert_eq!(
            result.expect_err("unsupported coding accepted").kind(),
            ErrorKind::Protocol
        );
        closed?;
        joined?;
    }
    Ok(())
}

#[test]
fn opted_in_same_origin_redirect_preserves_post_body_and_request_headers() -> TestResult {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    listener.set_nonblocking(true)?;
    let server = thread::spawn(move || -> TestResult {
        let deadline = Instant::now() + Duration::from_secs(5);
        for index in 0..2 {
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(2))
                    }
                    Err(error) => return Err(Box::new(error)),
                }
            };
            stream.set_nonblocking(false)?;
            stream.set_read_timeout(Some(Duration::from_secs(2)))?;
            stream.set_write_timeout(Some(Duration::from_secs(2)))?;
            let (header, body) = read_request(&mut stream)?;
            assert_eq!(body, "{\"Params\":[\"中文\",7]}".as_bytes());
            if index == 0 {
                assert!(header.starts_with("POST /TQLEX?Entry=sample HTTP/1.1\r\n"));
                answer(
                    &mut stream,
                    "307 Temporary Redirect",
                    "Location: /moved?Entry=sample\r\n",
                    b"",
                )?;
            } else {
                assert!(header.starts_with("POST /moved?Entry=sample HTTP/1.1\r\n"));
                answer(&mut stream, "200 OK", "", TABLE)?;
            }
        }
        Ok(())
    });
    let client = F10Client::builder()
        .with_base_url(format!("http://{address}/TQLEX"))
        .with_max_redirects(1)
        .build()?;
    let response = client.call(&query("sample")?, &OperationOptions::default())?;
    assert!(response.ok());
    assert!(client
        .diagnostics()?
        .active_endpoint()
        .unwrap_or_default()
        .contains("/moved?Entry=sample"));
    client.close()?;
    server.join().map_err(|_| "redirect fixture panic")?
}

#[test]
fn redirect_policy_rejects_cross_origin_method_rewrite_and_limit_exhaustion() -> TestResult {
    for (status, location) in [
        ("307 Temporary Redirect", "http://127.0.0.1:1/other"),
        ("302 Found", "/moved"),
        ("308 Permanent Redirect", "/moved#fragment"),
        (
            "307 Temporary Redirect",
            "http://user:password@127.0.0.1:1/other",
        ),
    ] {
        let extra = format!("Location: {location}\r\n");
        let server = Server::start(move |mut stream| {
            read_request(&mut stream)?;
            answer(&mut stream, status, &extra, b"")
        })?;
        let client = F10Client::builder()
            .with_base_url(server.url())
            .with_max_redirects(1)
            .build()?;
        assert_eq!(
            client
                .call(&query("sample")?, &OperationOptions::default())
                .unwrap_err()
                .kind(),
            ErrorKind::Protocol
        );
        client.close()?;
        server.finish()?;
    }
    let server = Server::start(|mut stream| {
        read_request(&mut stream)?;
        answer(
            &mut stream,
            "307 Temporary Redirect",
            "Location: /moved\r\n",
            b"",
        )
    })?;
    let client = client(server.url())?;
    assert_eq!(
        client
            .call(&query("sample")?, &OperationOptions::default())
            .unwrap_err()
            .kind(),
        ErrorKind::Protocol
    );
    client.close()?;
    server.finish()
}
