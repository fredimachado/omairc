//! Channel moderation and list-mode slash verbs: `/kick`, `/invite`, `/ban`,
//! `/quiet`, `/kickban`, `/bans`, `/quiets`. Mode wrappers and list inquiries
//! live here so `commanddispatcher` and `replyrouter` stay within the file-size
//! ratchet.

use crate::irc::channelmode::{parse_channel_mode_request, IrcChannelModeRequest};
use crate::irc::command::{CommandOutcome, ComposerSurface, Words};
use crate::irc::event::{ConversationKey, Event, WhoisTranscriptEvent};
use crate::irc::listmask::{expand_list_mask, kick_nick_from_mask, ListMaskKind};
use crate::irc::statusentry::ListModeLine;
use crate::session::events::SessionState;
use crate::session::session::Session;

use super::controller::Controller;
use super::replytext::refused_or_wrong_scope;

#[cfg(test)]
use super::replyrouter::ReplyWatches;

/// The outcome of one written frame: sent when it wrote, refused when it did not.
fn outcome_of(sent: bool) -> CommandOutcome {
    if sent {
        CommandOutcome::Sent
    } else {
        CommandOutcome::Refused
    }
}

/// One in-flight ban or quiet list inquiry (`ListModeWatch`). One watch per
/// network; a new `/bans` or `/quiets` replaces the previous arm.
#[derive(Clone, Debug)]
pub(super) struct ListModeWatch {
    pub(super) destination: ConversationKey,
    pub(super) kind: ListMaskKind,
    pub(super) channel: String,
}

/// Which channel-mode wrapper a `/op`, `/deop`, `/voice`, `/devoice`, `/ban`,
/// `/unban`, `/quiet` or `/unquiet` is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ChannelModeWrapper {
    pub(super) modes: &'static str,
    /// When set, the parameter is a list mask and gets the shared nick expansion.
    pub(super) list_mask: Option<ListMaskKind>,
}

#[cfg(test)]
impl ReplyWatches {
    /// How many list-mode inquiry watches are armed (a test seam).
    pub(super) fn list_mode_count(&self) -> usize {
        self.list_mode.len()
    }
}

impl Controller {
    /// Copies one whois-style body into a conversation transcript.
    pub(super) fn apply_whois_transcript(&mut self, destination: &ConversationKey, body: &str) {
        self.apply(Event::WhoisTranscript(WhoisTranscriptEvent {
            destination: destination.clone(),
            formatted_body: body.to_owned(),
        }));
    }

    /// Copies one reply body into a watch's destination, unless there is none or the
    /// body is empty.
    pub(super) fn copy_to_destination(
        &mut self,
        destination: Option<&ConversationKey>,
        text: &str,
    ) {
        if let Some(destination) = destination {
            if !text.is_empty() {
                self.apply_whois_transcript(destination, text);
            }
        }
    }

    /// `/kick [channel] <nick> [reason]` (`dispatchKick`). A first word that names a channel on the
    /// network chooses the channel; otherwise the selected channel is the target.
    pub(super) fn dispatch_kick(
        &mut self,
        active: &Session,
        argument: &str,
        surface: ComposerSurface,
    ) -> CommandOutcome {
        let words = Words::split(argument);
        if words.first.is_empty() {
            return CommandOutcome::Refused;
        }
        let network = self.surface_network(surface);
        let features = self.reducer.server_features(&network).clone();
        if features.is_channel(&words.first) {
            let after = Words::split(&words.rest);
            if after.first.is_empty() {
                return CommandOutcome::Refused;
            }
            return outcome_of(active.kick(&words.first, &after.first, &after.rest));
        }
        if self.selected.is_none() || !self.is_channel() {
            return CommandOutcome::WrongScope;
        }
        let selected_network = self.selected.as_ref().map(|key| key.network_id.clone());
        if selected_network.as_deref() != Some(network.as_str()) {
            return CommandOutcome::Refused;
        }
        let channel = self.selected_target().to_owned();
        outcome_of(!channel.is_empty() && active.kick(&channel, &words.first, &words.rest))
    }

    /// `/invite <nick> [channel]` (`dispatchInvite`). Without a channel the selected one is used
    /// and Status has none to name, so an invite there needs one.
    pub(super) fn dispatch_invite(
        &mut self,
        active: &Session,
        argument: &str,
        surface: ComposerSurface,
    ) -> CommandOutcome {
        let words = Words::split(argument);
        if words.first.is_empty() {
            return CommandOutcome::WrongScope;
        }
        let network = self.surface_network(surface);
        let features = self.reducer.server_features(&network).clone();
        if features.is_channel(&words.first) {
            return CommandOutcome::Refused;
        }
        let nick = words.first.clone();
        let channel = if words.rest.is_empty() {
            if surface == ComposerSurface::Status {
                return CommandOutcome::WrongScope;
            }
            if self.selected.is_none() || !self.is_channel() {
                return CommandOutcome::WrongScope;
            }
            let selected_network = self.selected.as_ref().map(|key| key.network_id.clone());
            if selected_network.as_deref() != Some(network.as_str()) {
                return CommandOutcome::Refused;
            }
            self.selected_target().to_owned()
        } else {
            let named = Words::split(&words.rest);
            if !features.is_channel(&named.first) || !named.rest.is_empty() {
                return CommandOutcome::Refused;
            }
            named.first
        };
        outcome_of(!channel.is_empty() && active.invite(&nick, &channel))
    }

    /// `/kickban [channel] <nick> [reason]` (`/kb`). Scope matches `/kick`; the ban
    /// mask expands like `/ban`, and a token with `!` bans that mask while kicking
    /// the nick before `!`.
    pub(super) fn dispatch_kickban(
        &mut self,
        active: &Session,
        argument: &str,
        surface: ComposerSurface,
    ) -> CommandOutcome {
        let words = Words::split(argument);
        if words.first.is_empty() {
            return CommandOutcome::Refused;
        }
        let network = self.surface_network(surface);
        let features = self.reducer.server_features(&network).clone();
        let (channel, nick_token, reason) = if features.is_channel(&words.first) {
            let after = Words::split(&words.rest);
            if after.first.is_empty() {
                return CommandOutcome::Refused;
            }
            (words.first.clone(), after.first.clone(), after.rest.clone())
        } else {
            if self.selected.is_none() || !self.is_channel() {
                return CommandOutcome::WrongScope;
            }
            let selected_network = self.selected.as_ref().map(|key| key.network_id.clone());
            if selected_network.as_deref() != Some(network.as_str()) {
                return CommandOutcome::Refused;
            }
            let channel = self.selected_target().to_owned();
            if channel.is_empty() {
                return CommandOutcome::Refused;
            }
            (channel, words.first.clone(), words.rest.clone())
        };
        let nick = kick_nick_from_mask(&nick_token);
        if nick.is_empty() {
            return CommandOutcome::Refused;
        }
        let mask = expand_list_mask(&nick_token);
        outcome_of(active.kickban(&channel, &mask, nick, &reason))
    }

    /// `/mode <target>...` (`dispatchMode`). The argument is read against the network's channel
    /// modes, so a query and a change both map to the frame the server expects.
    pub(super) fn dispatch_mode(&mut self, argument: &str, surface: ComposerSurface) -> CommandOutcome {
        if argument.is_empty() {
            return CommandOutcome::Refused;
        }
        let network = self.surface_network(surface);
        if network.is_empty() {
            return refused_or_wrong_scope(surface);
        }
        let features = self.reducer.server_features(&network).clone();
        let Some(request) = parse_channel_mode_request(argument, &features) else {
            return CommandOutcome::Refused;
        };
        let Some(active) = self.registered_session(&network) else {
            return CommandOutcome::NotConnected;
        };
        outcome_of(active.send_channel_mode(&request))
    }

    /// `/op`, `/deop`, `/voice`, `/devoice`, `/ban` and `/unban`
    /// (`dispatchChannelModeWrapper`). They turn one nick or mask into a MODE
    /// change on the selected channel.
    pub(super) fn dispatch_channel_mode_wrapper(
        &mut self,
        wrapper: ChannelModeWrapper,
        words: &Words,
        surface: ComposerSurface,
    ) -> CommandOutcome {
        if self.selected.is_none() || !self.is_channel() {
            return CommandOutcome::WrongScope;
        }
        let selected_network = self.selected.as_ref().map(|key| key.network_id.clone());
        if selected_network.as_deref() != Some(self.surface_network(surface).as_str()) {
            return CommandOutcome::Refused;
        }
        if words.first.is_empty() || !words.rest.is_empty() {
            return CommandOutcome::Refused;
        }
        if wrapper.list_mask == Some(ListMaskKind::Quiet) {
            let network = self.surface_network(surface);
            let features = self.reducer.server_features(&network);
            if !features.chan_modes_a().contains('q') {
                return CommandOutcome::Refused;
            }
        }
        let parameter = if wrapper.list_mask.is_some() {
            expand_list_mask(&words.first)
        } else {
            words.first.clone()
        };
        let argument = format!("{} {} {parameter}", self.selected_target(), wrapper.modes);
        self.dispatch_mode(&argument, surface)
    }

    /// Routes a ban or quiet list line to the armed inquiry destination
    /// (`routeListModeLine`). A new inquiry replaces the previous watch, so at
    /// most one list inquiry is armed per network.
    pub(super) fn route_list_mode_line(&mut self, network_id: &str, line: &ListModeLine) {
        let Some(watch) = self.reply_watches.list_mode.get(network_id).cloned() else {
            return;
        };
        if watch.kind != line.kind {
            return;
        }
        let features = self.reducer.server_features(network_id);
        if !features
            .case_mapping()
            .equals(&watch.channel, &line.channel)
        {
            return;
        }
        self.apply_whois_transcript(&watch.destination, &line.text);
        if line.is_terminal() {
            self.reply_watches.list_mode.remove(network_id);
        }
    }

    /// `/bans` or `/quiets`: arms a list-mode watch and writes `MODE #chan b|q`.
    /// Conversation-only; a second inquiry on the same network replaces the first.
    pub(super) fn dispatch_list_mode_inquiry(
        &mut self,
        kind: ListMaskKind,
        argument: &str,
        surface: ComposerSurface,
    ) -> CommandOutcome {
        if !argument.is_empty() {
            return CommandOutcome::Refused;
        }
        if surface != ComposerSurface::Conversation
            || self.selected.is_none()
            || !self.is_channel()
        {
            return CommandOutcome::WrongScope;
        }
        let network = self.surface_network(surface);
        if network.is_empty() {
            return CommandOutcome::WrongScope;
        }
        let selected_network = self.selected.as_ref().map(|key| key.network_id.clone());
        if selected_network.as_deref() != Some(network.as_str()) {
            return CommandOutcome::Refused;
        }
        let features = self.reducer.server_features(&network).clone();
        if kind == ListMaskKind::Quiet && !features.chan_modes_a().contains('q') {
            return CommandOutcome::Refused;
        }
        let channel = self.selected_target().to_owned();
        if channel.is_empty() || !features.is_channel(&channel) {
            return CommandOutcome::Refused;
        }
        let Some(session) = self
            .session_for(surface)
            .filter(|session| session.state() == SessionState::Registered)
        else {
            return CommandOutcome::NotConnected;
        };
        let destination = self.selected.clone().expect("selected channel checked above");
        let modes = kind.mode_letter().to_string();
        let request = IrcChannelModeRequest {
            channel: channel.clone(),
            modes,
            parameters: Vec::new(),
            query: false,
        };
        // Replace any prior list inquiry on this network.
        self.reply_watches.list_mode.insert(
            network.clone(),
            ListModeWatch {
                destination,
                kind,
                channel,
            },
        );
        if !session.send_channel_mode(&request) {
            self.reply_watches.list_mode.remove(&network);
            return CommandOutcome::Refused;
        }
        CommandOutcome::Sent
    }
}
