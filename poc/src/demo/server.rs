//! Port of `tui/internal/demo/demo.go` (Go) and `IrcDemoServer` in
//! `src/irc/ircdemoserver.{h,cpp}` (C++): the demo peer for the two seeded
//! networks, behind `--demo-server`.
//!
//! The demo is only the peer at the far end of a `LoopbackTransport`. `attach`
//! starts a session and completes its connect, as `IrcDemoServer::startNetwork`
//! does. `seed` then plays Qt's seeding order: the network order, the
//! registration burst, the world, the transcript, the seeded reads, the typing
//! hint, the live account change, and the OFTC INVITE. Once the seed is in,
//! `seed` can install the auto-answer hook, so the client's own frames get
//! server replies.
//!
//! The demo never touches `World` or the controller. Two of the seed's steps are
//! controller intents, the network order and conversation selection. They come
//! back to the caller as `SeedStep`s, and `seed` runs them through `SeedControls`.
//! The steps are pure data (`seed_steps`), so the order is testable on its own.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use chrono::{DateTime, Utc};

use crate::session::clock::Clock;
use crate::session::config::SessionConfig;
use crate::session::loopback::{FrameWrittenHook, LoopbackTransport};
use crate::session::session::Session;
use crate::session::transport::{ConnectionState, Transport};

use super::answers::{
    answer_away, answer_ctcp, answer_list, answer_metadata, answer_monitor, answer_ping,
    client_echo, demo_online_nicks, self_join,
};
use super::seed::{
    world_for, SeedNetwork, OFTC_NETWORK_ID, OFTC_WORLD, OMARCHY_NETWORK_ID, OMARCHY_WORLD,
};
use super::wire::{
    autojoin_names, bytes, live_account_lines, oftc_invite_line, registration_lines,
    transcript_lines, typing_lines, world_lines,
};

/// One step of Qt's seeding order (`IrcDemoServer::attach`). `Inject` bytes go to
/// one network's peer. The other two are controller intents that the caller runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SeedStep {
    /// The sidebar's network order (`setNetworkOrder`).
    NetworkOrder(Vec<String>),
    /// Bytes for one network's peer (`injectBytes`).
    Inject { network_id: String, bytes: Vec<u8> },
    /// A conversation selection (`selectConversation`). A seeded read runs through
    /// the same step.
    Select { network_id: String, target: String },
}

/// The controller intents the seed interleaves with its bytes. The controller
/// implements this. The demo calls it and never reads or writes controller state.
pub trait SeedControls {
    /// Sets the sidebar's network order.
    fn set_network_order(&mut self, network_ids: &[String]);
    /// Selects a conversation. The seed uses it for the transcript and the reads.
    fn select_conversation(&mut self, network_id: &str, target: &str);
}

/// Why attach or seed refused to run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DemoError {
    /// The session's network is not one of the two seeded networks.
    UnknownNetwork(String),
    /// The transport did not reach an open state after the connect.
    NotConnected(String),
    /// `seed` ran before both seeded networks were attached.
    NotAttached(String),
}

impl fmt::Display for DemoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownNetwork(id) => write!(formatter, "demo: no seeded network {id}"),
            Self::NotConnected(id) => write!(formatter, "demo: {id} did not connect"),
            Self::NotAttached(id) => write!(formatter, "demo: {id} is not attached"),
        }
    }
}

impl std::error::Error for DemoError {}

/// The session config a demo network starts with (`sessionConfig` in C++): the
/// seeded nick, `irc.example` on TLS, the seeded channels for autojoin, and no
/// reconnect. The profile name stays empty, so the display name falls back to
/// the host and both networks read `irc.example` apart by nick.
pub fn session_config(network: &SeedNetwork) -> SessionConfig {
    let mut config =
        SessionConfig::with_defaults(network.network_id, "", "irc.example", network.nick);
    config.tls_enabled = true;
    config.autojoin_channels = autojoin_names(network);
    config.reconnect_enabled = false;
    config
}

/// One attached network: its seed, its loopback, and its MONITOR list.
#[derive(Clone)]
struct Peer {
    network: &'static SeedNetwork,
    transport: Rc<LoopbackTransport>,
    monitor: Rc<RefCell<Vec<String>>>,
}

/// The demo peer for the two seeded networks.
pub struct DemoServer {
    clock: Rc<dyn Clock>,
    peers: RefCell<Vec<Peer>>,
}

impl DemoServer {
    /// A demo with no network attached yet. `clock` supplies the TIME reply.
    pub fn new(clock: Rc<dyn Clock>) -> Self {
        Self {
            clock,
            peers: RefCell::new(Vec::new()),
        }
    }

    /// Starts `session` and completes its loopback connect, as
    /// `IrcDemoServer::startNetwork` does. The session's network must be one of the
    /// seeded networks. The handshake's own writes (CAP LS and the rest) are
    /// recorded but not answered: the auto-answer hook goes in only at the end of
    /// `seed`, so the registration burst is what the session sees first.
    pub fn attach(
        &self,
        session: &Session,
        transport: &Rc<LoopbackTransport>,
    ) -> Result<(), DemoError> {
        let network_id = session.network_id();
        let network =
            world_for(&network_id).ok_or_else(|| DemoError::UnknownNetwork(network_id.clone()))?;
        session.start();
        transport.complete_connect();
        if !matches!(
            transport.state(),
            ConnectionState::Connected | ConnectionState::Encrypted
        ) {
            return Err(DemoError::NotConnected(network_id));
        }

        let peer = Peer {
            network,
            transport: Rc::clone(transport),
            monitor: Rc::new(RefCell::new(Vec::new())),
        };
        let mut peers = self.peers.borrow_mut();
        peers.retain(|existing| existing.network.network_id != network.network_id);
        peers.push(peer);
        Ok(())
    }

    /// Starts a seeded network again on a fresh loopback. It attaches the session,
    /// plays the network's registration burst, and installs the auto-answer hook,
    /// so the restarted session registers and answers like the first one did. The
    /// controller calls it after a disconnect (`Start`).
    pub fn restart(
        &self,
        session: &Session,
        transport: &Rc<LoopbackTransport>,
    ) -> Result<(), DemoError> {
        self.attach(session, transport)?;
        let network_id = session.network_id();
        let network =
            world_for(&network_id).ok_or_else(|| DemoError::UnknownNetwork(network_id.clone()))?;
        self.inject(
            network.network_id,
            &bytes(&registration_lines(
                network.nick,
                network.welcome,
                network.icon_url,
            )),
        );
        let peer = self
            .peers
            .borrow()
            .iter()
            .find(|peer| peer.network.network_id == network.network_id)
            .cloned()
            .ok_or(DemoError::NotAttached(network_id))?;
        self.install_auto_echo(&peer);
        Ok(())
    }

    /// The loopback attached for `network_id`, or `None`.
    pub fn transport(&self, network_id: &str) -> Option<Rc<LoopbackTransport>> {
        self.peers
            .borrow()
            .iter()
            .find(|peer| peer.network.network_id == network_id)
            .map(|peer| Rc::clone(&peer.transport))
    }

    /// Delivers inbound bytes to `network_id`'s peer. False when that network is
    /// not attached.
    pub fn inject(&self, network_id: &str, payload: &[u8]) -> bool {
        let Some(transport) = self.transport(network_id) else {
            return false;
        };
        transport.inject_bytes(payload);
        true
    }

    /// Replays the last client PRIVMSG on `network_id` as a self echo. False when
    /// there is no such frame (`echoLastPrivmsg`).
    pub fn echo_last_privmsg(&self, network_id: &str, nick: &str) -> bool {
        if nick.is_empty() {
            return false;
        }
        let Some(transport) = self.transport(network_id) else {
            return false;
        };
        let frames = transport.written_frames();
        let Some(frame) = frames
            .iter()
            .rev()
            .find(|frame| frame.starts_with(b"PRIVMSG "))
        else {
            return false;
        };
        transport.inject_bytes(&client_echo(nick, frame));
        true
    }

    /// Qt's seeding order, as data. Each `Inject` carries the bytes its builder
    /// produces. Each `Select` is one `selectConversation`, and a seeded read is a
    /// `Select` too, in the Go `markRead` order.
    pub fn seed_steps() -> Vec<SeedStep> {
        let omarchy = &OMARCHY_WORLD;
        let oftc = &OFTC_WORLD;
        let inject = |network: &SeedNetwork, lines: Vec<String>| SeedStep::Inject {
            network_id: network.network_id.to_owned(),
            bytes: bytes(&lines),
        };
        let select = |network: &SeedNetwork, target: &str| SeedStep::Select {
            network_id: network.network_id.to_owned(),
            target: target.to_owned(),
        };

        let mut steps = vec![SeedStep::NetworkOrder(vec![
            OMARCHY_NETWORK_ID.to_owned(),
            OFTC_NETWORK_ID.to_owned(),
        ])];
        steps.push(inject(
            omarchy,
            registration_lines(omarchy.nick, omarchy.welcome, omarchy.icon_url),
        ));
        steps.push(inject(
            oftc,
            registration_lines(oftc.nick, oftc.welcome, oftc.icon_url),
        ));
        steps.push(inject(omarchy, world_lines(omarchy)));
        steps.push(inject(oftc, world_lines(oftc)));
        steps.push(select(omarchy, "#omarchy"));
        steps.push(inject(omarchy, transcript_lines(omarchy)));
        steps.push(inject(oftc, transcript_lines(oftc)));
        for network in [omarchy, oftc] {
            for channel in network.channels {
                if channel.mark_read {
                    steps.push(select(network, channel.name));
                }
            }
            for direct in network.directs {
                if direct.mark_read {
                    steps.push(select(network, direct.nick));
                }
            }
        }
        steps.push(select(omarchy, "#omarchy"));
        steps.push(inject(omarchy, typing_lines(omarchy)));
        steps.push(inject(omarchy, live_account_lines(omarchy)));
        // A live background INVITE, not replayed history. It leaves one inbox row
        // at launch, and it lands on OFTC so the omarchy Status link sheet keeps
        // only what a test injects.
        steps.push(SeedStep::Inject {
            network_id: oftc.network_id.to_owned(),
            bytes: bytes(&[oftc_invite_line()]),
        });
        steps
    }

    /// Plays the seed in Qt's order: the controller intents go to `controls`, and
    /// the bytes go to the attached peers. When `auto_echo` is set, the auto-answer
    /// hook is installed last, so the seed's own traffic is not answered
    /// (`IrcDemoServer::attach`). Both seeded networks must be attached.
    pub fn seed(&self, controls: &mut dyn SeedControls, auto_echo: bool) -> Result<(), DemoError> {
        for network_id in [OMARCHY_NETWORK_ID, OFTC_NETWORK_ID] {
            if self.transport(network_id).is_none() {
                return Err(DemoError::NotAttached(network_id.to_owned()));
            }
        }
        for step in Self::seed_steps() {
            match step {
                SeedStep::NetworkOrder(network_ids) => controls.set_network_order(&network_ids),
                SeedStep::Inject {
                    network_id,
                    bytes: payload,
                } => {
                    self.inject(&network_id, &payload);
                }
                SeedStep::Select { network_id, target } => {
                    controls.select_conversation(&network_id, &target);
                }
            }
        }
        if auto_echo {
            let peers = self.peers.borrow().clone();
            for peer in &peers {
                self.install_auto_echo(peer);
            }
        }
        Ok(())
    }

    /// Installs the frame-written hook that answers the client's frames. The hook
    /// holds the transport weakly, so no reference cycle forms.
    fn install_auto_echo(&self, peer: &Peer) {
        let weak = Rc::downgrade(&peer.transport);
        let network = peer.network;
        let monitor = Rc::clone(&peer.monitor);
        let clock = Rc::clone(&self.clock);
        let online = demo_online_nicks(network);
        let hook: FrameWrittenHook = Rc::new(move |frame: &[u8]| {
            let Some(transport) = weak.upgrade() else {
                return;
            };
            let reply = auto_echo_reply(network, &monitor, &online, clock.now(), frame);
            if let Some(reply) = reply {
                if !reply.is_empty() {
                    transport.inject_bytes(&reply);
                }
            }
        });
        peer.transport.set_frame_written_hook(hook);
    }
}

/// The answer ladder, in the Go order: PING, LIST, CTCP, AWAY, METADATA, MONITOR,
/// then the self JOIN and the PRIVMSG or NOTICE echo (`hookAutoEcho`). The first
/// answer that claims the frame wins.
fn auto_echo_reply(
    network: &SeedNetwork,
    monitor: &RefCell<Vec<String>>,
    online: &[String],
    now: DateTime<Utc>,
    frame: &[u8],
) -> Option<Vec<u8>> {
    let me = network.nick;
    if let Some(reply) = answer_ping(frame) {
        return Some(reply);
    }
    if let Some(reply) = answer_list(network, frame) {
        return Some(reply);
    }
    if let Some(reply) = answer_ctcp(me, frame, now) {
        return Some(reply);
    }
    if let Some(reply) = answer_away(me, frame) {
        return Some(reply);
    }
    if let Some(reply) = answer_metadata(me, frame) {
        return Some(reply);
    }
    let monitored = {
        let mut watched = monitor.borrow_mut();
        answer_monitor(me, frame, online, &mut watched)
    };
    if let Some(reply) = monitored {
        return Some(reply);
    }
    if frame.starts_with(b"JOIN ") {
        return Some(self_join(network, me, frame));
    }
    if frame.starts_with(b"PRIVMSG ") || frame.starts_with(b"NOTICE ") {
        return Some(client_echo(me, frame));
    }
    None
}
