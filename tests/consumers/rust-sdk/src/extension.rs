//! Standalone synthetic EX/MAC/MAC-EX consumers. Only std and public eltdx
//! APIs are used. Synthetic login rules are never production ACK evidence.
use std::error::Error;
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use eltdx::{ErrorKind, OperationOptions, RawPolicy};

type Outcome = Result<(), Box<dyn Error>>;

struct Peer {
    endpoint: SocketAddr,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<io::Result<()>>>,
}
impl Peer {
    fn start(script: impl FnOnce(TcpStream) -> io::Result<()> + Send + 'static) -> io::Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        listener.set_nonblocking(true)?;
        let endpoint = listener.local_addr()?;
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let worker = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                if stopping.load(Ordering::Acquire) || Instant::now() >= deadline {
                    return Err(io::Error::new(io::ErrorKind::TimedOut, "extension consumer never connected"));
                }
                match listener.accept() {
                    Ok((stream, _)) => {
                        stream.set_read_timeout(Some(Duration::from_secs(3)))?;
                        stream.set_write_timeout(Some(Duration::from_secs(3)))?;
                        return script(stream);
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        thread::park_timeout(Duration::from_millis(1));
                    }
                    Err(error) => return Err(error),
                }
            }
        });
        Ok(Self { endpoint, stop, worker: Some(worker) })
    }
    fn finish(mut self) -> io::Result<()> {
        self.stop.store(true, Ordering::Release);
        self.worker.take().ok_or_else(|| io::Error::other("extension peer already joined"))?
            .join().map_err(|_| io::Error::other("extension peer panicked"))?
    }
}
impl Drop for Peer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() { let _ = worker.join(); }
    }
}

fn literal(text: &str) -> io::Result<Vec<u8>> {
    if text.len() % 2 != 0 { return Err(io::Error::other("odd fixture hex literal")); }
    (0..text.len()).step_by(2).map(|offset| {
        u8::from_str_radix(&text[offset..offset + 2], 16).map_err(io::Error::other)
    }).collect()
}
fn exact(stream: &mut TcpStream, expected: &[u8]) -> io::Result<()> {
    let mut actual = vec![0; expected.len()];
    stream.read_exact(&mut actual)?;
    crate::require(actual == expected, "extension request differs from independent literal")
}
fn eof(stream: &mut TcpStream) -> io::Result<()> {
    crate::require(stream.read(&mut [0])? == 0, "extension close did not produce peer EOF")
}
fn reply(stream: &mut TcpStream, command: u16, body: &[u8]) -> io::Result<()> {
    let size = u16::try_from(body.len()).map_err(io::Error::other)?;
    let mut frame = 7_654_321_u32.to_le_bytes().to_vec();
    frame.extend_from_slice(&0_u32.to_le_bytes());
    frame.extend_from_slice(&u32::from(command).to_le_bytes());
    frame.extend_from_slice(&size.to_le_bytes());
    frame.extend_from_slice(&size.to_le_bytes());
    frame.extend_from_slice(body);
    for fragment in frame.chunks(3) { stream.write_all(fragment)?; }
    Ok(())
}
fn settled(operation: Outcome, close: eltdx::Result<()>, peer: Peer) -> Outcome {
    let observed = peer.finish();
    if let Err(primary) = operation {
        if let Err(error) = close { eprintln!("extension close also failed: {error}"); }
        if let Err(error) = observed { eprintln!("extension peer also failed: {error}"); }
        return Err(primary);
    }
    close?;
    observed?;
    Ok(())
}

pub fn ex() -> Outcome {
    use eltdx::ex::{ExClient, ExCode, ExInstrumentId, ExMarketId, InstrumentsQuery, QuoteQuery};
    let peer = Peer::start(|mut stream| {
        let setup = format!("010148650001520052005424{}cce16dffd5ba3fb8cbc57a054f7748ea", "1f32c6e5d53dfb41".repeat(8));
        exact(&mut stream, &literal(&setup)?)?;
        reply(&mut stream, 0x2454, b"synthetic EX setup")?;
        exact(&mut stream, &literal("01024869000102000200f423")?)?;
        let mut market = vec![0; 66];
        market[0] = 1; market[2] = 2; market[3..10].copy_from_slice(b"Futures");
        market[35] = 47; market[36..38].copy_from_slice(b"IF");
        reply(&mut stream, 0x23f4, &market)?;
        exact(&mut stream, &literal("01034866000102000200f023")?)?;
        let mut count = vec![0; 23]; count[19..23].copy_from_slice(&1_u32.to_le_bytes());
        reply(&mut stream, 0x23f0, &count)?;
        exact(&mut stream, &literal("01044867000108000800f523000000000100")?)?;
        let mut info = vec![0; 70]; info[..4].copy_from_slice(&1_u32.to_le_bytes()); info[4] = 1;
        info[6] = 2; info[7] = 47; info[11..15].copy_from_slice(b"IFL0");
        info[20..32].copy_from_slice(b"Index future"); info[37..42].copy_from_slice(b"CFFEX");
        info[69] = 0xa5;
        reply(&mut stream, 0x23f5, &info)?;
        exact(&mut stream, &literal("0101080202010c000c00fa232f49464c300000000000")?)?;
        let mut quote = vec![0; 150]; quote[0] = 47; quote[1..5].copy_from_slice(b"IFL0");
        quote[30..34].copy_from_slice(&4002_f32.to_le_bytes());
        quote[70..74].copy_from_slice(&4001_f32.to_le_bytes());
        quote[126..130].copy_from_slice(&4006_f32.to_le_bytes());
        reply(&mut stream, 0x23fa, &quote)?;
        eof(&mut stream)
    })?;
    let client = ExClient::builder().with_endpoints(vec![peer.endpoint])
        .with_raw(RawPolicy::RetainWire).build()?;
    let operation = (|| -> Outcome {
        crate::require(client.host()?.is_none(), "EX builder performed eager network I/O")?;
        let options = OperationOptions::default();
        let markets = client.markets(&options)?;
        crate::require(markets.len() == 1 && markets[0].market().get() == 47
            && markets[0].name() == "Futures", "EX nonempty market projection changed")?;
        crate::require(client.instrument_count(&options)? == 1, "EX count changed")?;
        let info = client.instruments(&InstrumentsQuery::new(0).with_count(1)?, &options)?;
        crate::require(info.rows().len() == 1 && info.rows()[0].code() == "IFL0"
            && info.rows()[0].record_raw()[63] == 0xa5, "EX directory identity/raw changed")?;
        let id = ExInstrumentId::new(ExMarketId::new(47)?, ExCode::new("IFL0")?);
        let quote = client.quote(&QuoteQuery::new(id), &options)?
            .ok_or_else(|| io::Error::other("EX nonempty quote missing"))?;
        crate::require(quote.price().value().as_f32() == 4002.0
            && quote.bid()[0].price().value().as_f32() == 4001.0
            && quote.ask()[4].price().value().as_f32() == 4006.0, "EX price/depth changed")?;
        crate::require(quote.record_raw().is_some_and(|raw| raw.len() == 150), "EX explicit raw lost")?;
        Ok(())
    })();
    let cleanup = client.close().and_then(|()| client.close());
    settled(operation, cleanup, peer)?;
    crate::require(matches!(client.markets(&OperationOptions::default()), Err(error) if error.kind() == ErrorKind::Closed),
        "closed EX owner accepted an operation")?;
    Ok(())
}

#[cfg(any(feature = "sdk-mac", feature = "sdk-full"))]
fn mac_setup(stream: &mut TcpStream) -> io::Result<()> {
    for packet in ["0c0218930001030003000d0001", "0c0218940001030003000d0002",
        "0c031899000120002000db0fd5d0c9ccd6a4a8af0000008fc22540130000d500c9ccbdf0d7ea00000002"] {
        exact(stream, &literal(packet)?)?;
        reply(stream, 0x000d, b"synthetic required setup")?;
    }
    Ok(())
}
#[cfg(any(feature = "sdk-mac", feature = "sdk-full"))]
fn quote_request(head: u8, bitmap: [u8; 20], market: u16, code: &[u8]) -> io::Result<Vec<u8>> {
    let mut packet = literal("1c0000000001300030002b12")?;
    packet[0] = head; packet.extend_from_slice(&bitmap); packet.extend_from_slice(&1_u16.to_le_bytes());
    packet.extend_from_slice(&market.to_le_bytes());
    let mut identity = [0; 22]; identity[..code.len()].copy_from_slice(code);
    packet.extend_from_slice(&identity);
    Ok(packet)
}
#[cfg(any(feature = "sdk-mac", feature = "sdk-full"))]
fn quote_body(bitmap: [u8; 20], market: u16, code: &[u8], slots: &[u8]) -> Vec<u8> {
    let mut body = vec![0; 94]; body[..20].copy_from_slice(&bitmap);
    body[20..24].copy_from_slice(&1_u32.to_le_bytes()); body[24..26].copy_from_slice(&1_u16.to_le_bytes());
    body[26..28].copy_from_slice(&market.to_le_bytes()); body[28..28 + code.len()].copy_from_slice(code);
    body[50..54].copy_from_slice(b"name"); body.extend_from_slice(slots);
    body
}

#[cfg(any(feature = "sdk-mac", feature = "sdk-full"))]
pub fn mac() -> Outcome {
    use eltdx::mac::{FieldId, FieldSelection, MacClient, MacCode, MacInstrumentId, MacMarketId, QuotesQuery};
    use eltdx::mac::models::SlotValue;
    let peer = Peer::start(|mut stream| {
        mac_setup(&mut stream)?;
        let mut bitmap = [0; 20]; bitmap[0] = 1; bitmap[5] = 0x80;
        exact(&mut stream, &quote_request(0x1c, bitmap, 0, b"600000")?)?;
        let slots = [10_f32.to_le_bytes(), 0xdead_beef_u32.to_le_bytes()].concat();
        reply(&mut stream, 0x122b, &quote_body(bitmap, 0, b"600000", &slots))?;
        eof(&mut stream)
    })?;
    let client = MacClient::builder().with_endpoints(vec![peer.endpoint])
        .with_raw(RawPolicy::RetainWire).build()?;
    let id = MacInstrumentId::new(MacMarketId::new(0), MacCode::new("600000")?);
    let query = QuotesQuery::new(vec![id.clone()])?.with_fields(
        FieldSelection::from_ids([FieldId::new(0)?, FieldId::new(47)?]))?;
    let operation = (|| -> Outcome {
        let rows = client.quotes(&query, &OperationOptions::default())?;
        crate::require(rows.len() == 1 && rows[0].id() == &id && rows[0].name() == "name",
            "MAC nonempty quote identity changed")?;
        crate::require(rows[0].field(FieldId::new(47)?).is_some_and(|field|
            field.value == SlotValue::Unknown([0xef, 0xbe, 0xad, 0xde])), "MAC unknown slot was discarded")?;
        let raw = rows[0].meta().raw().ok_or_else(|| io::Error::other("MAC raw missing"))?;
        crate::require(raw.wire_frames().is_some_and(|frames| frames.len() == 1), "MAC original frame lost")?;
        Ok(())
    })();
    let cleanup = client.close().and_then(|()| client.close());
    settled(operation, cleanup, peer)?;
    crate::require(matches!(client.quotes(&query, &OperationOptions::default()), Err(error) if error.kind() == ErrorKind::Closed),
        "closed MAC owner accepted an operation")?;
    Ok(())
}

#[cfg(any(feature = "sdk-mac", feature = "sdk-full"))]
pub fn mac_ex() -> Outcome {
    use eltdx::ex::{ExCode, ExInstrumentId, ExMarketId};
    use eltdx::mac::{ex_query, MacExClient};
    use eltdx::mac::handshake::LoginReplyRules;
    let peer = Peer::start(|mut stream| {
        let mut login = literal("010000000001520052005424")?;
        login.extend_from_slice(&literal(concat!(
            "e5bb1c2fafe525941f32c6e5d53dfb415b734cc9cdbf0ac92021bfdd1eb06d22",
            "d008884c1611cb1378f6abd824d899d21f32c6e5d53dfb411f32c6e5d53dfb41",
            "a9325ac935dc0837335a16e4ce17c1bb"))?);
        exact(&mut stream, &login)?;
        reply(&mut stream, 0x2454, b"synthetic accepted")?;
        let mut bitmap = [0; 20]; bitmap[0] = 0xbf;
        exact(&mut stream, &quote_request(1, bitmap, 47, b"IFL0")?)?;
        let slots = [4000_f32.to_le_bytes(), 4001_f32.to_le_bytes(), 4005_f32.to_le_bytes(),
            3999_f32.to_le_bytes(), 4002_f32.to_le_bytes(), 300_u32.to_le_bytes(), 1_200_600_f32.to_le_bytes()].concat();
        reply(&mut stream, 0x122b, &quote_body(bitmap, 47, b"IFL0", &slots))?;
        eof(&mut stream)
    })?;
    // .into() uses the public API's inferred byte type; the consumer does not
    // import or directly depend on bytes, protocol, runtime or bindings.
    let rules = LoginReplyRules::new(vec![b"synthetic accepted".to_vec().into()],
        vec![b"synthetic denied".to_vec().into()], "standalone fixture only; no production evidence")?;
    let client = MacExClient::builder().with_endpoints(vec![peer.endpoint])
        .with_login_reply_rules(rules).with_raw(RawPolicy::RetainWire).build()?;
    let query = ex_query::QuotesQuery::new(vec![ExInstrumentId::new(ExMarketId::new(47)?, ExCode::new("IFL0")?)])?;
    let operation = (|| -> Outcome {
        let rows = client.quotes(&query, &OperationOptions::default())?;
        crate::require(rows.len() == 1 && rows[0].id().market().get() == 47
            && rows[0].id().code().as_str() == "IFL0", "MAC-EX identity changed")?;
        crate::require(rows[0].fields().iter().map(|field| field.bit.get()).collect::<Vec<_>>()
            == [0, 1, 2, 3, 4, 5, 7], "MAC-EX default seven fields changed")?;
        crate::require(rows[0].meta().raw().is_some(), "MAC-EX raw missing")?;
        Ok(())
    })();
    let cleanup = client.close().and_then(|()| client.close());
    settled(operation, cleanup, peer)?;
    crate::require(matches!(client.quotes(&query, &OperationOptions::default()), Err(error) if error.kind() == ErrorKind::Closed),
        "closed MAC-EX owner accepted an operation")?;
    Ok(())
}
