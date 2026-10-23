//! Desktop notifications on macOS. When the process has no bundle identifier (`cargo run`),
//! notifications are disabled. Otherwise `osascript` shows a banner; activation is not
//! available through `display notification`, matching the TUI stub until a bundled build
//! wires `UNUserNotificationCenter`.

use std::process::Command;
use std::thread;

use async_channel::Sender;

use super::{Activation, Notifier, NullNotifier};

struct OsascriptNotifier;

impl Notifier for OsascriptNotifier {
    fn notify(
        &self,
        summary: &str,
        body: &str,
        _network_id: &str,
        _target: &str,
        _msgid: &str,
    ) {
        let summary = escape_applescript(summary);
        let body = escape_applescript(body);
        let script = format!(
            "display notification \"{body}\" with title \"{summary}\""
        );
        thread::spawn(move || {
            let _ = Command::new("osascript").arg("-e").arg(script).status();
        });
    }
}

fn escape_applescript(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn bundle_identifier() -> String {
    Command::new("osascript")
        .args([
            "-e",
            "tell application \"System Events\" to get bundle identifier of (path to me)",
        ])
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                Some(String::from_utf8_lossy(&output.stdout).trim().to_owned())
            } else {
                None
            }
        })
        .filter(|id| !id.is_empty() && id != "missing value")
        .unwrap_or_default()
}

pub fn open(_tx: Sender<Activation>) -> Box<dyn Notifier> {
    if bundle_identifier().is_empty() {
        return Box::new(NullNotifier);
    }
    if std::env::var("OMAIRC_SKIP_NOTIFICATIONS").is_ok_and(|value| !value.is_empty()) {
        return Box::new(NullNotifier);
    }
    Box::new(OsascriptNotifier)
}
