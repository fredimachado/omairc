//! Reads the same `OMAIRC_LIVE_*` variables `bin/test-live` exports.

use std::fs;
use std::path::PathBuf;

use crate::irc::casemapping::IrcCaseMappingKind;

/// Expected CASEMAPPING for a daemon fixture (matches Qt `LiveDaemonInfo`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaseMappingExpect {
    Ascii,
    Rfc1459,
}

impl CaseMappingExpect {
    pub fn kind(self) -> IrcCaseMappingKind {
        match self {
            Self::Ascii => IrcCaseMappingKind::Ascii,
            Self::Rfc1459 => IrcCaseMappingKind::Rfc1459,
        }
    }
}

/// One daemon from `OMAIRC_LIVE_DAEMONS` with published.
#[derive(Clone, Debug)]
pub struct DaemonInfo {
    pub name: String,
    pub plain_port: u16,
    pub tls_port: u16,
    pub sasl: bool,
    pub away_notify: bool,
    pub message_tags: bool,
    pub metadata: bool,
    pub wanted_caps_advertised: bool,
    pub mapping: CaseMappingExpect,
    pub nick_length: usize,
}

impl DaemonInfo {
    /// Prefers plain when present (Qt `LiveClient` does the same).
    pub fn use_tls(&self) -> bool {
        self.plain_port == 0
    }

    pub fn port(&self, tls: bool) -> u16 {
        if tls {
            self.tls_port
        } else {
            self.plain_port
        }
    }
}

/// Whether `bin/test-live` (or an equivalent) exported daemon.
pub fn live_enabled() -> bool {
    std::env::var("OMAIRC_LIVE_DAEMONS")
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

pub fn live_host() -> String {
    std::env::var("OMAIRC_LIVE_HOST").unwrap_or_else(|_| "127.0.0.1".to_owned())
}

fn env_port(key: &str) -> u16 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

fn cert_dir() -> PathBuf {
    if let Ok(from_env) = std::env::var("OMAIRC_LIVE_CERT_DIR") {
        if !from_env.is_empty() {
            return PathBuf::from(from_env);
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tests/live/certs")
}

/// Bytes of `live.pem` for the pinned TLS client config.
pub fn live_cert_pem() -> Result<Vec<u8>, String> {
    let path = cert_dir().join("live.pem");
    fs::read(&path).map_err(|error| format!("read {}: {error}", path.display()))
}

/// Daemons listed in `OMAIRC_LIVE_DAEMONS` that have at least one port.
pub fn live_daemons() -> Vec<DaemonInfo> {
    let Ok(raw) = std::env::var("OMAIRC_LIVE_DAEMONS") else {
        return Vec::new();
    };
    let mut daemons = Vec::new();
    for name in raw.split(',') {
        let name = name.trim().to_ascii_lowercase();
        if name.is_empty() {
            continue;
        }
        let info = match name.as_str() {
            "ergo" => DaemonInfo {
                name,
                plain_port: env_port("OMAIRC_LIVE_ERGO_PLAIN_PORT"),
                tls_port: env_port("OMAIRC_LIVE_ERGO_TLS_PORT"),
                sasl: true,
                away_notify: true,
                message_tags: true,
                metadata: true,
                wanted_caps_advertised: true,
                mapping: CaseMappingExpect::Ascii,
                nick_length: 32,
            },
            "solanum" => DaemonInfo {
                name,
                plain_port: env_port("OMAIRC_LIVE_SOLANUM_PLAIN_PORT"),
                tls_port: env_port("OMAIRC_LIVE_SOLANUM_TLS_PORT"),
                sasl: false,
                away_notify: true,
                message_tags: true,
                metadata: false,
                wanted_caps_advertised: true,
                mapping: CaseMappingExpect::Rfc1459,
                nick_length: 16,
            },
            "ngircd" => DaemonInfo {
                name,
                plain_port: env_port("OMAIRC_LIVE_NGIRCD_PLAIN_PORT"),
                tls_port: 0,
                sasl: false,
                away_notify: false,
                message_tags: false,
                metadata: false,
                wanted_caps_advertised: false,
                mapping: CaseMappingExpect::Ascii,
                nick_length: 9,
            },
            _ => continue,
        };
        if info.plain_port == 0 && info.tls_port == 0 {
            continue;
        }
        daemons.push(info);
    }
    daemons
}
