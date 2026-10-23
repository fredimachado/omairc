//! Mention and monitor arrivals queued for the shell. Gap 1 turns these into desktop
//! notifications and title marks; the shell drains them from `take_arrivals`.

use crate::irc::event::MentionArrival;

use super::controller::Controller;

/// One arrival the shell may surface as a notification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Arrival {
    Mention(MentionArrival),
    Monitor {
        display: String,
        body: String,
        network_id: String,
    },
}

impl Controller {
    /// Drains mention and monitor arrivals raised since the last call.
    pub fn take_arrivals(&mut self) -> Vec<Arrival> {
        let _ = (
            &self.notifier,
            &self.closed,
            &self.playback_times,
            &self.playback,
            self.transcript_follow_end,
        );
        std::mem::take(&mut self.arrivals)
    }
}
