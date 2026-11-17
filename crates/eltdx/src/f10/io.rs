use std::io;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use super::control::Control;
use crate::{Error, ErrorKind, Result, TimeoutPhase};

pub(crate) trait Stream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Stream for T {}
pub(crate) type BoxStream = Box<dyn Stream>;

/// Enforce the response-head byte budget before Hyper receives the bytes.
/// Hyper's client build does not expose its server-only TooLarge classifier.
/// This observer only finds head boundaries; Hyper still validates HTTP syntax.
#[derive(Debug)]
struct HeaderBudget {
    maximum: usize,
    received: usize,
    prefix: [u8; 12],
    tail: [u8; 4],
    active: bool,
}

impl HeaderBudget {
    fn new(maximum: usize) -> Self {
        Self {
            maximum,
            received: 0,
            prefix: [0; 12],
            tail: [0; 4],
            active: true,
        }
    }

    fn record(&mut self, bytes: &[u8]) -> Result<()> {
        for &byte in bytes {
            if !self.active {
                break;
            }
            if self.received == self.maximum {
                return Err(Error::new(
                    ErrorKind::LimitExceeded,
                    "F10 response headers exceed limit",
                ));
            }
            if self.received < self.prefix.len() {
                self.prefix[self.received] = byte;
            }
            self.received += 1;
            self.tail.rotate_left(1);
            self.tail[3] = byte;
            // Hyper also accepts LF-only line endings. Keep boundary counting
            // consistent without inspecting or retaining the response body.
            if self.tail == *b"\r\n\r\n" || self.tail[2..] == *b"\n\n" {
                let informational = self.prefix.starts_with(b"HTTP/1.")
                    && self.prefix[8] == b' '
                    && self.prefix[9] == b'1'
                    && self.prefix[10].is_ascii_digit()
                    && self.prefix[11].is_ascii_digit()
                    && &self.prefix[9..12] != b"101";
                self.active = informational;
                self.received = 0;
                self.prefix = [0; 12];
                self.tail = [0; 4];
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod header_budget_tests {
    use super::*;

    #[test]
    fn every_head_fragment_boundary_accepts_exact_limit_and_excludes_body() -> Result<()> {
        for head in [
            b"HTTP/1.1 200 OK\r\nContent-Length: 1000\r\n\r\n".as_slice(),
            b"HTTP/1.1 200 OK\nContent-Length: 1000\n\n".as_slice(),
        ] {
            for split in 0..=head.len() {
                let mut budget = HeaderBudget::new(head.len());
                budget.record(&head[..split])?;
                budget.record(&head[split..])?;
                budget.record(&[b'x'; 1000])?;
                assert!(!budget.active);
            }
        }
        Ok(())
    }

    #[test]
    fn unterminated_or_one_byte_oversize_head_returns_typed_limit() -> Result<()> {
        let head = b"HTTP/1.1 200 OK\r\n\r\n";
        let mut budget = HeaderBudget::new(head.len() - 1);
        budget.record(&head[..head.len() - 1])?;
        assert!(matches!(budget.record(&head[head.len() - 1..]),
            Err(error) if error.kind() == ErrorKind::LimitExceeded));
        let partial = b"HTTP/1.1 200 OK\r\n";
        let mut unterminated = HeaderBudget::new(partial.len());
        unterminated.record(partial)?;
        assert!(matches!(unterminated.record(b"X"),
            Err(error) if error.kind() == ErrorKind::LimitExceeded));
        Ok(())
    }

    #[test]
    fn informational_heads_each_have_a_budget_before_the_final_body() -> Result<()> {
        let interim = b"HTTP/1.1 103 Early Hints\r\n\r\n";
        let final_head = b"HTTP/1.1 200 OK\r\n\r\n";
        let mut budget = HeaderBudget::new(interim.len());
        for _ in 0..2 {
            for byte in interim {
                budget.record(std::slice::from_ref(byte))?;
            }
            assert!(budget.active);
        }
        budget.record(final_head)?;
        budget.record(&[0; 1024])?;
        assert!(!budget.active);
        let mut oversized = HeaderBudget::new(interim.len());
        oversized.record(interim)?;
        assert!(
            matches!(oversized.record(b"HTTP/1.1 200 OK\r\nX-Large: too long\r\n\r\n"),
            Err(error) if error.kind() == ErrorKind::LimitExceeded)
        );
        Ok(())
    }
}

/// Bounded observation of the actual HTTP plaintext received from the peer,
/// including status/headers, chunk framing and encoded body. TLS wrapping is
/// below this observer; ciphertext is intentionally not presented as HTTP raw.
#[derive(Debug)]
pub(crate) struct WireCapture {
    maximum: usize,
    received: usize,
    retain: bool,
    bytes: Vec<u8>,
    failure: Option<Error>,
    control: Control,
    headers: HeaderBudget,
}
impl WireCapture {
    pub fn new(
        maximum: usize,
        header_maximum: usize,
        retain: bool,
        control: Control,
    ) -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self {
            maximum,
            received: 0,
            retain,
            bytes: Vec::new(),
            failure: None,
            control,
            headers: HeaderBudget::new(header_maximum),
        }))
    }
    pub fn take_bytes(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.bytes)
    }
    pub fn failure(&self) -> Option<Error> {
        self.failure.clone()
    }
    fn record(&mut self, bytes: &[u8]) -> Result<()> {
        let result = (|| {
            self.headers.record(bytes)?;
            self.received = self.received.checked_add(bytes.len()).ok_or_else(|| {
                Error::new(ErrorKind::LimitExceeded, "F10 wire byte counter overflow")
            })?;
            if self.received > self.maximum {
                return Err(Error::new(
                    ErrorKind::LimitExceeded,
                    "F10 HTTP framing and wire byte limit exceeded",
                ));
            }
            if self.retain {
                self.control.sponsor.record_bytes(bytes.len())?;
                self.bytes.try_reserve(bytes.len()).map_err(|error| {
                    Error::new(ErrorKind::LimitExceeded, "cannot reserve F10 wire capture")
                        .with_source(error)
                })?;
                self.bytes.extend_from_slice(bytes);
            }
            Ok(())
        })();
        if let Err(error) = &result {
            self.failure = Some(error.clone());
        }
        result
    }
}

pub(crate) struct ObservedStream {
    pub stream: BoxStream,
    pub capture: Arc<Mutex<WireCapture>>,
    pub control: Control,
    pub headers: Option<Vec<u8>>,
    pub generated: Vec<u8>,
    pub pending: Vec<u8>,
    pub written: usize,
}
impl AsyncRead for ObservedStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if let Err(error) = self.control.check(TimeoutPhase::Response) {
            return Poll::Ready(Err(io::Error::other(error)));
        }
        let start = buffer.filled().len();
        match Pin::new(&mut self.stream).poll_read(cx, buffer) {
            Poll::Ready(Ok(())) => {
                let result = self
                    .capture
                    .lock()
                    .map_err(|_| io::Error::other("F10 capture lock poisoned"))
                    .and_then(|mut capture| {
                        capture
                            .record(&buffer.filled()[start..])
                            .map_err(io::Error::other)
                    });
                Poll::Ready(result)
            }
            value => value,
        }
    }
}
impl AsyncWrite for ObservedStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        if let Err(error) = self.control.check(TimeoutPhase::Send) {
            return Poll::Ready(Err(io::Error::other(error)));
        }
        match self.flush_pending(cx) {
            Poll::Ready(Ok(())) => {}
            Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
            Poll::Pending => return Poll::Pending,
        }
        // HeaderMap groups duplicate names. Preserve the caller's complete
        // ordered list by replacing only Hyper's first request header block
        // with its already validated, framing-equivalent representation.
        // Body bytes still pass through Hyper's framing without modification.
        if let Some(headers) = &self.headers {
            let maximum = headers.len().saturating_add(8192);
            let mut consumed = 0;
            for &byte in bytes {
                if self.generated.len() >= maximum {
                    return Poll::Ready(Err(io::Error::other(
                        "generated request headers exceeded bound",
                    )));
                }
                self.generated.push(byte);
                consumed += 1;
                if self.generated.ends_with(b"\r\n\r\n") {
                    self.pending = self.headers.take().unwrap_or_default();
                    self.generated.clear();
                    self.written = 0;
                    break;
                }
            }
            return Poll::Ready(Ok(consumed));
        }
        Pin::new(&mut self.stream).poll_write(cx, bytes)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.flush_pending(cx) {
            Poll::Ready(Ok(())) => Pin::new(&mut self.stream).poll_flush(cx),
            value => value,
        }
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.flush_pending(cx) {
            Poll::Ready(Ok(())) => Pin::new(&mut self.stream).poll_shutdown(cx),
            value => value,
        }
    }
}
impl ObservedStream {
    fn flush_pending(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        while self.written < self.pending.len() {
            if let Err(error) = self.control.check(TimeoutPhase::Send) {
                return Poll::Ready(Err(io::Error::other(error)));
            }
            match Pin::new(&mut self.stream).poll_write(cx, &self.pending[self.written..]) {
                Poll::Ready(Ok(0)) => return Poll::Ready(Err(io::ErrorKind::WriteZero.into())),
                Poll::Ready(Ok(count)) => self.written += count,
                Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                Poll::Pending => return Poll::Pending,
            }
        }
        self.pending.clear();
        self.written = 0;
        Poll::Ready(Ok(()))
    }
}

/// Writer used by streaming decompression. Both total output bytes and parent
/// cancellation are checked before growing storage, including compressed bombs.
pub(crate) struct Sink {
    pub bytes: Vec<u8>,
    maximum: usize,
    control: Control,
    pub failure: Option<Error>,
}
impl Sink {
    pub fn new(maximum: usize, control: Control) -> Self {
        Self {
            bytes: Vec::new(),
            maximum,
            control,
            failure: None,
        }
    }
}
impl io::Write for Sink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let result = (|| {
            self.control.check(TimeoutPhase::Decode)?;
            let next =
                self.bytes.len().checked_add(bytes.len()).ok_or_else(|| {
                    Error::new(ErrorKind::LimitExceeded, "F10 decoded size overflow")
                })?;
            if next > self.maximum {
                return Err(Error::new(
                    ErrorKind::LimitExceeded,
                    "F10 decoded response exceeds limit",
                ));
            }
            self.control.sponsor.record_bytes(bytes.len())?;
            self.bytes.try_reserve(bytes.len()).map_err(|error| {
                Error::new(
                    ErrorKind::LimitExceeded,
                    "cannot reserve F10 decoded response",
                )
                .with_source(error)
            })?;
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        })();
        result.map_err(|error: Error| {
            self.failure = Some(error.clone());
            io::Error::other(error)
        })
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(crate) enum Decoder {
    Plain(Sink),
    Gzip(flate2::write::MultiGzDecoder<Sink>),
    Deflate(Deflater),
}
impl Decoder {
    pub fn new(encoding: &str, maximum: usize, control: Control) -> Result<Self> {
        let sink = Sink::new(maximum, control);
        match encoding.trim().to_ascii_lowercase().as_str() {
            "" | "identity" => Ok(Self::Plain(sink)),
            "gzip" | "x-gzip" => Ok(Self::Gzip(flate2::write::MultiGzDecoder::new(sink))),
            "deflate" => Ok(Self::Deflate(Deflater {
                decoder: flate2::Decompress::new(true),
                sink,
                ended: false,
            })),
            _ => Err(Error::new(
                ErrorKind::Protocol,
                "unsupported F10 Content-Encoding",
            )),
        }
    }
    fn sink(&self) -> &Sink {
        match self {
            Self::Plain(sink) => sink,
            Self::Gzip(decoder) => decoder.get_ref(),
            Self::Deflate(decoder) => &decoder.sink,
        }
    }
    pub fn write(&mut self, bytes: &[u8]) -> Result<()> {
        use io::Write;
        let result = match self {
            Self::Plain(sink) => sink.write_all(bytes),
            Self::Gzip(decoder) => decoder.write_all(bytes),
            Self::Deflate(decoder) => decoder.decode(bytes),
        };
        result.map_err(|error| {
            self.sink().failure.clone().unwrap_or_else(|| {
                Error::new(ErrorKind::Protocol, "invalid compressed F10 response")
                    .with_source(error)
            })
        })
    }
    pub fn finish(mut self) -> Result<Vec<u8>> {
        let result = match &mut self {
            Self::Plain(_) => Ok(()),
            Self::Gzip(decoder) => decoder.try_finish(),
            Self::Deflate(decoder) if decoder.ended => Ok(()),
            Self::Deflate(_) => Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "truncated zlib response",
            )),
        };
        if let Err(error) = result {
            return Err(self.sink().failure.clone().unwrap_or_else(|| {
                Error::new(ErrorKind::Protocol, "truncated compressed F10 response")
                    .with_source(error)
            }));
        }
        let sink = match self {
            Self::Plain(sink) => sink,
            Self::Gzip(decoder) => decoder.finish().map_err(|error| {
                Error::new(ErrorKind::Protocol, "invalid gzip end").with_source(error)
            })?,
            Self::Deflate(decoder) => decoder.sink,
        };
        Ok(sink.bytes)
    }
}

pub(crate) struct Deflater {
    decoder: flate2::Decompress,
    sink: Sink,
    ended: bool,
}
impl Deflater {
    fn decode(&mut self, mut bytes: &[u8]) -> io::Result<()> {
        use io::Write;
        if self.ended && !bytes.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "trailing data after zlib response",
            ));
        }
        loop {
            self.sink
                .control
                .check(TimeoutPhase::Decode)
                .map_err(io::Error::other)?;
            let mut output = [0_u8; 8192];
            let before_in = self.decoder.total_in();
            let before_out = self.decoder.total_out();
            let status = self
                .decoder
                .decompress(bytes, &mut output, flate2::FlushDecompress::None)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
            let consumed = (self.decoder.total_in() - before_in) as usize;
            let produced = (self.decoder.total_out() - before_out) as usize;
            self.sink.write_all(&output[..produced])?;
            bytes = &bytes[consumed..];
            if status == flate2::Status::StreamEnd {
                self.ended = true;
                if !bytes.is_empty() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "trailing data after zlib response",
                    ));
                }
                return Ok(());
            }
            if consumed == 0 && produced == 0 {
                if bytes.is_empty() {
                    return Ok(());
                }
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "zlib decoder made no progress",
                ));
            }
            if bytes.is_empty() && produced < output.len() {
                return Ok(());
            }
        }
    }
}
