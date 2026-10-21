//! Port of `tui/internal/notify/notify.go` (Go) and `Backend::notifyDesktop` in `src/backend.cpp` (C++).
//!
//! The seam the controller calls when a mention arrives. The PoC notifier does
//! nothing until Phase 8 adds desktop notifications, so the controller can be
//! built and tested with `NullNotifier` and no platform code.

use crate::irc::event::MentionArrival;

/// Raises a desktop notification for an arrival the reducer hands up.
pub trait Notifier {
    /// Called once per mention, after the reducer has folded the line in.
    fn mention(&self, arrival: &MentionArrival);
}

/// The notifier that does nothing.
#[derive(Clone, Copy, Debug, Default)]
pub struct NullNotifier;

impl Notifier for NullNotifier {
    fn mention(&self, _arrival: &MentionArrival) {}
}
