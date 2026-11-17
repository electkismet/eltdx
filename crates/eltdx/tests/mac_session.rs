//! Public-client fixture sources. These synthetic replies are not production
//! authentication or response-association evidence. Execution is deferred.
#![cfg(feature = "mac")]
use std::error::Error as StdError;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use bytes::Bytes;
use eltdx::{ErrorKind, OperationOptions, RawPolicy};
use eltdx::mac::*;
use eltdx::mac::handshake::{LoginReplyRules, MacSetupPolicy, OptionalSetupBoundary, ResponseIdentity, SetupReply};
type Case<T = ()> = Result<T,Box<dyn StdError + Send + Sync>>;

fn peer(script: impl FnOnce(TcpStream) -> Case + Send + 'static) -> Case<(Endpoint,JoinHandle<Case>)> {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST,0))?;
    let endpoint = listener.local_addr()?;
    let job = thread::spawn(move || {
        let (stream,_) = listener.accept()?;
        stream.set_read_timeout(Some(Duration::from_secs(3)))?;
        stream.set_write_timeout(Some(Duration::from_secs(3)))?;
        script(stream)
    });
    Ok((endpoint,job))
}
fn join(job: JoinHandle<Case>) -> Case { job.join().map_err(|_| std::io::Error::other("MAC fixture panicked"))? }
fn eof(stream: &mut TcpStream) -> Case { assert_eq!(stream.read(&mut [0])?,0); Ok(()) }
fn hex(text: &str) -> Case<Vec<u8>> {
    let mut bytes = Vec::new();
    for offset in (0..text.len()).step_by(2) { bytes.push(u8::from_str_radix(&text[offset..offset+2],16)?); }
    Ok(bytes)
}
fn exact(stream: &mut TcpStream, expected: &[u8]) -> Case {
    let mut bytes = vec![0;expected.len()]; stream.read_exact(&mut bytes)?;
    assert_eq!(bytes,expected); Ok(())
}
fn frame(stream: &mut TcpStream, method: u32, body: &[u8]) -> Case {
    let mut bytes = 7_654_321_u32.to_le_bytes().to_vec();
    bytes.extend_from_slice(&0_u32.to_le_bytes()); bytes.extend_from_slice(&method.to_le_bytes());
    let len = u16::try_from(body.len())?;
    bytes.extend_from_slice(&len.to_le_bytes()); bytes.extend_from_slice(&len.to_le_bytes()); bytes.extend_from_slice(body);
    for chunk in bytes.chunks(3) { stream.write_all(chunk)?; }
    Ok(())
}
fn required_setup(stream: &mut TcpStream) -> Case {
    for packet in ["0c0218930001030003000d0001","0c0218940001030003000d0002",
        "0c031899000120002000db0fd5d0c9ccd6a4a8af0000008fc22540130000d500c9ccbdf0d7ea00000002"] {
        exact(stream,&hex(packet)?)?; frame(stream,0,b"synthetic setup")?;
    }
    Ok(())
}
fn request(stream: &mut TcpStream, expected_head: u8, expected_command: u16) -> Case<Vec<u8>> {
    let mut header = [0;12]; stream.read_exact(&mut header)?;
    assert_eq!(header[0],expected_head);
    assert_eq!(&header[6..8],&header[8..10]);
    assert_eq!(u16::from_le_bytes([header[10],header[11]]),expected_command);
    let size = usize::from(u16::from_le_bytes([header[6],header[7]]));
    let mut body = vec![0;size.checked_sub(2).ok_or_else(|| std::io::Error::other("bad source request"))?];
    stream.read_exact(&mut body)?; Ok(body)
}
fn id() -> eltdx::Result<MacInstrumentId> { Ok(MacInstrumentId::new(MacMarketId::new(0),MacCode::new("600000")?)) }
fn quote_body() -> Vec<u8> {
    // Independent nonempty row: known float bit0 and unknown legal bit47.
    let mut body = vec![0;26+68+8]; body[0] = 1; body[5] = 0x80;
    body[20..24].copy_from_slice(&1_u32.to_le_bytes()); body[24..26].copy_from_slice(&1_u16.to_le_bytes());
    body[28..34].copy_from_slice(b"600000"); body[50..54].copy_from_slice(b"name");
    body[94..98].copy_from_slice(&0x4120_0000_u32.to_le_bytes());
    body[98..102].copy_from_slice(&0xdead_beef_u32.to_le_bytes()); body
}
fn rules() -> eltdx::Result<LoginReplyRules> {
    LoginReplyRules::new(vec![Bytes::from_static(b"synthetic accepted")],vec![Bytes::from_static(b"synthetic denied")],
        "synthetic fixture only; not production ACK evidence")
}

#[test]
fn builders_are_lazy_clones_share_close_and_other_owners_remain_independent() -> Case {
    let mac = MacClient::builder().build()?; let other = MacClient::builder().build()?;
    let mx = MacExClient::builder().build()?;
    assert!(mac.host()?.is_none()); assert!(mx.host()?.is_none());
    assert!(mac.healthy_hosts()?.is_empty()); assert!(mx.healthy_hosts()?.is_empty());
    assert_eq!(mac.config().timeout(),Duration::from_secs(8));
    assert!(mx.config().login_reply_rules().is_none());
    mac.clone().close()?;
    assert_eq!(mac.connect(&OperationOptions::default()).unwrap_err().kind(),ErrorKind::Closed);
    assert!(other.diagnostics()?.active_endpoint().is_none());
    other.close()?; mx.close()?; mac.close()?;
    assert!(MacClient::builder().with_endpoints(Vec::new()).build().is_err());
    assert!(MacExClient::builder().with_timeout(Duration::ZERO).build().is_err());
    assert!(MacClient::builder().with_cleanup_timeout(Duration::ZERO).build().is_err());
    Ok(())
}

#[test]
fn required_setup_nonempty_quotes_bits_meta_and_close_use_the_public_entry() -> Case {
    let (endpoint,job) = peer(|mut stream| {
        required_setup(&mut stream)?;
        let body = request(&mut stream,0x1c,0x122b)?;
        assert_eq!(body.len(),46); assert_eq!(&body[20..22],&1_u16.to_le_bytes());
        frame(&mut stream,0,&quote_body())?;
        eof(&mut stream)
    })?;
    let client = MacClient::builder().with_endpoints(vec![endpoint]).with_raw(RawPolicy::RetainWire).build()?;
    let fields = FieldSelection::from_ids([FieldId::new(0)?,FieldId::new(47)?]);
    let query = QuotesQuery::new(vec![id()?])?.with_fields(fields)?;
    let rows = client.quotes(&query,&OperationOptions::default())?;
    assert_eq!(rows.len(),1); assert_eq!(rows[0].id(),&id()?);
    assert_eq!(rows[0].name(),"name");
    assert_eq!(rows[0].field(FieldId::new(47)?).unwrap().value,models::SlotValue::Unknown([0xef,0xbe,0xad,0xde]));
    let raw = rows[0].meta().raw().unwrap(); assert_eq!(raw.row_ranges(),&[(0,26,102)]);
    assert_eq!(raw.wire_frames().unwrap().len(),1);
    assert_eq!(rows[0].meta().source().endpoint(),Some(endpoint));
    assert!(!rows[0].meta().diagnostics().is_empty());
    client.close()?; join(job)
}

#[test]
fn optional_setup_uses_exact_fence_then_keeps_business_response_aligned() -> Case {
    let (endpoint,job) = peer(|mut stream| {
        exact(&mut stream,&hex("0c0218930001030003000d0001")?)?; frame(&mut stream,1,b"required")?;
        exact(&mut stream,&hex("0c0218940001030003000d0002")?)?;
        exact(&mut stream,&hex("0c031899000120002000db0fd5d0c9ccd6a4a8af0000008fc22540130000d500c9ccbdf0d7ea00000002")?)?;
        request(&mut stream,0x1c,0x120f)?;
        // Optional step2 is absent. Step3 arrives before the mandatory fence.
        frame(&mut stream,3,b"optional third")?;
        let mut server = vec![0;71]; server[22..26].copy_from_slice(&20_261_117_u32.to_le_bytes());
        server[63..67].copy_from_slice(&20_261_116_u32.to_le_bytes());
        frame(&mut stream,4,&server)?;
        request(&mut stream,0x1c,0x124a)?; frame(&mut stream,0,&[0,0,1,0,2,0,0,0])?;
        eof(&mut stream)
    })?;
    let identity = |method| ResponseIdentity::new(7_654_321,0,method);
    let boundary = OptionalSetupBoundary::new([SetupReply::Required,SetupReply::Optional(identity(2)),SetupReply::Optional(identity(3))],
        identity(4),"synthetic ordered-fence fixture only")?;
    let client = MacClient::builder().with_endpoints(vec![endpoint]).with_setup_policy(MacSetupPolicy::OptionalWithFence(boundary)).build()?;
    let offsets = client.kline_offsets(&KlineOffsetsQuery::new(),&OperationOptions::default())?;
    assert_eq!(offsets.total(),256); assert_eq!(offsets.returned(),2);
    client.close()?; join(job)
}

#[test]
fn mac_ex_uses_login_then_head_rewrite_without_the_ex_setup_packet() -> Case {
    let (endpoint,job) = peer(|mut stream| {
        let login = request(&mut stream,1,0x2454)?; assert_eq!(login.len(),80);
        assert_eq!(&login[..8],&[0xe5,0xbb,0x1c,0x2f,0xaf,0xe5,0x25,0x94]);
        frame(&mut stream,0,b"synthetic accepted")?;
        let body = request(&mut stream,1,0x122b)?;
        assert_eq!(&body[..20],&[0xbf,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]);
        frame(&mut stream,0,&quote_body())?; eof(&mut stream)
    })?;
    let client = MacExClient::builder().with_endpoints(vec![endpoint]).with_login_reply_rules(rules()?).build()?;
    let id = eltdx::ex::ExInstrumentId::new(eltdx::ex::ExMarketId::from_wire(0),eltdx::ex::ExCode::new("600000")?);
    let rows = client.quotes(&ex_query::QuotesQuery::new(vec![id])?,&OperationOptions::default())?;
    assert_eq!(rows.len(),1); assert_eq!(rows[0].id().market().get(),0);
    assert!(rows[0].meta().raw().is_none()); client.close()?; join(job)
}

#[test]
fn negative_login_is_protocol_and_does_not_send_a_business_request() -> Case {
    let (endpoint,job) = peer(|mut stream| {
        request(&mut stream,1,0x2454)?; frame(&mut stream,0,b"synthetic denied")?; eof(&mut stream)
    })?;
    let client = MacExClient::builder().with_endpoints(vec![endpoint]).with_login_reply_rules(rules()?).build()?;
    assert_eq!(client.connect(&OperationOptions::default()).unwrap_err().kind(),ErrorKind::Protocol);
    assert!(client.host()?.is_none()); client.close()?; join(job)
}
