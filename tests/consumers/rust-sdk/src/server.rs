//! Small std-only HQ fixture. Never connects to an external address.
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

#[derive(Default)]
pub struct Observed { pub connections: usize, pub directory_offsets: Vec<u32>, pub snapshots: usize }
pub struct Fixture { pub endpoint: String, stop: Arc<AtomicBool>, worker: Option<JoinHandle<io::Result<Observed>>> }
impl Fixture {
    pub fn start() -> io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?; listener.set_nonblocking(true)?;
        let endpoint = format!("{}", listener.local_addr()?); let stop = Arc::new(AtomicBool::new(false)); let stopping = stop.clone();
        let worker = thread::spawn(move || {
            let observed = Arc::new(Mutex::new(Observed::default())); let mut children = Vec::new();
            let accept = (|| -> io::Result<()> {
                while !stopping.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            observed.lock().map_err(|_| io::Error::other("fixture observations poisoned"))?.connections += 1;
                            let child_stop = stopping.clone(); let child_observed = observed.clone();
                            children.push(thread::spawn(move || serve(stream, child_stop, child_observed)));
                        }
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => thread::park_timeout(Duration::from_millis(1)),
                        Err(error) => return Err(error),
                    }
                }
                Ok(())
            })();
            stopping.store(true, Ordering::Release);
            let mut child_error = None;
            for child in children { match child.join() {
                Ok(Ok(())) => {}, Ok(Err(error)) => { child_error.get_or_insert(error); },
                Err(_) => { child_error.get_or_insert(io::Error::other("fixture connection panicked")); },
            } }
            accept?; if let Some(error) = child_error { return Err(error); }
            Arc::try_unwrap(observed).map_err(|_| io::Error::other("fixture observations still borrowed"))?
                .into_inner().map_err(|_| io::Error::other("fixture observations poisoned"))
        });
        Ok(Self { endpoint, stop, worker: Some(worker) })
    }
    pub fn finish(mut self) -> io::Result<Observed> {
        self.stop.store(true, Ordering::Release);
        self.worker.take().ok_or_else(|| io::Error::other("fixture already stopped"))?.join()
            .map_err(|_| io::Error::other("fixture accept thread panicked"))?
    }
}
impl Drop for Fixture { fn drop(&mut self) { self.stop.store(true, Ordering::Release); } }

fn serve(mut stream: TcpStream, stop: Arc<AtomicBool>, observed: Arc<Mutex<Observed>>) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_millis(100)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    while !stop.load(Ordering::Acquire) {
        let mut header = [0_u8; 12];
        match stream.read_exact(&mut header[..1]) {
            Ok(()) => {},
            Err(error) if matches!(error.kind(), io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock) => continue,
            Err(error) if matches!(error.kind(), io::ErrorKind::UnexpectedEof | io::ErrorKind::ConnectionReset | io::ErrorKind::ConnectionAborted) => return Ok(()),
            Err(error) => return Err(error),
        }
        stream.set_read_timeout(Some(Duration::from_secs(2)))?; stream.read_exact(&mut header[1..])?;
        stream.set_read_timeout(Some(Duration::from_millis(100)))?;
        let first = u16::from_le_bytes([header[6], header[7]]); let second = u16::from_le_bytes([header[8], header[9]]);
        if header[0] != 0x0c || first != second || first < 2 { return Err(io::Error::other("invalid consumer request frame")); }
        let mut data = vec![0_u8; usize::from(first - 2)]; stream.read_exact(&mut data)?;
        let command = u16::from_le_bytes([header[10], header[11]]);
        let payload = match command {
            0x000d => { let mut bytes = vec![0; 189]; bytes[42..46].copy_from_slice(&20261113_u32.to_le_bytes());
                bytes[50..54].copy_from_slice(&20261116_u32.to_le_bytes()); bytes },
            0x0004 => vec![0; 10],
            0x044d => {
                if data.len() < 6 { return Err(io::Error::other("short directory request")); }
                let offset = u32::from_le_bytes([data[2], data[3], data[4], data[5]]);
                observed.lock().map_err(|_| io::Error::other("fixture observations poisoned"))?.directory_offsets.push(offset);
                if data[0] != 1 { vec![0, 0] } else { match offset { 0 => directory(b"600001"), 1 => directory(b"600000"), _ => vec![0, 0] } }
            }
            0x054c => {
                observed.lock().map_err(|_| io::Error::other("fixture observations poisoned"))?.snapshots += 1;
                if data.len() < 10 || (data.len() - 10) % 7 != 0 { return Err(io::Error::other("invalid snapshot request length")); }
                let codes = data[10..].chunks_exact(7).collect::<Vec<_>>(); let count = u16::try_from(codes.len()).map_err(io::Error::other)?;
                let mut bytes = vec![0, 0]; bytes.extend_from_slice(&count.to_le_bytes());
                for code in codes.into_iter().rev() {
                    if code[0] != 1 || (&code[1..] != b"600000" && &code[1..] != b"600001") { return Err(io::Error::other("unexpected consumer security")); }
                    bytes.extend_from_slice(code); bytes.extend_from_slice(&1_u16.to_le_bytes());
                    // Positive price delta in hundredths: 1000=>10.00 at decimal2.
                    varint(&mut bytes, if &code[1..] == b"600000" { 1000 } else { 2000 });
                    bytes.extend_from_slice(&[0; 8]); bytes.extend_from_slice(&[0; 4]); bytes.extend_from_slice(&[0; 8]);
                }
                bytes
            }
            _ => return Err(io::Error::other(format!("unexpected consumer command {command:04x}"))),
        };
        let length = u16::try_from(payload.len()).map_err(io::Error::other)?;
        let mut reply = vec![0xb1, 0xcb, 0x74, 0]; reply.push(1); reply.extend_from_slice(&header[1..5]);
        reply.push(0); reply.extend_from_slice(&command.to_le_bytes()); reply.extend_from_slice(&length.to_le_bytes());
        reply.extend_from_slice(&length.to_le_bytes()); reply.extend_from_slice(&payload); stream.write_all(&reply)?;
    }
    Ok(())
}
fn directory(number: &[u8; 6]) -> Vec<u8> {
    let mut record = [0_u8; 37]; record[..6].copy_from_slice(number); record[6..8].copy_from_slice(&100_u16.to_le_bytes());
    record[8..12].copy_from_slice(b"TEST"); record[24..28].copy_from_slice(&1_f32.to_le_bytes());
    record[28] = 2; record[29..33].copy_from_slice(&10_f32.to_le_bytes());
    let mut payload = vec![1, 0]; payload.extend_from_slice(&record); payload
}
fn varint(output: &mut Vec<u8>, value: u32) {
    let mut rest = value >> 6; output.push(((value & 0x3f) as u8) | if rest != 0 { 0x80 } else { 0 });
    while rest != 0 { let byte = (rest & 0x7f) as u8; rest >>= 7; output.push(byte | if rest != 0 { 0x80 } else { 0 }); }
}
