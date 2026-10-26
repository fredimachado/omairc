//! Port of `tui/internal/session/config.go` in
//! `tui/internal/session/session.go`.
//!
//! One network's identity, credentials, autojoin state and timer policy. Build
//! the defaults with `SessionConfig::with_defaults`, adjust the fields, then pass
//! the config to `Session::new`. The controller's persistence layer fills the fields
//! from the saved Connect profile.

use std::collections::BTreeMap;

use crate::util::Secret;

/// The per-network session settings.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionConfig {
    pub network_id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub tls_enabled: bool,

    pub nick: String,
    pub username: String,
    pub realname: String,
    /// The server password (`PASS`).
    pub password: Secret,
    /// The NickServ password, sent as `IDENTIFY` after registration.
    pub nickserv_password: Secret,
    /// The SASL account, or empty to use the nick.
    pub sasl_account: String,

    /// Channels joined after registration, in profile order.
    pub autojoin_channels: Vec<String>,
    /// Channel keys by channel name. A `BTreeMap` keeps iteration deterministic.
    pub autojoin_keys: BTreeMap<String, String>,

    pub reconnect_enabled: bool,
    pub reconnect_base_delay_ms: u64,
    pub reconnect_maximum_delay_ms: u64,
    /// How long to wait for CAP ACK / NAK before giving up on negotiation.
    pub capability_timeout_ms: u64,
    /// The ping watchdog interval: silence this long triggers a PING probe.
    pub ping_timeout_ms: u64,
    /// How long a labeled request may wait for its response.
    pub labeled_response_timeout_ms: u64,
}

impl SessionConfig {
    /// The defaults of `IrcSessionConfig`: port 6697, TLS on, reconnect on,
    /// 1000 ms base backoff doubling to 30000 ms, a 10000 ms capability timeout,
    /// a 60000 ms ping watchdog, and a 45000 ms labeled-response timeout.
    /// Username and realname fall back to the nick, as
    /// `IrcConnection::sessionConfigFor` does.
    pub fn with_defaults(network_id: &str, name: &str, host: &str, nick: &str) -> Self {
        Self {
            network_id: network_id.to_owned(),
            name: name.to_owned(),
            host: host.to_owned(),
            port: 6697,
            tls_enabled: true,
            nick: nick.to_owned(),
            username: nick.to_owned(),
            realname: nick.to_owned(),
            reconnect_enabled: true,
            reconnect_base_delay_ms: 1000,
            reconnect_maximum_delay_ms: 30000,
            capability_timeout_ms: 10000,
            ping_timeout_ms: 60000,
            labeled_response_timeout_ms: 45000,
            ..Self::default()
        }
    }

    /// The SASL account name. An empty `sasl_account` falls back to the nick,
    /// as `IrcNetworkProfile::saslAccount()` does.
    pub fn sasl_account_name(&self) -> &str {
        if self.sasl_account.is_empty() {
            &self.nick
        } else {
            &self.sasl_account
        }
    }

    /// The credential SASL sends: the NickServ password when both are set,
    /// otherwise the server password (`saslSecret` in `tui/internal/session/session.go`).
    pub(crate) fn sasl_secret(&self) -> &Secret {
        if self.nickserv_password.is_empty() {
            &self.password
        } else {
            &self.nickserv_password
        }
    }

    /// The display name: the trimmed configured name, else the trimmed host
    /// (`IrcNetworkProfile::resolvedName`).
    pub fn resolved_name(&self) -> String {
        let trimmed = self.name.trim();
        if trimmed.is_empty() {
            self.host.trim().to_owned()
        } else {
            trimmed.to_owned()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_prints_a_seeded_password() {
        let mut config = SessionConfig::with_defaults("net", "Net", "irc.example", "fred");
        config.password = "server-pass-7".to_owned().into();
        config.nickserv_password = "nickserv-pass-9".to_owned().into();
        let text = format!("{config:?}");
        assert!(!text.contains("server-pass-7"), "{text}");
        assert!(!text.contains("nickserv-pass-9"), "{text}");
        assert!(text.contains("Secret(<redacted>)"), "{text}");
    }

    #[test]
    fn sasl_uses_the_nickserv_password_when_both_are_set() {
        let mut config = SessionConfig::with_defaults("net", "Net", "irc.example", "fred");
        config.password = "server-pass".to_owned().into();
        assert_eq!(config.sasl_secret().expose(), "server-pass");
        config.nickserv_password = "nick-pass".to_owned().into();
        assert_eq!(config.sasl_secret().expose(), "nick-pass");
    }
}
