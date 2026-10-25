//! Controller + NetHub client that dials a live ircd (Qt `LiveClient`).

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use crate::controller::controller::Controller;
use crate::irc::message::IrcMessage;
use crate::irc::reducer::MemberView;
use crate::irc::serverfeatures::IrcServerFeatures;
use crate::irc::statusentry::{LogSource, StatusEntry};
use crate::irc::world::Message;
use crate::net::{client_config_trusting_pem, NetHub, NetOptions, NetTransports};
use crate::session::clock::{Clock, RealClock};
use crate::session::config::SessionConfig;
use crate::session::events::SessionState;
use crate::session::session::Session;
use crate::storage::Backing;
use crate::util::Secret;

use super::env::{live_cert_pem, live_host, DaemonInfo};
use super::wait::{message_has_command, wait_until, DEFAULT_TIMEOUT};

static NICK_SEQ: AtomicU32 = AtomicU32::new(0);
static CHANNEL_SEQ: AtomicU32 = AtomicU32::new(0);

/// Optional knobs for [`LiveClient::connect`].
#[derive(Clone, Default)]
pub struct LiveClientOptions {
    pub password: String,
    pub network_id: String,
    pub transcript_root: Option<PathBuf>,
    /// When set, preferences and open directs persist on this INI path.
    pub settings_file: Option<PathBuf>,
    pub reconnect_enabled: bool,
    pub autojoin_channels: Vec<String>,
}

/// A unique nick within the daemon's NICKLEN budget.
pub fn unique_nick(max_length: usize) -> String {
    let serial = NICK_SEQ.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id() % 4096;
    let mut nick = format!("o{:04x}", (serial.wrapping_add(pid as u32)) & 0xffff);
    if max_length > 0 && nick.len() > max_length {
        nick.truncate(max_length);
    }
    nick
}

/// A unique channel name.
pub fn unique_channel() -> String {
    let serial = CHANNEL_SEQ.fetch_add(1, Ordering::Relaxed);
    format!("#c{serial:06x}")
}

/// One live connection: hub, controller, and the session's network id.
pub struct LiveClient {
    pub hub: Rc<NetHub>,
    pub controller: Controller,
    pub network_id: String,
    _clock: Rc<dyn Clock>,
}

impl LiveClient {
    /// Dials `daemon` as `nick` (TLS when `tls` or when the daemon has no plain port).
    pub fn connect(daemon: &DaemonInfo, nick: &str, tls: bool, options: LiveClientOptions) -> Self {
        let pem = live_cert_pem().expect("live.pem is readable");
        let tls_config = client_config_trusting_pem(&pem).expect("live.pem pins");
        let hub = Rc::new(NetHub::with_options(NetOptions {
            tls: tls_config,
            ..NetOptions::default()
        }));
        let clock: Rc<dyn Clock> = Rc::new(RealClock::new());
        let transports = Box::new(NetTransports::new(hub.clone()));
        let backing = match options.settings_file {
            Some(path) => Backing::Disk(path),
            None => Backing::Ephemeral,
        };
        let mut controller = Controller::with_backing(clock.clone(), transports, backing);
        if let Some(root) = options.transcript_root {
            controller.set_transcript_root(root);
        }

        let network_id = if options.network_id.is_empty() {
            format!("live-{}", unique_id())
        } else {
            options.network_id
        };
        let use_tls = tls || daemon.use_tls();
        let port = daemon.port(use_tls);
        assert!(port != 0, "{} has no port for tls={use_tls}", daemon.name);

        let mut config = SessionConfig::with_defaults(
            &network_id,
            &daemon.name,
            &live_host(),
            nick,
        );
        config.port = port;
        config.tls_enabled = use_tls;
        config.username = "omairc".to_owned();
        config.realname = "Omairc live".to_owned();
        if !options.password.is_empty() {
            config.password = Secret::from(options.password);
        }
        config.reconnect_enabled = options.reconnect_enabled;
        config.autojoin_channels = options.autojoin_channels;
        config.capability_timeout_ms = 8000;

        controller
            .connect_session(config)
            .unwrap_or_else(|error| panic!("connect_session: {error}"));
        if let Some(session) = controller.session(&network_id) {
            session.enable_received_log();
        }

        Self {
            hub,
            controller,
            network_id,
            _clock: clock,
        }
    }

    /// Drains net deliveries and runs controller timers.
    pub fn pump(&mut self) {
        self.hub.drain_ready();
        self.controller.tick();
    }

    fn session(&self) -> Session {
        self.controller
            .session(&self.network_id)
            .expect("live session is registered")
    }

    pub fn incoming(&self) -> Vec<IrcMessage> {
        self.session().received_messages()
    }

    pub fn nick(&self) -> String {
        self.session().nick()
    }

    pub fn state(&self) -> SessionState {
        self.session().state()
    }

    pub fn features(&self) -> &IrcServerFeatures {
        self.controller.features(&self.network_id)
    }

    pub fn status_lines(&self) -> Vec<&StatusEntry> {
        self.controller.status_lines(&self.network_id)
    }

    pub fn has_server_label(&self, label: &str) -> bool {
        self.status_lines()
            .iter()
            .any(|entry| entry.label == label && entry.source == LogSource::Server)
    }

    pub fn wait_registered(&mut self) -> bool {
        let network_id = self.network_id.clone();
        wait_until(
            || {
                self.pump();
                self.controller.session(&network_id).is_some_and(|session| {
                    session.state() == SessionState::Registered
                })
            },
            DEFAULT_TIMEOUT,
        )
    }

    pub fn wait_failed(&mut self) -> bool {
        let network_id = self.network_id.clone();
        wait_until(
            || {
                self.pump();
                self.controller
                    .session(&network_id)
                    .is_some_and(|session| session.state() == SessionState::Failed)
            },
            DEFAULT_TIMEOUT,
        )
    }

    pub fn wait_reconnecting(&mut self) -> bool {
        let network_id = self.network_id.clone();
        wait_until(
            || {
                self.pump();
                self.controller.session(&network_id).is_some_and(|session| {
                    session.state() == SessionState::Reconnecting
                })
            },
            DEFAULT_TIMEOUT,
        )
    }

    pub fn wait_registered_again(&mut self) -> bool {
        wait_until(
            || {
                self.pump();
                self.state() == SessionState::Registered
            },
            Duration::from_secs(40),
        )
    }

    pub fn wait_motd(&mut self) -> bool {
        wait_until(
            || {
                self.pump();
                self.has_server_label("376") || self.has_server_label("422")
            },
            DEFAULT_TIMEOUT,
        )
    }

    pub fn wait_server_label(&mut self, label: &str) -> bool {
        wait_until(
            || {
                self.pump();
                self.has_server_label(label)
            },
            DEFAULT_TIMEOUT,
        )
    }

    pub fn wait_command(&mut self, command: &str) -> bool {
        wait_until(
            || {
                self.pump();
                message_has_command(&self.incoming(), command)
            },
            DEFAULT_TIMEOUT,
        )
    }

    pub fn join_channel(&mut self, channel: &str) -> bool {
        let ok = self.controller.join_channel(&self.network_id, channel);
        self.pump();
        ok
    }

    pub fn select_channel(&mut self, channel: &str) {
        self.controller
            .select_conversation(&self.network_id, channel);
        self.pump();
    }

    pub fn member(&self, nick: &str) -> Option<MemberView> {
        self.controller
            .members()
            .into_iter()
            .find(|member| member.nick.eq_ignore_ascii_case(nick))
    }

    pub fn messages(&self) -> &[Message] {
        self.controller.messages()
    }

    pub fn force_disconnect(&mut self) -> bool {
        if self.state() != SessionState::Registered {
            return false;
        }
        self.session().force_transport_shutdown();
        self.pump();
        true
    }

    pub fn last_error(&self) -> Option<&str> {
        self.controller.last_error_for(&self.network_id)
    }

    pub fn wait_until(&mut self, mut predicate: impl FnMut(&mut Self) -> bool) -> bool {
        wait_until(
            || {
                self.pump();
                predicate(self)
            },
            DEFAULT_TIMEOUT,
        )
    }
}

impl Drop for LiveClient {
    fn drop(&mut self) {
        let _ = self.controller.disconnect(&self.network_id);
        self.pump();
    }
}

fn unique_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos:x}")
}
