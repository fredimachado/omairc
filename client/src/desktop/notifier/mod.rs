//! The shell owns the platform notifier; the controller only queues `Arrival`s for
//! `take_arrivals`. `--demo-server` raises notifications like a live launch, as the Qt client
//! does. Setting `OMAIRC_SKIP_NOTIFICATIONS` to a non-empty value turns them off.

mod util;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
mod windows_ownership;

#[cfg(windows)]
pub use windows::activation_from_args;

use async_channel::Sender;

/// Identifies the conversation a notification belongs to when the user activates it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Activation {
    pub network_id: String,
    pub target: String,
    pub msgid: String,
}

/// Raises a desktop notification. Implementations must not block the caller.
pub trait Notifier: Send {
    fn notify(
        &self,
        summary: &str,
        body: &str,
        network_id: &str,
        target: &str,
        msgid: &str,
    );
}

/// The notifier that does nothing.
#[derive(Clone, Copy, Debug, Default)]
pub struct NullNotifier;

impl Notifier for NullNotifier {
    fn notify(
        &self,
        _summary: &str,
        _body: &str,
        _network_id: &str,
        _target: &str,
        _msgid: &str,
    ) {
    }
}

/// Whether desktop notifications may run for this launch. `OMAIRC_SKIP_NOTIFICATIONS`, when
/// set to a non-empty value, turns them off.
pub fn notifications_allowed() -> bool {
    !std::env::var("OMAIRC_SKIP_NOTIFICATIONS").is_ok_and(|value| !value.is_empty())
}

/// The platform notifier, or [`NullNotifier`] when notifications are skipped or unavailable.
/// Activations are sent on `tx` from a background thread or delegate.
pub fn platform_notifier(tx: Sender<Activation>) -> Box<dyn Notifier> {
    if !notifications_allowed() {
        return Box::new(NullNotifier);
    }
    platform_notifier_inner(tx)
}

#[cfg(target_os = "linux")]
fn platform_notifier_inner(tx: Sender<Activation>) -> Box<dyn Notifier> {
    linux::open(tx)
}

#[cfg(target_os = "macos")]
fn platform_notifier_inner(tx: Sender<Activation>) -> Box<dyn Notifier> {
    macos::open(tx)
}

#[cfg(windows)]
fn platform_notifier_inner(tx: Sender<Activation>) -> Box<dyn Notifier> {
    windows::open(tx)
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn platform_notifier_inner(tx: Sender<Activation>) -> Box<dyn Notifier> {
    drop(tx);
    Box::new(NullNotifier)
}
