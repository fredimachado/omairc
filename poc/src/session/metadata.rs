//! Port of the draft/metadata-2 writers of `tui/internal/session/session.go` (Go),
//! `IrcSession::setOwnMetadata` in `src/irc/ircsession.cpp` and
//! `src/irc/ircmetadata.*` (C++).
//!
//! The vocabulary the session reads (the subscribed keys, the limits the server
//! advertised, the value clamp, the CAP token parse) is pure and lives in
//! `irc/presence`. It is re-exported here so the session and its tests keep their
//! paths. What stays in this file is the session's own state machine: subscribing
//! once batch and member metadata are both enabled, the `SET` writer, and the
//! `774` retry.

use super::session::Core;
use super::wire;
use crate::irc::capability::IrcCapability;
use crate::irc::message::MAX_CLASSIC_FRAME_BYTES;

pub use crate::irc::presence::{
    canonical_key, clamped_to, effective_max_value_bytes, is_known_key, parse_metadata_capability,
    subscription_keys, MetadataCapability,
};

impl Core {
    /// Subscribes to the metadata keys the server allows, once member metadata
    /// and batch are both enabled (`subscribeToMemberMetadataLocked`).
    pub(crate) fn subscribe_to_member_metadata(&mut self) {
        let enabled = self.caps.enabled();
        if !enabled.contains(IrcCapability::MemberMetadata)
            || !enabled.contains(IrcCapability::Batch)
        {
            return;
        }
        let keys = subscription_keys(self.metadata.max_subs);
        if keys.is_empty() {
            return;
        }
        let line = format!("METADATA * SUB {}\r\n", keys.join(" "));
        self.send_line(line.as_bytes());
    }

    /// Sets (or, for an empty value, clears) one of this user's metadata keys.
    /// Returns the value placed on the wire, `Some("")` for a clear, or `None`
    /// when nothing was sent (`setOwnMetadataLocked`).
    pub(crate) fn set_own_metadata(&mut self, key: &str, value: &str) -> Option<String> {
        if self.state != super::events::SessionState::Registered {
            return None;
        }
        let enabled = self.caps.enabled();
        if !enabled.contains(IrcCapability::MemberMetadata)
            || !enabled.contains(IrcCapability::Batch)
        {
            return None;
        }
        if key.is_empty() || !is_known_key(key) {
            return None;
        }
        let stored = canonical_key(key)?;
        if value.is_empty() {
            if !self.send_command(&format!("METADATA * SET {stored}"), "") {
                return None;
            }
            return Some(String::new());
        }
        let prefix = format!("METADATA * SET {stored} :");
        let wire_budget = MAX_CLASSIC_FRAME_BYTES
            .saturating_sub(prefix.len())
            .saturating_sub(2);
        let max_bytes = effective_max_value_bytes(self.metadata.max_value_bytes).min(wire_budget);
        if max_bytes == 0 {
            return None;
        }
        let clamped = clamped_to(value, max_bytes);
        if clamped.is_empty() {
            return None;
        }
        if !self.send_command(&format!("{prefix}{clamped}"), "") {
            return None;
        }
        Some(clamped)
    }

    /// Handles `774`, which asks the client to retry a metadata sync later. The
    /// retry is capped at 60 seconds (`handleMetadataSyncLaterLocked`).
    pub(crate) fn handle_metadata_sync_later(&mut self, message: &crate::irc::message::IrcMessage) {
        let target = wire::param(message, 1);
        if target.is_empty() {
            return;
        }
        let retry_after = wire::param(message, 2)
            .trim()
            .parse::<i64>()
            .unwrap_or(0)
            .clamp(0, 60);
        let weak = self.weak.clone();
        let delay = std::time::Duration::from_secs(u64::try_from(retry_after).unwrap_or(0));
        let _ = self.clock.after(
            delay,
            Box::new(move || {
                if let Some(shared) = weak.upgrade() {
                    super::session::Session::from_shared(shared).locked(|core| {
                        core.send_command(&format!("METADATA {target} SYNC"), "");
                    });
                }
            }),
        );
    }
}
