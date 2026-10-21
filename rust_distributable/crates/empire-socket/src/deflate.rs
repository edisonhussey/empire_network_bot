//! `permessage-deflate` (RFC 7692), receive side only.
//!
//! The game server answers `400 Bad Request` to a WebSocket upgrade that does not offer a deflate extension, and then sends
//! its messages compressed (RSV1 set, raw deflate). `tungstenite` has no extension support and rejects RSV1, so this module sits
//! *under* it: it reads the server's frames, inflates the compressed messages and hands on ordinary, uncompressed, unfragmented
//! frames. What the client sends is not compressed, which the extension allows (RSV1 simply stays clear).
//!
//! Pure bytes in, bytes out: no sockets here, so every rule is a unit test.

use std::fmt;

use flate2::{Decompress, FlushDecompress, Status};

/// The largest frame, or inflated message, accepted. The game's biggest packets are a few hundred kilobytes.
pub const MAX_MESSAGE_BYTES: usize = 64 * 1024 * 1024;

/// What the extension appends to every message before it is deflated (RFC 7692 section 7.2.1); it is stripped on the wire.
const SYNC_TAIL: [u8; 4] = [0x00, 0x00, 0xff, 0xff];

#[derive(Debug)]
pub enum InflateError {
    TooLarge,
    Protocol(&'static str),
    Deflate(String),
}

impl fmt::Display for InflateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge => write!(f, "a WebSocket message is larger than {MAX_MESSAGE_BYTES} bytes"),
            Self::Protocol(why) => write!(f, "WebSocket framing error: {why}"),
            Self::Deflate(why) => write!(f, "permessage-deflate data is corrupt: {why}"),
        }
    }
}

impl std::error::Error for InflateError {}

struct Pending {
    opcode: u8,
    compressed: bool,
    data: Vec<u8>,
}

/// Turns the server's frame stream into plain frames. One per connection: the deflate window carries over between messages.
pub struct Inflater {
    input: Vec<u8>,
    pending: Option<Pending>,
    decompress: Decompress,
}

impl Default for Inflater {
    fn default() -> Self {
        Self::new()
    }
}

impl Inflater {
    pub fn new() -> Self {
        // Raw deflate, 15-bit window (what `permessage-deflate` with no parameters means). The window is kept for the whole
        // connection: if the server resets its own window each message, keeping ours is still correct.
        Self { input: Vec::new(), pending: None, decompress: Decompress::new(false) }
    }

    /// Feed bytes read from the server. Complete messages are appended to `out` as unmasked, unfragmented, uncompressed frames
    /// (control frames pass through as they came); a partial frame waits for the next call.
    pub fn feed(&mut self, bytes: &[u8], out: &mut Vec<u8>) -> Result<(), InflateError> {
        self.input.extend_from_slice(bytes);
        while let Some(head) = parse_header(&self.input)? {
            if head.payload_len > MAX_MESSAGE_BYTES as u64 {
                return Err(InflateError::TooLarge);
            }
            let total = head.header_len + head.payload_len as usize;
            if self.input.len() < total {
                break;
            }
            let frame: Vec<u8> = self.input.drain(..total).collect();
            let mut payload = frame[head.header_len..].to_vec();
            if let Some(key) = head.mask {
                for (i, byte) in payload.iter_mut().enumerate() {
                    *byte ^= key[i % 4];
                }
            }
            self.handle(&head, &frame, payload, out)?;
        }
        Ok(())
    }

    fn handle(&mut self, head: &Header, frame: &[u8], payload: Vec<u8>, out: &mut Vec<u8>) -> Result<(), InflateError> {
        match head.opcode {
            0x8..=0xf => out.extend_from_slice(frame), // close / ping / pong: never compressed, never fragmented
            0x1 | 0x2 => {
                if self.pending.is_some() {
                    return Err(InflateError::Protocol("a new message began inside a fragmented one"));
                }
                self.pending = Some(Pending { opcode: head.opcode, compressed: head.rsv1, data: payload });
                self.finish_if_last(head.fin, out)?;
            }
            0x0 => {
                let pending = self.pending.as_mut().ok_or(InflateError::Protocol("a continuation with nothing to continue"))?;
                if head.rsv1 {
                    return Err(InflateError::Protocol("RSV1 on a continuation frame"));
                }
                if pending.data.len() + payload.len() > MAX_MESSAGE_BYTES {
                    return Err(InflateError::TooLarge);
                }
                pending.data.extend_from_slice(&payload);
                self.finish_if_last(head.fin, out)?;
            }
            _ => return Err(InflateError::Protocol("reserved opcode")),
        }
        Ok(())
    }

    fn finish_if_last(&mut self, fin: bool, out: &mut Vec<u8>) -> Result<(), InflateError> {
        if !fin {
            return Ok(());
        }
        let Pending { opcode, compressed, data } = self.pending.take().expect("a message is pending when its last frame arrives");
        let message = if compressed { self.inflate(&data)? } else { data };
        write_frame(out, opcode, &message);
        Ok(())
    }

    fn inflate(&mut self, data: &[u8]) -> Result<Vec<u8>, InflateError> {
        let mut input = Vec::with_capacity(data.len() + SYNC_TAIL.len());
        input.extend_from_slice(data);
        input.extend_from_slice(&SYNC_TAIL);
        let mut out = Vec::with_capacity((data.len() * 4).max(256));
        let mut consumed = 0usize;
        loop {
            if out.len() == out.capacity() {
                out.reserve(out.len().max(4096));
            }
            let (in_before, out_before) = (self.decompress.total_in(), out.len());
            let status = self
                .decompress
                .decompress_vec(&input[consumed..], &mut out, FlushDecompress::Sync)
                .map_err(|error| InflateError::Deflate(error.to_string()))?;
            consumed += (self.decompress.total_in() - in_before) as usize;
            if out.len() > MAX_MESSAGE_BYTES {
                return Err(InflateError::TooLarge);
            }
            if matches!(status, Status::StreamEnd) {
                break;
            }
            // Finished when every input byte is used and the output buffer still had room (so nothing is waiting inside).
            if consumed >= input.len() && out.len() < out.capacity() {
                break;
            }
            if consumed >= input.len() && out.len() == out_before {
                break;
            }
        }
        Ok(out)
    }
}

struct Header {
    fin: bool,
    rsv1: bool,
    opcode: u8,
    mask: Option<[u8; 4]>,
    header_len: usize,
    payload_len: u64,
}

/// The header of the frame at the start of `buf`, or `None` when more bytes are needed to know it.
fn parse_header(buf: &[u8]) -> Result<Option<Header>, InflateError> {
    if buf.len() < 2 {
        return Ok(None);
    }
    let (b0, b1) = (buf[0], buf[1]);
    if b0 & 0x30 != 0 {
        return Err(InflateError::Protocol("RSV2 or RSV3 set"));
    }
    let (mut header_len, payload_len) = match b1 & 0x7f {
        126 => {
            if buf.len() < 4 {
                return Ok(None);
            }
            (4, u64::from(u16::from_be_bytes([buf[2], buf[3]])))
        }
        127 => {
            if buf.len() < 10 {
                return Ok(None);
            }
            (10, u64::from_be_bytes(buf[2..10].try_into().expect("eight bytes")))
        }
        n => (2, u64::from(n)),
    };
    let mask = if b1 & 0x80 != 0 {
        if buf.len() < header_len + 4 {
            return Ok(None);
        }
        let key = [buf[header_len], buf[header_len + 1], buf[header_len + 2], buf[header_len + 3]];
        header_len += 4;
        Some(key)
    } else {
        None
    };
    Ok(Some(Header { fin: b0 & 0x80 != 0, rsv1: b0 & 0x40 != 0, opcode: b0 & 0x0f, mask, header_len, payload_len }))
}

/// One unmasked, unfragmented, uncompressed frame.
fn write_frame(out: &mut Vec<u8>, opcode: u8, payload: &[u8]) {
    out.push(0x80 | opcode);
    match payload.len() {
        n if n < 126 => out.push(n as u8),
        n if n <= u16::MAX as usize => {
            out.push(126);
            out.extend_from_slice(&(n as u16).to_be_bytes());
        }
        n => {
            out.push(127);
            out.extend_from_slice(&(n as u64).to_be_bytes());
        }
    }
    out.extend_from_slice(payload);
}

#[cfg(test)]
mod tests {
    use flate2::{Compress, Compression, FlushCompress};

    use super::*;

    /// Compress one message the way a `permessage-deflate` sender does: sync flush, drop the 00 00 ff ff tail.
    fn deflate(compressor: &mut Compress, message: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(message.len() + 64);
        compressor.compress_vec(message, &mut out, FlushCompress::Sync).unwrap();
        assert!(out.ends_with(&SYNC_TAIL), "a sync flush ends with the tail");
        out.truncate(out.len() - SYNC_TAIL.len());
        out
    }

    fn frame(first_byte: u8, payload: &[u8]) -> Vec<u8> {
        let mut out = vec![first_byte];
        match payload.len() {
            n if n < 126 => out.push(n as u8),
            n if n <= u16::MAX as usize => { out.push(126); out.extend_from_slice(&(n as u16).to_be_bytes()); }
            n => { out.push(127); out.extend_from_slice(&(n as u64).to_be_bytes()); }
        }
        out.extend_from_slice(payload);
        out
    }

    fn compressed(opcode: u8, fin: bool, payload: &[u8]) -> Vec<u8> {
        frame((if fin { 0x80 } else { 0 }) | 0x40 | opcode, payload)
    }

    fn drain(inflater: &mut Inflater, bytes: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        inflater.feed(bytes, &mut out).unwrap();
        out
    }

    #[test]
    fn a_compressed_binary_message_comes_out_as_a_plain_frame() {
        let text = b"<msg t='sys'><body action='apiOK' r='0'></body></msg>";
        let mut compressor = Compress::new(Compression::default(), false);
        let wire = compressed(0x2, true, &deflate(&mut compressor, text));
        assert_eq!(drain(&mut Inflater::new(), &wire), frame(0x82, text));
    }

    #[test]
    fn the_window_carries_over_between_messages() {
        let mut compressor = Compress::new(Compression::default(), false);
        let mut inflater = Inflater::new();
        let first = b"%xt%EmpireEx_21%gbd%1%0%{\"gpi\":{\"UID\":1,\"PN\":\"someone\"}}%";
        let second = b"%xt%EmpireEx_21%gbd%1%0%{\"gpi\":{\"UID\":1,\"PN\":\"someone\"}}%"; // identical: compresses to a back-reference
        let one = compressed(0x1, true, &deflate(&mut compressor, first));
        let two = compressed(0x1, true, &deflate(&mut compressor, second));
        assert!(two.len() < one.len(), "the second message leans on the first's window");
        assert_eq!(drain(&mut inflater, &one), frame(0x81, first));
        assert_eq!(drain(&mut inflater, &two), frame(0x81, second));
    }

    #[test]
    fn frames_that_arrive_one_byte_at_a_time_are_still_assembled() {
        let text = vec![b'x'; 70_000]; // needs the 8-byte length form once compressed? (no) but the plain frame needs it
        let mut compressor = Compress::new(Compression::default(), false);
        let wire = compressed(0x2, true, &deflate(&mut compressor, &text));
        let mut inflater = Inflater::new();
        let mut out = Vec::new();
        for byte in &wire {
            inflater.feed(std::slice::from_ref(byte), &mut out).unwrap();
        }
        assert_eq!(out, frame(0x82, &text));
    }

    #[test]
    fn a_fragmented_compressed_message_is_joined() {
        let text = b"one message sent in three pieces, deflated as a whole";
        let mut compressor = Compress::new(Compression::default(), false);
        let body = deflate(&mut compressor, text);
        let (a, rest) = body.split_at(body.len() / 3);
        let (b, c) = rest.split_at(rest.len() / 2);
        let mut wire = compressed(0x1, false, a);
        wire.extend(frame(0x00, b));        // continuation, RSV1 clear
        wire.extend(frame(0x80, c));        // last continuation
        assert_eq!(drain(&mut Inflater::new(), &wire), frame(0x81, text));
    }

    #[test]
    fn a_ping_between_fragments_passes_straight_through() {
        let text = b"message split by a ping";
        let mut compressor = Compress::new(Compression::default(), false);
        let body = deflate(&mut compressor, text);
        let (a, b) = body.split_at(body.len() / 2);
        let ping = frame(0x89, b"hi");
        let mut wire = compressed(0x1, false, a);
        wire.extend(&ping);
        wire.extend(frame(0x80, b));
        let out = drain(&mut Inflater::new(), &wire);
        let mut expected = ping.clone();
        expected.extend(frame(0x81, text));
        assert_eq!(out, expected);
    }

    #[test]
    fn uncompressed_messages_are_re_framed_unchanged() {
        let wire = frame(0x81, b"plain text");
        assert_eq!(drain(&mut Inflater::new(), &wire), wire);
        let close = frame(0x88, &[0x03, 0xe8]);
        assert_eq!(drain(&mut Inflater::new(), &close), close);
    }

    #[test]
    fn a_masked_frame_is_unmasked() {
        let key = [1u8, 2, 3, 4];
        let masked: Vec<u8> = b"hello".iter().enumerate().map(|(i, b)| b ^ key[i % 4]).collect();
        let mut wire = vec![0x81, 0x80 | 5];
        wire.extend_from_slice(&key);
        wire.extend_from_slice(&masked);
        assert_eq!(drain(&mut Inflater::new(), &wire), frame(0x81, b"hello"));
    }

    #[test]
    fn broken_input_is_an_error_not_a_panic() {
        let mut out = Vec::new();
        assert!(Inflater::new().feed(&frame(0xf1, b"x"), &mut out).is_err()); // reserved opcode
        assert!(Inflater::new().feed(&frame(0x80, b"x"), &mut out).is_err()); // continuation with nothing before it
        assert!(Inflater::new().feed(&frame(0x91, b"x"), &mut out).is_err()); // RSV3
        let garbage = compressed(0x1, true, &[0xde, 0xad, 0xbe, 0xef, 0x01]);
        assert!(Inflater::new().feed(&garbage, &mut out).is_err());
    }

    #[test]
    fn a_huge_declared_length_is_refused_before_buffering_it() {
        let mut wire = vec![0x82, 127];
        wire.extend_from_slice(&u64::MAX.to_be_bytes());
        assert!(matches!(Inflater::new().feed(&wire, &mut Vec::new()), Err(InflateError::TooLarge)));
    }
}
