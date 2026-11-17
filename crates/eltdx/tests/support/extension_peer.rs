//! Synthetic public-client peer. Authored only; not production login evidence.
#![allow(dead_code)]
use std::error::Error;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use eltdx::mac::{BarsQuery, Endpoint, MacAdjust, MacCode, MacInstrumentId, MacMarketId};
pub type Case<T = ()> = Result<T,Box<dyn Error + Send + Sync>>;

pub fn peer(script: impl FnOnce(TcpStream) -> Case + Send + 'static) -> Case<(Endpoint,JoinHandle<Case>)> {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST,0))?;
    let endpoint = listener.local_addr()?;
    let task = thread::spawn(move || {
        let (stream,_) = listener.accept()?;
        stream.set_read_timeout(Some(Duration::from_secs(3)))?;
        stream.set_write_timeout(Some(Duration::from_secs(3)))?;
        script(stream)
    });
    Ok((endpoint,task))
}
pub fn join(task: JoinHandle<Case>) -> Case { task.join().map_err(|_| std::io::Error::other("extension fixture panicked"))? }
pub fn eof(stream: &mut TcpStream) -> Case { assert_eq!(stream.read(&mut [0])?,0); Ok(()) }
pub fn frame_bytes(body: &[u8]) -> Case<Vec<u8>> {
    let mut frame = 7_654_321_u32.to_le_bytes().to_vec();
    frame.extend_from_slice(&0_u32.to_le_bytes()); frame.extend_from_slice(&0_u32.to_le_bytes());
    let length = u16::try_from(body.len())?;
    frame.extend_from_slice(&length.to_le_bytes()); frame.extend_from_slice(&length.to_le_bytes());
    frame.extend_from_slice(body); Ok(frame)
}
pub fn frame(stream: &mut TcpStream, body: &[u8]) -> Case {
    for fragment in frame_bytes(body)?.chunks(7) { stream.write_all(fragment)?; }
    Ok(())
}
pub fn setup_ex(stream: &mut TcpStream) -> Case {
    let mut expected = vec![1,1,0x48,0x65,0,1,0x52,0,0x52,0,0x54,0x24];
    for _ in 0..8 { expected.extend_from_slice(&[0x1f,0x32,0xc6,0xe5,0xd5,0x3d,0xfb,0x41]); }
    expected.extend_from_slice(&[0xcc,0xe1,0x6d,0xff,0xd5,0xba,0x3f,0xb8,0xcb,0xc5,0x7a,5,0x4f,0x77,0x48,0xea]);
    assert_eq!(expected.len(),92); let mut actual = [0;92]; stream.read_exact(&mut actual)?;
    assert_eq!(actual.as_slice(),expected.as_slice()); frame(stream,b"synthetic EX setup reply")
}
pub fn setup_mac(stream: &mut TcpStream) -> Case {
    for literal in ["0c0218930001030003000d0001","0c0218940001030003000d0002",
        "0c031899000120002000db0fd5d0c9ccd6a4a8af0000008fc22540130000d500c9ccbdf0d7ea00000002"] {
        let mut expected = Vec::new();
        for index in (0..literal.len()).step_by(2) { expected.push(u8::from_str_radix(&literal[index..index+2],16)?); }
        let mut actual = vec![0;expected.len()]; stream.read_exact(&mut actual)?; assert_eq!(actual,expected);
        frame(stream,b"synthetic required setup")?;
    }
    Ok(())
}
pub fn request(stream: &mut TcpStream, head: u8, command: u16) -> Case<Vec<u8>> {
    let mut header = [0;12]; stream.read_exact(&mut header)?;
    assert_eq!(header[0],head); assert_eq!(&header[6..8],&header[8..10]);
    assert_eq!(u16::from_le_bytes([header[10],header[11]]),command);
    let length = usize::from(u16::from_le_bytes([header[6],header[7]]));
    let mut body = vec![0;length.checked_sub(2).ok_or_else(|| std::io::Error::other("bad request length"))?];
    stream.read_exact(&mut body)?; Ok(body)
}
pub fn bars_request(stream: &mut TcpStream, head: u8, adjust: u16, start: u32, count: u16) -> Case {
    let body = request(stream,head,0x122e)?;
    assert_eq!(u32::from_le_bytes(body[28..32].try_into()?),start);
    assert_eq!(u16::from_le_bytes(body[32..34].try_into()?),count);
    assert_eq!(u16::from_le_bytes(body[34..36].try_into()?),adjust);
    Ok(())
}
pub fn bars(values: &[(u32,f32)]) -> Case<Vec<u8>> {
    let mut body = vec![0;33]; body[27..29].copy_from_slice(&u16::try_from(values.len())?.to_le_bytes());
    for &(day,price) in values {
        body.extend_from_slice(&day.to_le_bytes()); body.extend_from_slice(&0_u32.to_le_bytes());
        for value in [price,price,price,price,1234.5,-0.0,0.0] { body.extend_from_slice(&value.to_bits().to_le_bytes()); }
    }
    Ok(body)
}
pub fn id() -> eltdx::Result<MacInstrumentId> { Ok(MacInstrumentId::new(MacMarketId::new(0),MacCode::new("600000")?)) }
pub fn qfq() -> eltdx::Result<BarsQuery> { BarsQuery::new(id()?).with_count(2).map(|query| query.with_adjust(MacAdjust::Qfq)) }
pub fn qfq_then_raw(stream: &mut TcpStream) -> Case {
    setup_mac(stream)?;
    bars_request(stream,0x1c,1,0,2)?; frame(stream,&bars(&[(20261117,-1.0),(20261118,20.0)])?)?;
    bars_request(stream,0x1c,0,0,2)?; frame(stream,&bars(&[(20261117,10.0),(20261118,20.0)])?)?;
    eof(stream)
}
