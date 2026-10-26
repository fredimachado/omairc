//! Polling helpers for the live harness (Qt `waitUntil`).

use std::thread;
use std::time::{Duration, Instant};

use crate::irc::message::IrcMessage;

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(20);

/// Spins `predicate` until it returns true or `timeout` elapses.
pub fn wait_until(mut predicate: impl FnMut() -> bool, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if predicate() {
            return true;
        }
        thread::sleep(Duration::from_millis(20));
    }
    predicate()
}

pub fn message_has_command(messages: &[IrcMessage], command: &str) -> bool {
    messages
        .iter()
        .any(|message| message.command.eq_ignore_ascii_case(command))
}

pub fn message_command_count(messages: &[IrcMessage], command: &str) -> usize {
    messages
        .iter()
        .filter(|message| message.command.eq_ignore_ascii_case(command))
        .count()
}
