//! A byte stream that inflates what the server sends, so the WebSocket library above it only ever sees plain frames.
//!
//! Reads pass through the upgrade response untouched (the library needs to read its status and headers), then everything after
//! the blank line is WebSocket frames and goes through the [`Inflater`]. Writes pass straight through.
//!
//! Only a `101 Switching Protocols` answer is followed by frames. Any other answer (a 502/503 page while the game's
//! servers restart) is passed through untouched, so the library reports the HTTP status. Inflating that page's body as
//! frames used to fail with `RSV2 or RSV3 set` (`<` is 0x3C), which hid the real answer (problem 24).

use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use crate::deflate::Inflater;

enum Phase {
    /// Still inside the HTTP upgrade response; `tail` holds the last bytes seen, to find a blank line split across reads,
    /// and `status_line` the first bytes, to know whether frames follow.
    Headers { tail: Vec<u8>, status_line: Vec<u8> },
    Frames,
    /// The server did not switch protocols: everything is passed through as it came.
    Raw,
}

const STATUS_PREFIX_LEN: usize = 12; // "HTTP/1.1 101"

fn switches_protocols(status_line: &[u8]) -> bool {
    status_line.len() >= STATUS_PREFIX_LEN
        && status_line.starts_with(b"HTTP/1.")
        && status_line[8] == b' '
        && &status_line[9..12] == b"101"
}

pub struct InflatingStream<S> {
    inner: S,
    phase: Phase,
    inflater: Inflater,
    ready: Vec<u8>,
    ready_at: usize,
}

impl<S> InflatingStream<S> {
    pub fn new(inner: S) -> Self {
        Self { inner, phase: Phase::Headers { tail: Vec::new(), status_line: Vec::new() }, inflater: Inflater::new(), ready: Vec::new(), ready_at: 0 }
    }

    /// Take freshly read bytes: headers pass through, frames are inflated into `self.ready`.
    fn absorb(&mut self, chunk: &[u8]) -> io::Result<()> {
        let (frames_from, upgraded) = match &mut self.phase {
            Phase::Frames => (Some(0), true),
            Phase::Raw => (None, false),
            Phase::Headers { tail, status_line } => {
                let wanted = STATUS_PREFIX_LEN.saturating_sub(status_line.len()).min(chunk.len());
                status_line.extend_from_slice(&chunk[..wanted]);
                let mut seen = std::mem::take(tail);
                let before = seen.len();
                seen.extend_from_slice(chunk);
                match seen.windows(4).position(|w| w == b"\r\n\r\n") {
                    // the blank line ends inside this chunk
                    Some(at) => (Some(at + 4 - before), switches_protocols(status_line)),
                    None => {
                        *tail = seen[seen.len().saturating_sub(3)..].to_vec();
                        (None, false)
                    }
                }
            }
        };
        match frames_from {
            None => self.ready.extend_from_slice(chunk),
            Some(_) if !upgraded => {
                self.phase = Phase::Raw;
                self.ready.extend_from_slice(chunk);
            }
            Some(at) => {
                self.ready.extend_from_slice(&chunk[..at]);
                self.phase = Phase::Frames;
                self.inflater.feed(&chunk[at..], &mut self.ready).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
            }
        }
        Ok(())
    }
}

impl<S: AsyncRead + Unpin> AsyncRead for InflatingStream<S> {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        loop {
            if self.ready_at < self.ready.len() {
                let n = (self.ready.len() - self.ready_at).min(buf.remaining());
                let from = self.ready_at;
                buf.put_slice(&self.ready[from..from + n]);
                self.ready_at += n;
                if self.ready_at == self.ready.len() {
                    self.ready.clear();
                    self.ready_at = 0;
                }
                return Poll::Ready(Ok(()));
            }
            let mut chunk = [0u8; 16 * 1024];
            let mut read = ReadBuf::new(&mut chunk);
            match Pin::new(&mut self.inner).poll_read(cx, &mut read) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                Poll::Ready(Ok(())) => {}
            }
            if read.filled().is_empty() {
                return Poll::Ready(Ok(())); // end of stream
            }
            let filled = read.filled().to_vec();
            self.absorb(&filled)?;
        }
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for InflatingStream<S> {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}
