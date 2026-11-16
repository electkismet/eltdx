//! Synthetic loopback framing. This is not a real-server capture.

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use eltdx_protocol::frame::{RequestFrame, RESPONSE_PREFIX};

pub struct Server {
    pub address: SocketAddr,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<io::Result<()>>>,
    accepted: Arc<AtomicUsize>,
}

impl Server {
    pub fn start(handler: impl Fn(&RequestFrame) -> Vec<u8> + Send + Sync + 'static) -> io::Result<Self> {
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
                        let handler = Arc::clone(&handler);
                        let stop = Arc::clone(&worker_stop);
                        connections.push(thread::spawn(move || -> io::Result<()> {
                            while !stop.load(Ordering::Acquire) {
                                let request = match read_request(&mut stream) {
                                    Ok(request) => request,
                                    Err(error) if matches!(error.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => continue,
                                    Err(error) if matches!(error.kind(), io::ErrorKind::UnexpectedEof | io::ErrorKind::ConnectionReset) => return Ok(()),
                                    Err(error) => return Err(error),
                                };
                                let payload = match request.msg_type {
                                    0x000d => vec![0; 189],
                                    0x0004 => vec![0; 10],
                                    _ => handler(&request),
                                };
                                stream.write_all(&response(&request, &payload))?;
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
    let length = u16::try_from(payload.len()).expect("bounded synthetic payload");
    let mut raw = Vec::new();
    raw.extend_from_slice(&RESPONSE_PREFIX);
    raw.push(1);
    raw.extend_from_slice(&request.msg_id.to_le_bytes());
    raw.push(0);
    raw.extend_from_slice(&request.msg_type.to_le_bytes());
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

fn append_varint(target: &mut Vec<u8>, value: i64) {
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
