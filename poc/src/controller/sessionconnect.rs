//! Explicit session connect for the live harness (and similar non-profile starts).

use crate::session::config::SessionConfig;
use crate::storage::conversationlog::FileConversationLog;

use super::controller::Controller;

impl Controller {
    /// Points transcript persistence at `root` (Qt `setTranscriptRoot`). Call before
    /// [`Self::connect_session`] when a live case restores DMs across reloads.
    pub fn set_transcript_root(&mut self, root: impl Into<std::path::PathBuf>) {
        self.reducer.set_conversation_log(Some(Box::new(
            FileConversationLog::new(root.into()),
        )));
    }

    /// Registers and starts a session with an explicit config. The live harness uses this
    /// instead of saving a Connect profile, so reconnect, autojoin and timers match the
    /// Qt `LiveClient` knobs.
    pub fn connect_session(&mut self, config: SessionConfig) -> Result<(), String> {
        let network_id = config.network_id.clone();
        if network_id.is_empty() {
            return Err("network id is required".to_owned());
        }
        if self.manager.find(&network_id).is_some() {
            self.manager.discard(&network_id);
            self.applied_sessions.remove(&network_id);
            self.applied_profile_avatars.remove(&network_id);
            self.autoaway_forget_network(&network_id);
            self.monitor_forget_presence(&network_id);
        }
        let transport = self.transports.open(&network_id)?;
        self.manager
            .create(config, transport, self.clock.clone())
            .map_err(|error| error.to_string())?;
        self.attach_store_hooks(&network_id);
        if !self.manager.activate(&network_id) {
            return Err("session did not become live".to_owned());
        }
        self.pump();
        Ok(())
    }
}
