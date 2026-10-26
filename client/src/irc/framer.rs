//! Port of `tui/internal/irc/framer.go` (Go) and `src/irc/ircframer.{h,cpp}` (C++).
//!
//! `IrcFramer` cuts a byte stream into CRLF-terminated frames. It keeps partial
//! frames across `feed` calls, drops frames that break the NUL or length rules
//! and reports each one as a fault. While it discards a bad frame it keeps a
//! trailing '\r', so a CRLF split across two reads still ends the discard.
//!
//! Frames are raw bytes. Faults keep at most `FAULT_PREVIEW_BYTES` of the bad
//! frame.

use super::message::{IrcError, MAX_INBOUND_CLASSIC_FRAME_BYTES, MAX_TAG_SECTION_BYTES};

/// How much of a bad frame a fault keeps for a Status line.
pub const FAULT_PREVIEW_BYTES: usize = 160;

/// One rejected frame (`IrcFrameFault`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IrcFrameFault {
    pub error: IrcError,
    pub preview: Vec<u8>,
    pub byte_count: usize,
}

/// The complete frames and faults one `feed` call resolved (`IrcFrameResult`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IrcFrameResult {
    pub frames: Vec<Vec<u8>>,
    pub faults: Vec<IrcFrameFault>,
}

/// Reassembles frames from a byte stream (`IrcFramer`).
#[derive(Clone, Debug, Default)]
pub struct IrcFramer {
    /// Bytes not yet resolved into a frame, a fault, or a discard.
    pending: Vec<u8>,
    discarding: bool,
}

impl IrcFramer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends `incoming` and returns every frame and fault it can now resolve.
    pub fn feed(&mut self, incoming: &[u8]) -> IrcFrameResult {
        let mut result = IrcFrameResult::default();
        let mut data = std::mem::take(&mut self.pending);
        data.extend_from_slice(incoming);
        let mut position = 0;

        loop {
            let rest = &data[position..];

            if self.discarding {
                match find_crlf(rest) {
                    Some(delimiter) => {
                        position += delimiter + 2;
                        self.discarding = false;
                    }
                    None => {
                        self.pending = retained_carriage_return(rest);
                        return result;
                    }
                }
                continue;
            }

            if let Some(delimiter) = find_crlf(rest) {
                let frame = &rest[..delimiter];
                if frame.contains(&0) {
                    record_fault(&mut result, IrcError::InvalidCharacter, frame);
                } else if !complete_frame_is_valid(frame) {
                    record_fault(&mut result, IrcError::TooManyBytes, frame);
                } else {
                    result.frames.push(frame.to_vec());
                }
                position += delimiter + 2;
                continue;
            }

            let fault = if rest.contains(&0) {
                Some(IrcError::InvalidCharacter)
            } else if pending_frame_is_overlong(rest) {
                Some(IrcError::TooManyBytes)
            } else {
                None
            };
            let Some(error) = fault else {
                self.pending = rest.to_vec();
                return result;
            };

            record_fault(&mut result, error, rest);
            self.discarding = true;
            let keep_carriage_return = rest.last() == Some(&b'\r');
            data.clear();
            position = 0;
            if keep_carriage_return {
                data.push(b'\r');
            }
        }
    }
}

fn find_crlf(bytes: &[u8]) -> Option<usize> {
    bytes.windows(2).position(|pair| pair == b"\r\n")
}

fn retained_carriage_return(rest: &[u8]) -> Vec<u8> {
    if rest.last() == Some(&b'\r') {
        vec![b'\r']
    } else {
        Vec::new()
    }
}

fn record_fault(result: &mut IrcFrameResult, error: IrcError, raw: &[u8]) {
    let take = raw.len().min(FAULT_PREVIEW_BYTES);
    result.faults.push(IrcFrameFault {
        error,
        preview: raw[..take].to_vec(),
        byte_count: raw.len(),
    });
}

/// `frame` is the complete frame without its CRLF. A tagged frame is limited
/// twice: its tag section, and the classic part after the tags, CRLF included.
fn complete_frame_is_valid(frame: &[u8]) -> bool {
    let plain_fits = frame.len() + 2 <= MAX_INBOUND_CLASSIC_FRAME_BYTES;
    if frame.first() != Some(&b'@') {
        return plain_fits;
    }
    match frame.iter().position(|&byte| byte == b' ') {
        None => plain_fits,
        Some(space) => {
            let tag_payload_bytes = space - 1;
            let classic_frame_bytes = frame.len() - space - 1 + 2;
            tag_payload_bytes <= MAX_TAG_SECTION_BYTES
                && classic_frame_bytes <= MAX_INBOUND_CLASSIC_FRAME_BYTES
        }
    }
}

/// True when the unresolved bytes can no longer become a valid frame.
fn pending_frame_is_overlong(rest: &[u8]) -> bool {
    if rest.first() != Some(&b'@') {
        return rest.len() >= MAX_INBOUND_CLASSIC_FRAME_BYTES;
    }
    match rest.iter().position(|&byte| byte == b' ') {
        None => rest.len() > 1 + MAX_TAG_SECTION_BYTES,
        Some(space) => {
            if space - 1 > MAX_TAG_SECTION_BYTES {
                return true;
            }
            rest.len() - space - 1 >= MAX_INBOUND_CLASSIC_FRAME_BYTES
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::irc::message::MAX_CLASSIC_FRAME_BYTES;
    use crate::irc::parser::parse;

    #[test]
    fn frames_latin1_then_utf8() {
        let mut framer = IrcFramer::new();
        let mut wire = b":a!u@h PRIVMSG #c :".to_vec();
        wire.push(0xe9);
        wire.extend_from_slice(b"\r\n:b!u@h PRIVMSG #c :ok\r\n");

        let result = framer.feed(&wire);
        assert!(result.faults.is_empty(), "faults = {:?}", result.faults);
        assert_eq!(result.frames.len(), 2);

        let first = parse(&result.frames[0]).expect("first frame parses");
        let second = parse(&result.frames[1]).expect("second frame parses");
        assert_eq!(first.params[1], vec![0xe9]);
        assert_eq!(second.params[1], b"ok".to_vec());
    }

    #[test]
    fn frames_fragmented_and_coalesced_input() {
        let mut framer = IrcFramer::new();
        assert!(framer.feed(b"PIN").frames.is_empty());
        assert!(framer.feed(b"G :token").frames.is_empty());

        let result = framer.feed(b"\r\n");
        assert_eq!(result.frames, vec![b"PING :token".to_vec()]);

        let result = framer.feed(b"PING :one\r\nPING :two\r\n");
        assert_eq!(result.frames, vec![b"PING :one".to_vec(), b"PING :two".to_vec()]);
    }

    #[test]
    fn rejects_nul_and_recovers() {
        let mut framer = IrcFramer::new();
        let mut wire = b"PRIVMSG #c :hel".to_vec();
        wire.push(0);
        wire.extend_from_slice(b"lo\r\nPING :ok\r\n");

        let result = framer.feed(&wire);
        assert_eq!(result.faults.len(), 1, "faults = {:?}", result.faults);
        assert_eq!(result.faults[0].error, IrcError::InvalidCharacter);
        assert_eq!(result.faults[0].byte_count, 18);
        assert_eq!(result.frames, vec![b"PING :ok".to_vec()]);
    }

    #[test]
    fn rejects_overlong_and_recovers() {
        let mut framer = IrcFramer::new();
        let overlong_bytes = MAX_INBOUND_CLASSIC_FRAME_BYTES - 1;
        let mut wire = vec![b'X'; overlong_bytes];
        wire.extend_from_slice(b"\r\nPING :complete\r\n");

        let result = framer.feed(&wire);
        assert_eq!(result.faults.len(), 1, "faults = {:?}", result.faults);
        assert_eq!(result.faults[0].error, IrcError::TooManyBytes);
        assert_eq!(result.faults[0].byte_count, overlong_bytes);
        assert_eq!(result.faults[0].preview, vec![b'X'; 160]);
        assert_eq!(result.frames, vec![b"PING :complete".to_vec()]);

        let result = framer.feed(&vec![b'Y'; MAX_INBOUND_CLASSIC_FRAME_BYTES]);
        assert_eq!(result.faults.len(), 1, "faults = {:?}", result.faults);
        assert!(result.frames.is_empty());

        let result = framer.feed(b"tail\r\nPING :after\r\n");
        assert_eq!(result.frames, vec![b"PING :after".to_vec()]);
    }

    #[test]
    fn discard_keeps_a_split_crlf() {
        let mut framer = IrcFramer::new();
        let result = framer.feed(&vec![b'Z'; MAX_INBOUND_CLASSIC_FRAME_BYTES + 10]);
        assert_eq!(result.faults.len(), 1);
        assert!(result.frames.is_empty());

        assert!(framer.feed(b"junk\r").frames.is_empty());
        let result = framer.feed(b"\nPING :next\r\n");
        assert_eq!(result.frames, vec![b"PING :next".to_vec()]);
    }

    #[test]
    fn enforces_classic_frame_boundary() {
        let mut framer = IrcFramer::new();
        let frame_with_body = |body: usize| {
            let mut frame = b"PING :".to_vec();
            frame.extend(vec![b'z'; body]);
            frame.extend_from_slice(b"\r\n");
            frame
        };

        let spec = frame_with_body(504);
        assert_eq!(spec.len(), MAX_CLASSIC_FRAME_BYTES);
        let result = framer.feed(&spec);
        assert!(result.frames.len() == 1 && result.faults.is_empty(), "{result:?}");

        let over_spec = frame_with_body(505);
        let result = framer.feed(&over_spec);
        assert!(result.frames.len() == 1 && result.faults.is_empty(), "{result:?}");

        let exact = frame_with_body(MAX_INBOUND_CLASSIC_FRAME_BYTES - "PING :".len() - 2);
        assert_eq!(exact.len(), MAX_INBOUND_CLASSIC_FRAME_BYTES);
        let result = framer.feed(&exact);
        assert!(result.frames.len() == 1 && result.faults.is_empty(), "{result:?}");

        let mut too_long = frame_with_body(MAX_INBOUND_CLASSIC_FRAME_BYTES - "PING :".len() - 1);
        too_long.truncate(too_long.len() - 2);
        too_long.extend_from_slice(b"\r\nPING :short\r\n");
        let result = framer.feed(&too_long);
        assert_eq!(result.faults.len(), 1, "faults = {:?}", result.faults);
        assert_eq!(result.faults[0].error, IrcError::TooManyBytes);
        assert_eq!(result.faults[0].byte_count, MAX_INBOUND_CLASSIC_FRAME_BYTES - 1);
        assert!(result.faults[0].preview.starts_with(b"PING :"));
        assert_eq!(result.frames, vec![b"PING :short".to_vec()]);
    }

    #[test]
    fn accepts_tagged_classic_frame() {
        let mut framer = IrcFramer::new();
        let mut tagged = b"@label=123 PING :".to_vec();
        tagged.extend(vec![b'z'; 504]);
        tagged.extend_from_slice(b"\r\n");

        let result = framer.feed(&tagged);
        assert!(result.faults.is_empty(), "faults = {:?}", result.faults);
        assert_eq!(result.frames.len(), 1);
        assert!(parse(&result.frames[0]).is_ok());
    }

    #[test]
    fn enforces_tag_section_boundary() {
        let mut framer = IrcFramer::new();
        let mut exact = b"@".to_vec();
        exact.extend(vec![b'a'; MAX_TAG_SECTION_BYTES]);
        exact.extend_from_slice(b" PING :ok\r\n");

        let result = framer.feed(&exact);
        assert!(result.faults.is_empty(), "faults = {:?}", result.faults);
        assert_eq!(result.frames.len(), 1);
        assert!(parse(&result.frames[0]).is_ok());

        let mut too_long = b"@".to_vec();
        too_long.extend(vec![b'a'; MAX_TAG_SECTION_BYTES + 1]);
        too_long.extend_from_slice(b" PING :no\r\nPING :recovered\r\n");
        let result = framer.feed(&too_long);
        assert_eq!(result.faults.len(), 1, "faults = {:?}", result.faults);
        assert_eq!(result.frames, vec![b"PING :recovered".to_vec()]);
    }
}
