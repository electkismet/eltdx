//! Synthetic loopback framing. This is not a real-server capture.

use std::io::{self, Read, Write};
use std::net::{SocketAddr, Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use eltdx_protocol::frame::{RequestFrame, RESPONSE_PREFIX};

pub struct Server {
    pub address: SocketAddr,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<io::Result<()>>>,
    accepted: Arc<AtomicUsize>,
}

/// A separately owned fixture writer. Test channels can publish unsolicited
/// frames or close a source after establishment without timing sleeps.
#[derive(Clone, Debug)]
pub struct Connection {
    writer: Arc<Mutex<TcpStream>>,
    push_id: Arc<AtomicU32>,
}

impl Connection {
    pub fn push(&self, msg_type: u16, payload: &[u8]) -> io::Result<()> {
        let message = self.push_id.fetch_add(1, Ordering::Relaxed);
        self.write(&framed_response(message, msg_type, payload))
    }

    pub fn disconnect(&self) -> io::Result<()> {
        self.writer.lock().map_err(|_| io::Error::other("fixture writer poisoned"))?
            .shutdown(Shutdown::Both)
    }

    fn write(&self, raw: &[u8]) -> io::Result<()> {
        self.writer.lock().map_err(|_| io::Error::other("fixture writer poisoned"))?.write_all(raw)
    }
}

impl Server {
    pub fn start(handler: impl Fn(&RequestFrame) -> Vec<u8> + Send + Sync + 'static) -> io::Result<Self> {
        Self::start_controlled(move |request, _| handler(request))
    }

    pub fn start_controlled(handler: impl Fn(&RequestFrame, &Connection) -> Vec<u8> + Send + Sync + 'static) -> io::Result<Self> {
        Self::start_profile(handler, false)
    }

    /// Allows explicit session business bytes as well as ordinary commands.
    pub fn start_all(handler: impl Fn(&RequestFrame) -> Vec<u8> + Send + Sync + 'static) -> io::Result<Self> {
        Self::start_profile(move |request, _| handler(request), true)
    }

    fn start_profile(handler: impl Fn(&RequestFrame, &Connection) -> Vec<u8> + Send + Sync + 'static, custom_session: bool) -> io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let accepted = Arc::new(AtomicUsize::new(0));
        let worker_accepted = Arc::clone(&accepted);
        let handler = Arc::new(handler);
        let worker = thread::spawn(move || {
            let mut connections = Vec::new();
            while !worker_stop.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        worker_accepted.fetch_add(1, Ordering::SeqCst);
                        stream.set_read_timeout(Some(Duration::from_millis(100)))?;
                        stream.set_write_timeout(Some(Duration::from_secs(2)))?;
                        let connection = Connection { writer: Arc::new(Mutex::new(stream.try_clone()?)),
                            push_id: Arc::new(AtomicU32::new(0x7000_0000)) };
                        let handler = Arc::clone(&handler);
                        let stop = Arc::clone(&worker_stop);
                        connections.push(thread::spawn(move || -> io::Result<()> {
                            while !stop.load(Ordering::Acquire) {
                                let request = match read_request(&mut stream) {
                                    Ok(request) => request,
                                    Err(error) if matches!(error.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => continue,
                                    Err(error) if matches!(error.kind(), io::ErrorKind::UnexpectedEof | io::ErrorKind::ConnectionReset
                                        | io::ErrorKind::ConnectionAborted | io::ErrorKind::NotConnected) => return Ok(()),
                                    Err(error) => return Err(error),
                                };
                                let payload = if custom_session { handler(&request, &connection) } else { match request.msg_type {
                                    0x000d => vec![0; 189],
                                    0x0004 => vec![0; 10],
                                    _ => handler(&request, &connection),
                                } };
                                if let Err(error) = connection.write(&response(&request, &payload)) {
                                    if matches!(error.kind(), io::ErrorKind::BrokenPipe | io::ErrorKind::NotConnected
                                        | io::ErrorKind::ConnectionReset | io::ErrorKind::ConnectionAborted) { return Ok(()); }
                                    return Err(error);
                                }
                            }
                            Ok(())
                        }));
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => thread::sleep(Duration::from_millis(1)),
                    Err(error) => return Err(error),
                }
            }
            for connection in connections {
                connection.join().map_err(|_| io::Error::new(io::ErrorKind::Other, "fixture connection panicked"))??;
            }
            Ok(())
        });
        Ok(Self { address, stop, worker: Some(worker), accepted })
    }

    pub fn connection_count(&self) -> usize { self.accepted.load(Ordering::SeqCst) }

    pub fn finish(mut self) -> io::Result<()> {
        self.stop.store(true, Ordering::Release);
        self.worker.take().expect("server worker").join()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "fixture server panicked"))?
    }
}

impl Drop for Server {
    fn drop(&mut self) { self.stop.store(true, Ordering::Release); }
}

fn read_request(stream: &mut TcpStream) -> io::Result<RequestFrame> {
    // A timeout is retried only before any header byte was read. Once a frame
    // starts, read_exact must complete it rather than lose partial framing.
    let mut first = [0; 1];
    stream.read_exact(&mut first)?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    let mut raw = vec![0; 12];
    raw[0] = first[0];
    stream.read_exact(&mut raw[1..])?;
    let body = usize::from(u16::from_le_bytes([raw[6], raw[7]])).checked_sub(2)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "bad request length"))?;
    raw.resize(12 + body, 0);
    stream.read_exact(&mut raw[12..])?;
    stream.set_read_timeout(Some(Duration::from_millis(100)))?;
    RequestFrame::decode(&raw).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn response(request: &RequestFrame, payload: &[u8]) -> Vec<u8> {
    framed_response(request.msg_id, request.msg_type, payload)
}

fn framed_response(msg_id: u32, msg_type: u16, payload: &[u8]) -> Vec<u8> {
    let length = u16::try_from(payload.len()).expect("bounded synthetic payload");
    let mut raw = Vec::new();
    raw.extend_from_slice(&RESPONSE_PREFIX);
    raw.push(1);
    raw.extend_from_slice(&msg_id.to_le_bytes());
    raw.push(0);
    raw.extend_from_slice(&msg_type.to_le_bytes());
    raw.extend_from_slice(&length.to_le_bytes());
    raw.extend_from_slice(&length.to_le_bytes());
    raw.extend_from_slice(payload);
    raw
}

pub fn directory(rows: &[(&str, u8)]) -> Vec<u8> {
    let mut payload = (rows.len() as u16).to_le_bytes().to_vec();
    for &(number, decimal) in rows {
        let mut record = [0; 37];
        record[..6].copy_from_slice(number.as_bytes());
        record[6..8].copy_from_slice(&100_u16.to_le_bytes());
        record[8..12].copy_from_slice(b"TEST");
        record[24..28].copy_from_slice(&1_f32.to_le_bytes());
        record[28] = decimal;
        record[29..33].copy_from_slice(&10_f32.to_le_bytes());
        payload.extend_from_slice(&record);
    }
    payload
}

pub fn directory_offset(request: &RequestFrame) -> u32 {
    u32::from_le_bytes(request.data[2..6].try_into().expect("directory offset"))
}

pub fn snapshots(rows: &[(&str, i64)]) -> Vec<u8> {
    let mut payload = vec![0, 0];
    payload.extend_from_slice(&(rows.len() as u16).to_le_bytes());
    for &(number, current_hundredths) in rows {
        payload.push(1); // Shanghai.
        payload.extend_from_slice(number.as_bytes());
        payload.extend_from_slice(&1_u16.to_le_bytes());
        append_varint(&mut payload, current_hundredths / 100);
        for _ in 0..8 { append_varint(&mut payload, 0); } // four price deltas and time/unknown/total/current.
        payload.extend_from_slice(&0_u32.to_le_bytes());
        for _ in 0..8 { append_varint(&mut payload, 0); } // inside/outside/unknown/open, bid/ask/volumes.
    }
    payload
}

/// Source hand volumes allow profile arithmetic to have nonzero inputs.
pub fn snapshots_with_volume(rows: &[(&str, i64, i64)]) -> Vec<u8> {
    let mut payload = vec![0, 0];
    payload.extend_from_slice(&(rows.len() as u16).to_le_bytes());
    for &(number, current_hundredths, total_hand) in rows {
        payload.push(1); payload.extend_from_slice(number.as_bytes());
        payload.extend_from_slice(&1_u16.to_le_bytes());
        append_varint(&mut payload, current_hundredths / 100);
        for _ in 0..6 { append_varint(&mut payload, 0); }
        append_varint(&mut payload, total_hand); append_varint(&mut payload, 0);
        payload.extend_from_slice(&0_u32.to_le_bytes());
        for _ in 0..8 { append_varint(&mut payload, 0); }
    }
    payload
}

/// Fixed finance records. Only circulation/total (ten-thousand shares) and
/// raw EPS vary; remaining source values are finite zero, with explicit dates.
pub fn finance(rows: &[(&str, f32, f32, f32)]) -> Vec<u8> {
    let mut payload = (rows.len() as u16).to_le_bytes().to_vec();
    for &(number, circulation, total, eps) in rows {
        payload.push(1); payload.extend_from_slice(number.as_bytes());
        payload.extend_from_slice(&circulation.to_le_bytes());
        payload.extend_from_slice(&3_u16.to_le_bytes()); payload.extend_from_slice(&7_u16.to_le_bytes());
        payload.extend_from_slice(&20261116_u32.to_le_bytes()); payload.extend_from_slice(&19910101_u32.to_le_bytes());
        for index in 1..31 {
            let value = if index == 1 { total } else if index == 7 { eps } else { 0.0 };
            payload.extend_from_slice(&value.to_le_bytes());
        }
    }
    payload
}

pub fn finance_codes(request: &RequestFrame) -> Vec<String> {
    request.data[2..].chunks_exact(7).map(|row| String::from_utf8(row[1..7].to_vec()).unwrap()).collect()
}

/// Legacy pseudo-index fixtures use wire hundredths for each price. The amount
/// word is finite zero; volume is23 hands. No production capture is implied.
pub fn legacy_prices(rows: &[(&str, i64, i64, i64, i64)]) -> Vec<u8> {
    let mut payload = vec![0, 0];
    payload.extend_from_slice(&(rows.len() as u16).to_le_bytes());
    for &(number, last, open, high, low) in rows {
        payload.push(1); payload.extend_from_slice(number.as_bytes());
        payload.extend_from_slice(&1_u16.to_le_bytes());
        for value in [last, 0, open - last, high - last, low - last, 93000, 0, 23, 0] {
            append_varint(&mut payload, value);
        }
        payload.extend_from_slice(&0_u32.to_le_bytes());
        for _ in 0..24 { append_varint(&mut payload, 0); }
        payload.extend_from_slice(&0_u16.to_le_bytes());
        for _ in 0..4 { append_varint(&mut payload, 0); }
        payload.extend_from_slice(&[0; 4]); // explicit optional long tail.
    }
    payload
}

pub fn trade_offset(request: &RequestFrame) -> u16 {
    let offset = if request.msg_type == 0x0fc5 { 8 } else { 12 };
    u16::from_le_bytes(request.data[offset..offset + 2].try_into().unwrap())
}

/// Ordinary tick fixtures: minute, per-page price delta, hand volume, status.
pub fn trade_rows(request: &RequestFrame, rows: &[(u16, i64, i64, i64)]) -> Vec<u8> {
    let mut payload = (rows.len() as u16).to_le_bytes().to_vec();
    if request.msg_type == 0x0fc6 { payload.extend_from_slice(&10_f32.to_le_bytes()); }
    for &(minute, delta, volume, status) in rows {
        payload.extend_from_slice(&minute.to_le_bytes());
        for value in [delta, volume, 2, status, 0] { append_varint(&mut payload, value); }
    }
    payload
}

pub fn handshake_dates(first: u32, second: u32) -> Vec<u8> {
    let mut payload = vec![0; 189];
    payload[1..3].copy_from_slice(&((first / 10000) as u16).to_le_bytes());
    payload[3] = (first % 100) as u8; payload[4] = (first / 100 % 100) as u8;
    payload[6] = 10; // source-local10:00:00.
    payload[42..46].copy_from_slice(&first.to_le_bytes());
    payload[50..54].copy_from_slice(&second.to_le_bytes());
    payload
}

pub fn auction_rows(rows: &[(u16, u8, f32, u32, i32)]) -> Vec<u8> {
    let mut payload = (rows.len() as u16).to_le_bytes().to_vec();
    for &(minute, second, price, matched, unmatched) in rows {
        payload.extend_from_slice(&minute.to_le_bytes()); payload.extend_from_slice(&price.to_le_bytes());
        payload.extend_from_slice(&matched.to_le_bytes()); payload.extend_from_slice(&unmatched.to_le_bytes());
        payload.push(0); payload.push(second);
    }
    payload
}

pub fn refresh(rows: &[(&str, u32)]) -> Vec<u8> {
    let mut payload = (rows.len() as u16).to_le_bytes().to_vec();
    for &(number, cursor) in rows {
        payload.push(1); // Shanghai.
        payload.extend_from_slice(number.as_bytes());
        payload.extend_from_slice(&1_u16.to_le_bytes());
        append_varint(&mut payload, 10);
        for _ in 0..4 { append_varint(&mut payload, 0); }
        payload.extend_from_slice(&cursor.to_le_bytes());
        for _ in 0..3 { append_varint(&mut payload, 0); }
        payload.extend_from_slice(&0_u32.to_le_bytes());
        for _ in 0..24 { append_varint(&mut payload, 0); } // four fields, five levels with four fields each.
    }
    for byte in &mut payload { *byte ^= 0x93; }
    payload
}

/// Category fixtures deliberately retain the supplied order and duplicates.
pub fn category(rows: &[&str]) -> Vec<u8> {
    let mut payload = vec![0, 0];
    payload.extend_from_slice(&(rows.len() as u16).to_le_bytes());
    for number in rows {
        payload.push(1);
        payload.extend_from_slice(number.as_bytes());
        payload.extend_from_slice(&1_u16.to_le_bytes());
        append_varint(&mut payload, 1_000);
        for _ in 0..8 { append_varint(&mut payload, 0); }
        payload.extend_from_slice(&0_u32.to_le_bytes());
        for _ in 0..8 { append_varint(&mut payload, 0); }
        payload.extend_from_slice(&[0; 56]);
    }
    payload
}

pub fn category_window(request: &RequestFrame) -> (u16, u16) {
    (u16::from_le_bytes(request.data[4..6].try_into().unwrap()),
        u16::from_le_bytes(request.data[6..8].try_into().unwrap()))
}

pub fn snapshot_codes(request: &RequestFrame) -> Vec<String> {
    request.data[10..].chunks_exact(7).map(|row| String::from_utf8(row[1..].to_vec()).unwrap()).collect()
}

pub fn refresh_requests(request: &RequestFrame) -> Vec<(String, u32)> {
    request.data[2..].chunks_exact(11).map(|row| (String::from_utf8(row[1..7].to_vec()).unwrap(),
        u32::from_le_bytes(row[7..11].try_into().unwrap()))).collect()
}

pub(crate) fn append_varint(target: &mut Vec<u8>, value: i64) {
    let mut magnitude = value.unsigned_abs();
    let mut first = (magnitude & 0x3f) as u8;
    magnitude >>= 6;
    if value < 0 { first |= 0x40; }
    if magnitude != 0 { first |= 0x80; }
    target.push(first);
    while magnitude != 0 {
        let mut byte = (magnitude & 0x7f) as u8;
        magnitude >>= 7;
        if magnitude != 0 { byte |= 0x80; }
        target.push(byte);
    }
}
