//! Bouncer playback and chathistory catch-up (Gap 5). The coordinator owns per-connection
//! books; this slice only reserves the field on `Controller`.

use std::cell::RefCell;
use std::rc::Rc;

use crate::storage::playbacktime::PlaybackTimeStore;

/// Per-connection playback state (`IrcPlaybackCoordinator`). Gap 5 fills the books.
pub struct PlaybackCoordinator;

impl PlaybackCoordinator {
    /// An empty coordinator tied to the persisted playback-time store.
    pub fn new(_times: Rc<RefCell<PlaybackTimeStore>>) -> Self {
        Self
    }
}
