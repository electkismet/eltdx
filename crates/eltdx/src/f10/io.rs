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

/// Bounded observation of the actual HTTP plaintext received from the peer,
/// including status/headers, chunk framing and encoded body. TLS wrapping is
/// below this observer; ciphertext is intentionally not presented as HTTP raw.
#[derive(Debug)]
pub(crate) struct WireCapture {
    maximum: usize, received: usize, retain: bool, bytes: Vec<u8>, failure: Option<Error>,
}
impl WireCapture {
    pub fn new(maximum: usize, retain: bool) -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self { maximum, received: 0, retain, bytes: Vec::new(), failure: None }))
    }
    pub fn take_bytes(&mut self) -> Vec<u8> { std::mem::take(&mut self.bytes) }
    pub fn failure(&self) -> Option<Error> { self.failure.clone() }
    fn record(&mut self, bytes: &[u8]) -> Result<()> {
        let result = (|| {
            self.received = self.received.checked_add(bytes.len())
                .ok_or_else(|| Error::new(ErrorKind::LimitExceeded, "F10 wire byte counter overflow"))?;
            if self.received > self.maximum { return Err(Error::new(ErrorKind::LimitExceeded, "F10 HTTP framing and wire byte limit exceeded")); }
            if self.retain {
                self.bytes.try_reserve(bytes.len()).map_err(|error| Error::new(ErrorKind::LimitExceeded, "cannot reserve F10 wire capture").with_source(error))?;
                self.bytes.extend_from_slice(bytes);
            }
            Ok(())
        })();
        if let Err(error) = &result { self.failure = Some(error.clone()); }
        result
    }
}

pub(crate) struct ObservedStream {
    pub stream: BoxStream, pub capture: Arc<Mutex<WireCapture>>, pub control: Control,
    pub headers: Option<Vec<u8>>, pub generated: Vec<u8>, pub pending: Vec<u8>, pub written: usize,
}
impl AsyncRead for ObservedStream {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buffer: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        if let Err(error) = self.control.check(TimeoutPhase::Response) { return Poll::Ready(Err(io::Error::other(error))); }
        let start = buffer.filled().len();
        match Pin::new(&mut self.stream).poll_read(cx, buffer) {
            Poll::Ready(Ok(())) => {
                let result = self.capture.lock().map_err(|_| io::Error::other("F10 capture lock poisoned"))
                    .and_then(|mut capture| capture.record(&buffer.filled()[start..]).map_err(io::Error::other));
                Poll::Ready(result)
            }
            value => value,
        }
    }
}
impl AsyncWrite for ObservedStream {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, bytes: &[u8]) -> Poll<io::Result<usize>> {
        if let Err(error) = self.control.check(TimeoutPhase::Send) { return Poll::Ready(Err(io::Error::other(error))); }
        match self.flush_pending(cx) { Poll::Ready(Ok(())) => {}, Poll::Ready(Err(error)) => return Poll::Ready(Err(error)), Poll::Pending => return Poll::Pending }
        // HeaderMap groups duplicate names. Preserve the caller's complete
        // ordered list by replacing only Hyper's first request header block
        // with its already validated, framing-equivalent representation.
        // Body bytes still pass through Hyper's framing without modification.
        if let Some(headers) = &self.headers {
            let maximum = headers.len().saturating_add(8192);
            let mut consumed = 0;
            for &byte in bytes {
                if self.generated.len() >= maximum { return Poll::Ready(Err(io::Error::other("generated request headers exceeded bound"))); }
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
        match self.flush_pending(cx) { Poll::Ready(Ok(())) => Pin::new(&mut self.stream).poll_flush(cx), value => value }
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.flush_pending(cx) { Poll::Ready(Ok(())) => Pin::new(&mut self.stream).poll_shutdown(cx), value => value }
    }
}
impl ObservedStream {
    fn flush_pending(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        while self.written < self.pending.len() {
            if let Err(error) = self.control.check(TimeoutPhase::Send) { return Poll::Ready(Err(io::Error::other(error))); }
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
pub(crate) struct Sink { pub bytes: Vec<u8>, maximum: usize, control: Control, pub failure: Option<Error> }
impl Sink {
    pub fn new(maximum: usize, control: Control) -> Self { Self { bytes: Vec::new(), maximum, control, failure: None } }
}
impl io::Write for Sink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let result = (|| {
            self.control.check(TimeoutPhase::Decode)?;
            let next = self.bytes.len().checked_add(bytes.len()).ok_or_else(|| Error::new(ErrorKind::LimitExceeded, "F10 decoded size overflow"))?;
            if next > self.maximum { return Err(Error::new(ErrorKind::LimitExceeded, "F10 decoded response exceeds limit")); }
            self.bytes.try_reserve(bytes.len()).map_err(|error| Error::new(ErrorKind::LimitExceeded, "cannot reserve F10 decoded response").with_source(error))?;
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        })();
        result.map_err(|error: Error| { self.failure = Some(error.clone()); io::Error::other(error) })
    }
    fn flush(&mut self) -> io::Result<()> { Ok(()) }
}

pub(crate) enum Decoder {
    Plain(Sink), Gzip(flate2::write::MultiGzDecoder<Sink>), Deflate(flate2::write::ZlibDecoder<Sink>),
}
impl Decoder {
    pub fn new(encoding: &str, maximum: usize, control: Control) -> Result<Self> {
        let sink = Sink::new(maximum, control);
        match encoding.trim().to_ascii_lowercase().as_str() {
            "" | "identity" => Ok(Self::Plain(sink)),
            "gzip" | "x-gzip" => Ok(Self::Gzip(flate2::write::MultiGzDecoder::new(sink))),
            "deflate" => Ok(Self::Deflate(flate2::write::ZlibDecoder::new(sink))),
            _ => Err(Error::new(ErrorKind::Protocol, "unsupported F10 Content-Encoding")),
        }
    }
    fn sink(&self) -> &Sink { match self { Self::Plain(sink) => sink, Self::Gzip(decoder) => decoder.get_ref(), Self::Deflate(decoder) => decoder.get_ref() } }
    pub fn write(&mut self, bytes: &[u8]) -> Result<()> {
        use io::Write;
        let result = match self { Self::Plain(sink) => sink.write_all(bytes), Self::Gzip(decoder) => decoder.write_all(bytes), Self::Deflate(decoder) => decoder.write_all(bytes) };
        result.map_err(|error| self.sink().failure.clone().unwrap_or_else(|| Error::new(ErrorKind::Protocol, "invalid compressed F10 response").with_source(error)))
    }
    pub fn finish(mut self) -> Result<Vec<u8>> {
        let result = match &mut self { Self::Plain(_) => Ok(()), Self::Gzip(decoder) => decoder.try_finish(), Self::Deflate(decoder) => decoder.try_finish() };
        if let Err(error) = result { return Err(self.sink().failure.clone().unwrap_or_else(|| Error::new(ErrorKind::Protocol, "truncated compressed F10 response").with_source(error))); }
        let sink = match self {
            Self::Plain(sink) => sink,
            Self::Gzip(decoder) => decoder.finish().map_err(|error| Error::new(ErrorKind::Protocol, "invalid gzip end").with_source(error))?,
            Self::Deflate(decoder) => decoder.finish().map_err(|error| Error::new(ErrorKind::Protocol, "invalid deflate end").with_source(error))?,
        };
        Ok(sink.bytes)
    }
}
