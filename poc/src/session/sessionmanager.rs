//! Port of `tui/internal/session/sessionmanager.go` (Go) and
//! `src/irc/ircsessionmanager.{h,cpp}` (C++).
//!
//! The manager owns the live sessions, one per network id. The Qt manager keeps
//! its sessions as QObject children and tears them down with `deleteLater`; Rust
//! has no parent, so the manager holds the only reference and `discard` stops
//! the session and drops it.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt;
use std::rc::Rc;

use super::clock::Clock;
use super::config::SessionConfig;
use super::events::SessionState;
use super::session::Session;
use super::sts::StsStore;
use super::transport::Transport;

/// Why `SessionManager::create` refused a session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionManagerError {
    /// The config carried no network id.
    NetworkIdRequired,
    /// A session for this network id already exists.
    DuplicateNetworkId(String),
}

impl fmt::Display for SessionManagerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NetworkIdRequired => formatter.write_str("session: network id is required"),
            Self::DuplicateNetworkId(id) => {
                write!(formatter, "session: network id {id} is already registered")
            }
        }
    }
}

impl std::error::Error for SessionManagerError {}

/// The registry of sessions, keyed by network id. It holds the one STS policy cache its
/// sessions share, so a policy learned on one network is kept for the next start.
pub struct SessionManager {
    sessions: RefCell<BTreeMap<String, Session>>,
    sts: Rc<dyn StsStore>,
}

impl SessionManager {
    /// An empty manager whose sessions share `sts`.
    pub fn new(sts: Rc<dyn StsStore>) -> Self {
        Self {
            sessions: RefCell::new(BTreeMap::new()),
            sts,
        }
    }

    /// Builds a session and registers it. Rejects an empty network id and a
    /// duplicate one (`IrcSessionManager::createSession`). Creation is add-only:
    /// nothing here replaces a session already registered.
    pub fn create(
        &self,
        config: SessionConfig,
        transport: Rc<dyn Transport>,
        clock: Rc<dyn Clock>,
    ) -> Result<Session, SessionManagerError> {
        if config.network_id.is_empty() {
            return Err(SessionManagerError::NetworkIdRequired);
        }
        let mut sessions = self.sessions.borrow_mut();
        if sessions.contains_key(&config.network_id) {
            return Err(SessionManagerError::DuplicateNetworkId(config.network_id));
        }
        let network_id = config.network_id.clone();
        let session = Session::new(config, transport, clock, self.sts.clone());
        sessions.insert(network_id, session.clone());
        Ok(session)
    }

    /// The session for `network_id`, if registered.
    pub fn find(&self, network_id: &str) -> Option<Session> {
        self.sessions.borrow().get(network_id).cloned()
    }

    /// Every registered network id, sorted.
    pub fn network_ids(&self) -> Vec<String> {
        self.sessions.borrow().keys().cloned().collect()
    }

    /// Starts the session for `network_id`, and reports whether it is live
    /// afterwards (`IrcSessionManager::activateSession`).
    pub fn activate(&self, network_id: &str) -> bool {
        let Some(session) = self.find(network_id) else {
            return false;
        };
        session.start();
        is_live(&session)
    }

    /// Stops the session for `network_id`. A missing network is a no-op.
    // Phase 11: the socket transport starts and stops its sessions
    #[allow(dead_code)]
    pub fn stop(&self, network_id: &str) {
        if let Some(session) = self.find(network_id) {
            session.stop();
        }
    }

    /// Unregisters the session and stops it. The removal comes first, so a
    /// `find` after `discard` returns nothing (`IrcSessionManager::discardSession`).
    pub fn discard(&self, network_id: &str) {
        let removed = self.sessions.borrow_mut().remove(network_id);
        if let Some(session) = removed {
            session.stop();
        }
    }

    /// Whether `network_id` is registered and its session is live.
    pub fn is_live(&self, network_id: &str) -> bool {
        self.find(network_id)
            .is_some_and(|session| is_live(&session))
    }
}

/// Live means neither Idle nor Failed (`IrcSessionManager::isLive`).
fn is_live(session: &Session) -> bool {
    !matches!(session.state(), SessionState::Idle | SessionState::Failed)
}
