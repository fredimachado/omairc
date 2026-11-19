//! `draft/multiline` receive (`tui/internal/session/multiline.go`). A multiline
//! batch is buffered whole and delivered as one synthetic PRIVMSG or NOTICE
//! (`assemble_multiline`), through the path any single line takes: live as
//! `MessageReceived`, or into the replay root's collected lines when the batch is
//! nested in CHATHISTORY. Nothing downstream knows about batches.

use super::batch::OpenBatch;
use super::session::Core;
use crate::irc::capability::IrcCapability;
use crate::irc::message::{IrcMessage, IrcTag};
use crate::irc::multiline::{assemble_multiline, MULTILINE_MAX_BYTES, MULTILINE_MAX_LINES};

/// The lines one open multiline batch has buffered.
#[derive(Clone, Debug)]
pub(crate) struct MultilineBuffer {
    /// The `BATCH +ref draft/multiline <target>` line, which carries the tags.
    pub(crate) open: IrcMessage,
    /// The raw tagged lines, at most `MULTILINE_MAX_LINES`.
    pub(crate) lines: Vec<IrcMessage>,
    /// The raw text bytes of `lines`, at most `MULTILINE_MAX_BYTES`.
    pub(crate) bytes: usize,
    /// A ceiling was hit: the buffered part was delivered, and the rest of the
    /// batch is delivered line by line.
    pub(crate) overflowed: bool,
}

impl MultilineBuffer {
    fn new(open: IrcMessage) -> Self {
        Self {
            open,
            lines: Vec::new(),
            bytes: 0,
            overflowed: false,
        }
    }
}

impl Core {
    /// Whether a multiline batch is assembled: it needs `batch` to be a batch
    /// and `message-tags` for the `batch` and concat tags on its lines
    /// (`multilineEnabledLocked`).
    pub(crate) fn multiline_enabled(&self) -> bool {
        let enabled = self.caps.enabled();
        enabled.contains(IrcCapability::Batch)
            && enabled.contains(IrcCapability::MessageTags)
            && enabled.contains(IrcCapability::Multiline)
    }

    /// Starts buffering a frame that `BATCH +ref draft/multiline` opened
    /// (`openMultilineLocked`).
    pub(crate) fn open_multiline(&self, frame: &mut OpenBatch, open: &IrcMessage) {
        if self.multiline_enabled() {
            frame.multiline = Some(MultilineBuffer::new(open.clone()));
        }
    }

    /// Buffers one line of an open multiline batch (`captureMultilineLocked`).
    /// Returns true when `reference` is one, and the line is therefore swallowed.
    pub(crate) fn capture_multiline(&mut self, reference: &str, line: &IrcMessage) -> bool {
        let Some(frame) = self.batches.open.get_mut(reference) else {
            return false;
        };
        let root = frame.replay_root.clone();
        let Some(buffer) = frame.multiline.as_mut() else {
            return false;
        };
        let command = line.command.to_ascii_uppercase();
        if command != "PRIVMSG" && command != "NOTICE" {
            return true;
        }

        let size = line.param(1).map_or(0, <[u8]>::len);
        let mut deliver = Vec::new();
        let full = buffer.lines.len() >= MULTILINE_MAX_LINES
            || buffer.bytes + size > MULTILINE_MAX_BYTES;
        if full && !buffer.overflowed {
            deliver = pieces(&buffer.open, std::mem::take(&mut buffer.lines));
            buffer.bytes = 0;
            buffer.overflowed = true;
        }
        if buffer.overflowed {
            deliver.push(without_batch_tag(line));
        } else {
            buffer.bytes += size;
            buffer.lines.push(line.clone());
        }
        for message in deliver {
            self.deliver_assembled(&root, message);
        }
        true
    }

    /// Delivers a closed multiline batch (`finishMultilineLocked`). `root` is
    /// the replay root the batch sat in, or empty for a live batch.
    pub(crate) fn finish_multiline(&mut self, root: &str, buffer: MultilineBuffer) {
        for message in pieces(&buffer.open, buffer.lines) {
            self.deliver_assembled(root, message);
        }
    }

    /// One message out of a multiline batch: into the collected lines of the replay
    /// it sits in, or live (`deliverAssembledLocked`).
    fn deliver_assembled(&mut self, root: &str, message: IrcMessage) {
        if root.is_empty() {
            self.emit_message_received(message);
            return;
        }
        // A replay root that is gone took its children with it. Delivering the
        // line live would replay history as new traffic.
        if let Some(root) = self.batches.open.get_mut(root) {
            root.push_collected(message);
        }
    }

    /// Delivers every live multiline batch that never closed, because the
    /// connection ended first (`flushOpenMultilineLocked`). One nested in a replay
    /// is dropped with its unclosed root, as its siblings are. Each buffer is
    /// emptied as it is flushed, so a second call delivers nothing.
    pub(crate) fn flush_open_multiline(&mut self) {
        let unclosed: Vec<MultilineBuffer> = self
            .batches
            .open
            .values_mut()
            .filter(|frame| frame.replay_root.is_empty())
            .filter_map(|frame| frame.multiline.take())
            .collect();
        for buffer in unclosed {
            self.finish_multiline("", buffer);
        }
    }
}

/// The message a batch assembles to, or its lines one by one when they do not
/// assemble (blank, or CTCP, which the spec does not allow in a batch).
fn pieces(open: &IrcMessage, lines: Vec<IrcMessage>) -> Vec<IrcMessage> {
    if lines.is_empty() {
        return Vec::new();
    }
    match assemble_multiline(open, &lines) {
        Some(message) => vec![message],
        None => lines.iter().map(without_batch_tag).collect(),
    }
}

fn without_batch_tag(line: &IrcMessage) -> IrcMessage {
    let tags: Vec<IrcTag> = line
        .tags
        .iter()
        .filter(|tag| tag.name != "batch")
        .cloned()
        .collect();
    IrcMessage {
        tags,
        ..line.clone()
    }
}
