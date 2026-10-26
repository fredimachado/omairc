//! The session's STS flow: the upgrade from plaintext, the reconnect on the upgraded port, the
//! cached policy applied at start and reconnect, and the expiry refreshed on disconnect.
//! the `STS` section of `tui/internal/session/session.go` (Go) and of the STS methods of
//! `tui/internal/session/session.go`. The cache itself is `sts.rs`.

use super::events::{SessionState, Severity};
use super::session::{lifecycle_event, Core, Effect};
use super::sts::StsAdvertisement;

impl Core {
    /// Applies a cached policy for the configured host: it sets the port and turns TLS on, so a
    /// plaintext profile still connects securely (`applyCachedStsLocked`).
    pub(crate) fn apply_cached_sts(&mut self) {
        let now = self.now();
        let Some(cached) = self.sts.lookup(&self.config.host, now) else {
            return;
        };
        self.port = cached.port;
        self.tls_enabled = true;
    }

    /// Acts on an advertisement. A plaintext session with a port upgrades; a TLS session
    /// caches the duration, or clears the host on zero (`handleStsAdvertisementLocked`).
    pub(crate) fn handle_sts_advertisement(&mut self, advertisement: Option<StsAdvertisement>) {
        let Some(advertisement) = advertisement else {
            return;
        };
        if !self.tls_enabled {
            let Some(port) = advertisement.port else {
                return;
            };
            if self.state != SessionState::StsUpgrading {
                self.begin_sts_upgrade(port);
            }
            return;
        }
        let Some(duration) = advertisement.duration_seconds else {
            return;
        };
        if duration == 0 {
            self.sts.clear(&self.config.host);
            self.sts_duration = None;
            return;
        }
        let now = self.now();
        self.sts.store(&self.config.host, self.port, duration, now);
        self.sts_duration = Some(duration);
    }

    /// Moves to the upgraded port with TLS on, and tells the user (`beginStsUpgradeLocked`). The
    /// transport shuts down next; its disconnect reconnects on the new port.
    pub(crate) fn begin_sts_upgrade(&mut self, port: u16) {
        self.port = port;
        self.tls_enabled = true;
        self.set_state(SessionState::StsUpgrading);
        let network_id = self.network_id();
        let now = self.now();
        let text = format!("Upgraded to TLS on port {port}");
        let entry = lifecycle_event(&network_id, Severity::Info, "sts", &text, now);
        self.emit(entry);
        self.effects.push(Effect::Shutdown);
    }

    /// The plaintext socket closed for the upgrade: connect again at once on the upgraded
    /// port (`shouldFinishStsUpgrade` branch of `disconnectedLocked`).
    pub(crate) fn finish_sts_upgrade(&mut self) {
        self.reset_for_connection();
        self.connect_transport();
    }

    /// Refreshes the cached policy's expiry from now, for the duration last advertised over TLS.
    /// It runs on every disconnect and on stop, so a long session keeps its policy alive
    /// (`rescheduleStsExpiryLocked`).
    pub(crate) fn reschedule_sts_expiry(&mut self) {
        if !self.tls_enabled {
            return;
        }
        let Some(duration) = self.sts_duration else {
            return;
        };
        let now = self.now();
        self.sts.store(&self.config.host, self.port, duration, now);
    }
}
