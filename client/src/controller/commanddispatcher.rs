//! Every slash verb's decision. The UI never decides a verb: it parses the composer line
//! calls `Controller::submit` (or `dispatch`), and reads back the outcome and the surfaces the
//! intent raised. The side effects a verb needs (the wire, the reducer, the stores, the Status
//! log, the transcript) are the controller's own, so the same path serves the composer, the
//! Status console and the product CLI.
//! The dispatcher owns the decisions: which surface a verb is valid on, which network and
//! channel it names, what a refusal means, and the Status text of each outcome. Frames are
//! written through `Session` (`irc::commandbuilder` rules), so a verb's frame is the one the
//! session's tests pin.
//! Two deliberate differences from the Go host seam: the selection is read from the controller
//! (`dispatch` takes no selection argument), and `/pref` feedback on a conversation is one
//! transcript row per line, since the transcript rows are single lines.



use crate::irc::autoaway::parse_autoaway_argument;
use crate::irc::channelmode::parse_channel_mode_request;
use crate::irc::listmask::{expand_list_mask, kick_nick_from_mask, ListMaskKind};
use crate::irc::pref::{PrefName, PrefRequest, PrefValue, Preferences};
use crate::irc::command::{
    command_outcome_text, help_text, parse, pref_reply, Command, CommandOutcome, ComposerSurface,
    Words,
};
use crate::irc::event::{ConversationKey, IrcConversationCause};
use crate::irc::jointarget::{parse_join_targets, IrcJoinTarget};

use crate::irc::viewnotify::{self, ViewNotify};
use crate::session::events::SessionState;
use crate::session::session::Session;

use super::controller::Controller;
use super::monitorcoordinator::MonitorOutcome;
use super::send::Outgoing;
use super::stores::{ignore_nick_is_usable, mute_target_is_usable};

/// Which list a `/ignore`, `/mute`, `/highlight` or `/monitor` family verb edits or reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StoreVerb {
    /// `/ignore`, `/mute`, `/highlight` or `/monitor`.
    Add,
    /// `/unignore`, `/unmute`, `/unhighlight` or `/unmonitor`.
    Remove,
    /// `/ignored`, `/muted`, `/highlights` or `/monitored`.
    List,
}

/// The two quiet-send verbs. `/msg` is PRIVMSG to a nick only; `/notice` is NOTICE to any target
/// (`IrcCommandDispatcher::QuietSend`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QuietSend {
    Msg,
    Notice,
}

/// Which channel-mode wrapper a `/op`, `/deop`, `/voice`, `/devoice`, `/ban`,
/// `/unban`, `/quiet` or `/unquiet` is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ChannelModeWrapper {
    modes: &'static str,
    /// When set, the parameter is a list mask and gets the shared nick expansion.
    list_mask: Option<ListMaskKind>,
}

/// A verb's words rejoined, so a verb that splits its argument (`Words`) reads the same text
/// back through the same split.
fn joined(words: &Words) -> String {
    if words.rest.is_empty() {
        words.first.clone()
    } else {
        format!("{} {}", words.first, words.rest)
    }
}

impl Controller {
    /// Runs one parsed command typed on `surface` (`IrcCommandDispatcher::dispatch`). The
    /// outcome is what the shell reads back. Pumps the events the verb's frames produced, so a
    /// demo reply is folded before this returns.
    pub fn dispatch(&mut self, surface: ComposerSurface, command: &Command) -> CommandOutcome {
        self.pump();
        let outcome = self.dispatch_command(surface, command);
        self.pump();
        outcome
    }

    /// Submits one composer line typed on `surface`, and returns whether the composer clears.
    /// On the conversation surface (`IrcController::sendMessage`), a refused line records the
    /// reason as the network's last error and keeps the text, so the caller leaves it in the
    /// composer. On Status (`IrcStatusConsole::submit`), a refusal is a Status line and the line
    /// clears. A blank line is never submitted.
    pub fn submit(&mut self, surface: ComposerSurface, text: &str) -> bool {
        let command = parse(text);
        if command == Command::Empty {
            return false;
        }
        let outcome = self.dispatch(surface, &command);
        match surface {
            ComposerSurface::Conversation => self.report(outcome, &command, surface),
            ComposerSurface::Status => {
                if outcome != CommandOutcome::Sent {
                    let network = self.console_network.clone();
                    if !network.is_empty() {
                        let text = command_outcome_text(outcome, &command);
                        self.log_outcome(&network, &text);
                    }
                    self.queue(vec![ViewNotify::Status]);
                }
                true
            }
        }
    }

    /// Records a conversation submission's outcome on its network and reports whether it was sent
    /// (`IrcController::report`).
    fn report(&mut self, outcome: CommandOutcome, command: &Command, surface: ComposerSurface) -> bool {
        let network = self.error_network_id(surface);
        if outcome == CommandOutcome::Sent {
            self.set_last_error(&network, None);
        } else {
            let text = self.outcome_error(outcome, command);
            self.set_last_error(&network, Some(text));
        }
        self.queue(vec![ViewNotify::Status]);
        outcome == CommandOutcome::Sent
    }

    /// The subtitle for a refused command. Chat on a channel you have left names that state
    /// every other refusal keeps the shared outcome text.
    fn outcome_error(&self, outcome: CommandOutcome, command: &Command) -> String {
        let chat = matches!(command, Command::Say(_) | Command::Action(_));
        if chat {
            if let Some(key) = self.selected.as_ref() {
                if let Some(conversation) = self.reducer.find(key) {
                    if conversation.is_channel() && !self.channel_is_joined(key) {
                        return "You have left this channel".to_owned();
                    }
                }
            }
        }
        command_outcome_text(outcome, command)
    }

    /// The network a refused command's subtitle belongs to: Status's network on Status, else the
    /// selection's, else Status's (`errorNetworkID`).
    fn error_network_id(&self, surface: ComposerSurface) -> String {
        match surface {
            ComposerSurface::Status => self.console_network.clone(),
            ComposerSurface::Conversation => match self.selected.as_ref() {
                Some(key) => key.network_id.clone(),
                None => self.console_network.clone(),
            },
        }
    }

    /// The network a verb reads its session from (`queryNetworkID`): Status's network on Status
    /// the selection's network on a conversation, and empty when nothing is selected.
    pub(super) fn surface_network(&self, surface: ComposerSurface) -> String {
        match surface {
            ComposerSurface::Status => self.console_network.clone(),
            ComposerSurface::Conversation => self
                .selected
                .as_ref()
                .map(|key| key.network_id.clone())
                .unwrap_or_default(),
        }
    }

    /// The live session of the surface's network (`sessionFor`), if one exists.
    pub(super) fn session_for(&self, surface: ComposerSurface) -> Option<Session> {
        let network = self.surface_network(surface);
        if network.is_empty() {
            return None;
        }
        self.session(&network)
    }

    /// The selected conversation's session, if the selection has one (`selectedSession`).
    pub(super) fn selected_session(&self) -> Option<Session> {
        let network = self.selected.as_ref()?.network_id.clone();
        self.session(&network)
    }

    /// Whether the selection is a direct message, or a row that is not a channel
    /// (`selectedIsCloseableDirect`). False with no selection.
    pub(super) fn selected_is_closeable_direct(&self) -> bool {
        match self.selected.as_ref() {
            None => false,
            Some(key) => self
                .reducer
                .find(key)
                .is_none_or(|conversation| !conversation.is_channel()),
        }
    }

    /// Whether the selected conversation is a channel the user is in (`ChannelJoined`).
    fn channel_is_joined(&self, key: &ConversationKey) -> bool {
        self.reducer
            .find(key)
            .and_then(|conversation| conversation.channel.as_ref())
            .is_some_and(|channel| channel.joined)
    }

    /// Whether the selection is a channel that is not joined (`/close` and chat refuse it).
    fn selected_channel_left(&self) -> bool {
        match self.selected.as_ref() {
            Some(key) => match self.reducer.find(key) {
                Some(conversation) if conversation.is_channel() => {
                    !self.channel_is_joined(key)
                }
                _ => false,
            },
            None => false,
        }
    }

    /// The routing table (`IrcCommandDispatcher::dispatch`). Each arm is one verb family.
    fn dispatch_command(&mut self, surface: ComposerSurface, command: &Command) -> CommandOutcome {
        match command {
            Command::Empty => return CommandOutcome::Sent,
            Command::Unknown(_) => return CommandOutcome::Unsupported,
            _ => {}
        }
        if !command.allowed_on(surface) {
            return CommandOutcome::WrongScope;
        }

        match command {
            Command::Say(text) => self.send_selected_message(text),
            Command::Action(text) => self.dispatch_action(text),
            Command::Query(words) => self.dispatch_query(&joined(words), surface),
            Command::Msg(words) => self.dispatch_quiet_send(QuietSend::Msg, &joined(words), surface),
            Command::Notice(words) => {
                self.dispatch_quiet_send(QuietSend::Notice, &joined(words), surface)
            }
            Command::Ns(text) => self.dispatch_service_msg("NickServ", text, surface),
            Command::Cs(text) => self.dispatch_service_msg("ChanServ", text, surface),
            Command::Znc(text) => self.dispatch_service_msg("*status", text, surface),
            Command::Mode(argument) => self.dispatch_mode(argument, surface),
            Command::Op(words) => self.dispatch_channel_mode_wrapper(
                ChannelModeWrapper {
                    modes: "+o",
                    list_mask: None,
                },
                words,
                surface,
            ),
            Command::Deop(words) => self.dispatch_channel_mode_wrapper(
                ChannelModeWrapper {
                    modes: "-o",
                    list_mask: None,
                },
                words,
                surface,
            ),
            Command::Voice(words) => self.dispatch_channel_mode_wrapper(
                ChannelModeWrapper {
                    modes: "+v",
                    list_mask: None,
                },
                words,
                surface,
            ),
            Command::Devoice(words) => self.dispatch_channel_mode_wrapper(
                ChannelModeWrapper {
                    modes: "-v",
                    list_mask: None,
                },
                words,
                surface,
            ),
            Command::Ban(words) => self.dispatch_channel_mode_wrapper(
                ChannelModeWrapper {
                    modes: "+b",
                    list_mask: Some(ListMaskKind::Ban),
                },
                words,
                surface,
            ),
            Command::Unban(words) => self.dispatch_channel_mode_wrapper(
                ChannelModeWrapper {
                    modes: "-b",
                    list_mask: Some(ListMaskKind::Ban),
                },
                words,
                surface,
            ),
            Command::Quiet(words) => self.dispatch_channel_mode_wrapper(
                ChannelModeWrapper {
                    modes: "+q",
                    list_mask: Some(ListMaskKind::Quiet),
                },
                words,
                surface,
            ),
            Command::Unquiet(words) => self.dispatch_channel_mode_wrapper(
                ChannelModeWrapper {
                    modes: "-q",
                    list_mask: Some(ListMaskKind::Quiet),
                },
                words,
                surface,
            ),
            Command::Bans(argument) => {
                self.dispatch_list_mode_inquiry(ListMaskKind::Ban, argument, surface)
            }
            Command::Quiets(argument) => {
                self.dispatch_list_mode_inquiry(ListMaskKind::Quiet, argument, surface)
            }
            Command::Raw(line) => self.dispatch_raw(line, surface),
            Command::Whois(words) => self.dispatch_whois(words, surface),
            Command::Ping(words) => self.dispatch_ctcp("PING", words, surface),
            Command::Time(words) => self.dispatch_ctcp("TIME", words, surface),
            Command::Version(words) => self.dispatch_ctcp("VERSION", words, surface),
            Command::Clear => self.clear_surface(surface),
            Command::Close => {
                if self.close_selected() {
                    CommandOutcome::Sent
                } else {
                    CommandOutcome::WrongScope
                }
            }
            Command::Topic(text) => self.set_selected_topic(text),
            Command::Ignore(words) => self.dispatch_ignore(StoreVerb::Add, &joined(words), surface),
            Command::Unignore(words) => {
                self.dispatch_ignore(StoreVerb::Remove, &joined(words), surface)
            }
            Command::Ignored(argument) => self.dispatch_ignore(StoreVerb::List, argument, surface),
            Command::Monitor(words) => self.dispatch_monitor(StoreVerb::Add, &joined(words), surface),
            Command::Unmonitor(words) => {
                self.dispatch_monitor(StoreVerb::Remove, &joined(words), surface)
            }
            Command::Monitored(argument) => {
                self.dispatch_monitor(StoreVerb::List, argument, surface)
            }
            Command::Mute(words) => self.dispatch_mute(StoreVerb::Add, &joined(words), surface),
            Command::Unmute(words) => self.dispatch_mute(StoreVerb::Remove, &joined(words), surface),
            Command::Muted(argument) => self.dispatch_mute(StoreVerb::List, argument, surface),
            Command::Highlight(words) => {
                self.dispatch_highlight(StoreVerb::Add, &joined(words), surface)
            }
            Command::Unhighlight(words) => {
                self.dispatch_highlight(StoreVerb::Remove, &joined(words), surface)
            }
            Command::Highlights(argument) => {
                self.dispatch_highlight(StoreVerb::List, argument, surface)
            }
            Command::Help => self.dispatch_help(surface),
            Command::Autoaway(argument) => self.dispatch_autoaway(argument, surface),
            Command::Pref(request) => self.dispatch_pref(*request, surface),
            Command::List(mask) => self.dispatch_list(mask, surface),
            Command::Status(argument) => self.dispatch_status(argument, surface),
            Command::Avatar(argument) => self.dispatch_avatar(argument, surface),
            Command::Join(_)
            | Command::Part(_)
            | Command::Nick(_)
            | Command::Disconnect(_)
            | Command::Kick(_)
            | Command::Kickban(_)
            | Command::Invite(_)
            | Command::Away(_)
            | Command::Back => self.dispatch_wire(command, surface),
            Command::Empty | Command::Unknown(_) => CommandOutcome::Unsupported,
        }
    }

    /// The verbs that need a registered session on the surface's network: the ones that write a
    /// frame the server answers (`dispatch`, the session-bound tail). `/disconnect` is the one
    /// verb that also runs on a session that is not registered yet.
    fn dispatch_wire(&mut self, command: &Command, surface: ComposerSurface) -> CommandOutcome {
        let Some(active) = self.session_for(surface) else {
            if surface == ComposerSurface::Conversation
                && self.selected.is_none()
                && !self.manager.network_ids().is_empty()
            {
                return CommandOutcome::Refused;
            }
            return CommandOutcome::NotConnected;
        };
        let quitting = matches!(command, Command::Disconnect(_));
        if active.state() != SessionState::Registered && !quitting {
            return CommandOutcome::NotConnected;
        }

        let sent = match command {
            Command::Join(argument) => return self.dispatch_join(&active, argument),
            Command::Part(words) => return self.dispatch_part(&active, words, surface),
            Command::Kick(words) => return self.dispatch_kick(&active, &joined(words), surface),
            Command::Kickban(words) => {
                return self.dispatch_kickban(&active, &joined(words), surface)
            }
            Command::Invite(words) => return self.dispatch_invite(&active, &joined(words), surface),
            Command::Nick(nick) => !nick.is_empty() && active.change_nick(nick),
            Command::Disconnect(reason) => {
                let network = active.network_id();
                self.disconnect_with_reason(&network, reason)
            }
            Command::Away(reason) => {
                let sent = active.set_away(reason);
                if sent {
                    let network = active.network_id();
                    if reason.trim().is_empty() {
                        self.note_away_cleared(&network);
                    } else {
                        self.note_manual_away(&network);
                    }
                }
                sent
            }
            Command::Back => {
                let sent = active.clear_away();
                if sent {
                    self.note_away_cleared(&active.network_id());
                }
                sent
            }
            _ => return CommandOutcome::Unsupported,
        };
        if sent {
            CommandOutcome::Sent
        } else {
            CommandOutcome::Refused
        }
    }

    /// `/join` (`dispatchJoin`). Each target writes one JOIN, erasing that channel's pending
    /// cancellation and closed mark first, so the demo's echo of the join is not dropped or parted.
    /// Both come back when the frame cannot be built. The last target opens and is selected.
    fn dispatch_join(&mut self, active: &Session, argument: &str) -> CommandOutcome {
        let network = active.network_id();
        let features = self.reducer.server_features(&network).clone();
        let targets: Vec<IrcJoinTarget> = if argument.is_empty() {
            let Some(pending) = active.pending_invite() else {
                return CommandOutcome::Refused;
            };
            match IrcJoinTarget::make(&pending.channel, None, &features) {
                Some(target) => vec![target],
                None => return CommandOutcome::Refused,
            }
        } else {
            match parse_join_targets(argument, &features) {
                Some(targets) => targets,
                None => return CommandOutcome::Refused,
            }
        };

        let mut sent = true;
        for target in &targets {
            let key = self.reducer.conversation_key(&network, target.channel());
            let was_closed = self.reducer.closed(&key);
            self.reducer.clear_closed(&key);
            let was_cancelled = self.cancelled_joins.remove(&key);
            let wrote = active.join(target);
            if !wrote {
                if was_closed {
                    self.reducer.note_closed(&key);
                }
                if was_cancelled {
                    self.cancelled_joins.insert(key);
                }
            }
            sent = wrote && sent;
        }
        match targets.last() {
            Some(last) if sent => {
                let channel = last.channel().to_owned();
                self.open_joined_channel(&network, &channel);
                CommandOutcome::Sent
            }
            _ => CommandOutcome::Refused,
        }
    }

    /// Opens a channel the user just joined, and selects it (`openJoinedChannel`). An explicit
    /// join brings a closed channel back.
    fn open_joined_channel(&mut self, network_id: &str, channel: &str) {
        if network_id.is_empty() || channel.is_empty() {
            return;
        }
        let key = self.reducer.conversation_key(network_id, channel);
        self.reducer.clear_closed(&key);
        if self
            .reducer
            .ensure_conversation(&key, channel, IrcConversationCause::ChannelState)
            .is_none()
        {
            return;
        }
        self.queue(vec![ViewNotify::Conversations]);
        self.select(network_id, channel);
    }

    /// `/part` (`dispatchPart`). With no channel it parts the selection, when the selection is a
    /// channel on this network. A channel you are in is parted and marked left, and its row
    /// stays. A channel you have left sends nothing and cancels any JOIN still in flight.
    fn dispatch_part(&mut self, active: &Session, words: &Words, surface: ComposerSurface) -> CommandOutcome {
        let mut channel = words.first.clone();
        if channel.is_empty() {
            if self.selected.is_none() || !self.is_channel() {
                return CommandOutcome::WrongScope;
            }
            let selected_network = self.selected.as_ref().map(|key| key.network_id.clone());
            if selected_network.as_deref() != Some(self.surface_network(surface).as_str()) {
                return CommandOutcome::Refused;
            }
            channel = self.selected_target().to_owned();
            if channel.is_empty() {
                return CommandOutcome::Refused;
            }
        }

        let network = active.network_id();
        let key = self.reducer.conversation_key(&network, &channel);
        let joined = self
            .reducer
            .find(&key)
            .and_then(|conversation| conversation.channel.as_ref())
            .map(|state| state.joined);
        if let Some(joined) = joined {
            if joined {
                if !active.part(&channel) {
                    return CommandOutcome::Refused;
                }
                self.mark_channel_left(&network, &channel);
                return CommandOutcome::Sent;
            }
            self.cancelled_joins.insert(key);
            return CommandOutcome::Sent;
        }
        if active.part(&channel) {
            CommandOutcome::Sent
        } else {
            CommandOutcome::Refused
        }
    }

    /// Marks a channel left in the reducer and wakes the models (`markChannelLeft`).
    pub(super) fn mark_channel_left(&mut self, network_id: &str, channel: &str) {
        if network_id.is_empty() || channel.is_empty() {
            return;
        }
        let key = self.reducer.conversation_key(network_id, channel);
        if self.reducer.mark_channel_left(&key) {
            self.queue(viewnotify::reset_all());
        }
    }

    /// `/kick [channel] <nick> [reason]` (`dispatchKick`). A first word that names a channel on the
    /// network chooses the channel; otherwise the selected channel is the target.
    fn dispatch_kick(&mut self, active: &Session, argument: &str, surface: ComposerSurface) -> CommandOutcome {
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

    /// `/kickban [channel] <nick> [reason]` (`/kb`). Scope matches `/kick`; the ban
    /// mask expands like `/ban`, and a token with `!` bans that mask while kicking
    /// the nick before `!`.
    fn dispatch_kickban(
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

    /// `/invite <nick> [channel]` (`dispatchInvite`). Without a channel the selected one is used
    /// and Status has none to name, so an invite there needs one.
    fn dispatch_invite(&mut self, active: &Session, argument: &str, surface: ComposerSurface) -> CommandOutcome {
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

    /// `/query <nick> [text]` (`dispatchQuery`). Opens or selects a direct message as the user
    /// asked, so it inserts the row (`UserOpen`). Text sends into it.
    fn dispatch_query(&mut self, argument: &str, surface: ComposerSurface) -> CommandOutcome {
        let words = Words::split(argument);
        if words.first.is_empty() {
            return CommandOutcome::Refused;
        }
        let network = self.surface_network(surface);
        if network.is_empty() {
            return refused_or_wrong_scope(surface);
        }
        let nick = words.first.clone();
        if self.reducer.server_features(&network).is_channel(&nick) {
            return CommandOutcome::Refused;
        }
        if !words.rest.is_empty() && self.registered_session(&network).is_none() {
            return CommandOutcome::NotConnected;
        }

        let key = self.reducer.conversation_key(&network, &nick);
        if self
            .reducer
            .ensure_conversation(&key, &nick, IrcConversationCause::UserOpen)
            .is_none()
        {
            return CommandOutcome::Refused;
        }
        self.remember_open_direct(&network, &nick);
        self.queue(vec![ViewNotify::Conversations]);
        self.select(&network, &nick);
        if words.rest.is_empty() {
            return CommandOutcome::Sent;
        }
        self.send_selected_message(&words.rest)
    }

    /// `/msg <nick> <text>` and `/notice <target> <text>` (`dispatchQuietSend`). The line goes out
    /// without opening or selecting a conversation: `/msg` never invents a direct message
    /// (`QuietSend`). The echo shows only where a conversation already exists, and a PRIVMSG
    /// counts as chat activity, so the user is brought back from away.
    fn dispatch_quiet_send(&mut self, kind: QuietSend, argument: &str, surface: ComposerSurface) -> CommandOutcome {
        let words = Words::split(argument);
        let target = words.first.clone();
        let body = words.rest.clone();
        if target.is_empty() || body.is_empty() {
            return CommandOutcome::Refused;
        }
        let network = self.surface_network(surface);
        if network.is_empty() {
            return refused_or_wrong_scope(surface);
        }
        if kind == QuietSend::Msg && self.reducer.server_features(&network).is_channel(&target) {
            return CommandOutcome::Refused;
        }
        let Some(active) = self.registered_session(&network) else {
            return CommandOutcome::NotConnected;
        };

        let sent = match kind {
            QuietSend::Msg => active.send_privmsg(&target, &body),
            QuietSend::Notice => active.send_notice(&target, &body),
        };
        if !sent {
            return CommandOutcome::Refused;
        }

        self.note_nick_delivery(&network, &target);
        if kind == QuietSend::Msg {
            // A private message the user sent is a query worth reopening (`rememberOpenDirect`).
            self.remember_open_direct(&network, &target);
        }
        let key = self.reducer.conversation_key(&network, &target);
        self.reducer
            .ensure_conversation(&key, &target, IrcConversationCause::QuietSend);
        let outgoing = match kind {
            QuietSend::Msg => Outgoing::Say,
            QuietSend::Notice => Outgoing::Notice,
        };
        self.echo_if_present(&active, &target, &body, outgoing);
        if kind == QuietSend::Msg {
            self.note_chat_sent(&network);
        }
        CommandOutcome::Sent
    }

    /// `/ns`, `/cs` and `/znc`: a `/msg` to the service (`dispatchServiceMsg`). The service nick
    /// is the target, so the line stays a quiet send and never opens a conversation.
    fn dispatch_service_msg(&mut self, service: &str, text: &str, surface: ComposerSurface) -> CommandOutcome {
        let argument = if text.is_empty() {
            service.to_owned()
        } else {
            format!("{service} {text}")
        };
        self.dispatch_quiet_send(QuietSend::Msg, &argument, surface)
    }

    /// `/mode <target>...` (`dispatchMode`). The argument is read against the network's channel
    /// modes, so a query and a change both map to the frame the server expects.
    fn dispatch_mode(&mut self, argument: &str, surface: ComposerSurface) -> CommandOutcome {
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
    fn dispatch_channel_mode_wrapper(
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

    /// `/raw <line>` and `/quote <line>`: one literal line, sent as given (`dispatchRaw`).
    fn dispatch_raw(&mut self, line: &str, surface: ComposerSurface) -> CommandOutcome {
        if line.is_empty() {
            return CommandOutcome::Refused;
        }
        let network = self.surface_network(surface);
        if network.is_empty() {
            return refused_or_wrong_scope(surface);
        }
        let Some(active) = self.registered_session(&network) else {
            return CommandOutcome::NotConnected;
        };
        outcome_of(active.send_raw(line))
    }

    /// `/me <text>` (`dispatch`'s action branch). A CTCP ACTION to the selected target. A channel
    /// you have left refuses it. A sent action is chat activity, and it clears the typing hint.
    fn dispatch_action(&mut self, text: &str) -> CommandOutcome {
        let Some(active) = self.selected_session() else {
            return CommandOutcome::WrongScope;
        };
        if self.selected.is_none() {
            return CommandOutcome::WrongScope;
        }
        if self.selected_channel_left() {
            return CommandOutcome::Refused;
        }
        let network = active.network_id();
        let target = self.selected_target().to_owned();
        if !active.send_action(&target, text) {
            return CommandOutcome::Refused;
        }
        self.note_nick_delivery(&network, &target);
        self.echo_if_present(&active, &target, text, Outgoing::Action);
        self.typing_target = None;
        self.note_chat_sent(&network);
        CommandOutcome::Sent
    }

    /// Plain text to the selected conversation (`sendSelectedMessageOutcome`). The message
    /// echoes as the server's own line when `echo-message` is on, and locally otherwise. The
    /// target becomes a direct message the user has now spoken in.
    pub(super) fn send_selected_message(&mut self, text: &str) -> CommandOutcome {
        let Some(active) = self.selected_session() else {
            return CommandOutcome::WrongScope;
        };
        if self.selected.is_none() {
            return CommandOutcome::WrongScope;
        }
        if self.selected_channel_left() {
            return CommandOutcome::Refused;
        }
        let network = active.network_id();
        let target = self.selected_target().to_owned();
        if !active.send_privmsg(&target, text) {
            return CommandOutcome::Refused;
        }
        self.note_nick_delivery(&network, &target);
        self.echo_if_present(&active, &target, text, Outgoing::Say);
        self.note_chat_sent(&network);
        CommandOutcome::Sent
    }

    /// `/list [mask]`. Opens the channel list on the surface's network. It writes `LIST`, or shows
    /// the cached rows for the same mask, through `list_channels`.
    fn dispatch_list(&mut self, mask: &str, surface: ComposerSurface) -> CommandOutcome {
        let network = self.surface_network(surface);
        if network.is_empty() {
            return if surface == ComposerSurface::Conversation && self.selected.is_none() {
                CommandOutcome::WrongScope
            } else {
                CommandOutcome::Refused
            };
        }
        match self.list_channels(&network, mask) {
            super::channellist::ListOutcome::Sent => CommandOutcome::Sent,
            super::channellist::ListOutcome::Refused => CommandOutcome::Refused,
            super::channellist::ListOutcome::NotConnected => CommandOutcome::NotConnected,
        }
    }

    /// `/clear`: empties the open Status log, or the selected conversation's lines
    /// (`clearSurface`).
    fn clear_surface(&mut self, surface: ComposerSurface) -> CommandOutcome {
        if surface == ComposerSurface::Status {
            let network = self.console_network.clone();
            if network.is_empty() {
                return CommandOutcome::Refused;
            }
            self.clear_status(&network);
            return CommandOutcome::Sent;
        }
        let Some(key) = self.selected.clone() else {
            return CommandOutcome::WrongScope;
        };
        let target = self.selected_target().to_owned();
        self.clear_conversation(&key.network_id, &target);
        CommandOutcome::Sent
    }

    /// `/topic [text]`. Sets the selected channel's topic. An empty topic is sent as nothing
    /// (`setSelectedTopic`).
    fn set_selected_topic(&mut self, topic: &str) -> CommandOutcome {
        if self.selected.is_none() || !self.is_channel() {
            return CommandOutcome::WrongScope;
        }
        if topic.is_empty() {
            return CommandOutcome::Sent;
        }
        let Some(active) = self.selected_session() else {
            return CommandOutcome::NotConnected;
        };
        if active.state() != SessionState::Registered {
            return CommandOutcome::NotConnected;
        }
        let channel = self.selected_target().to_owned();
        outcome_of(active.set_topic(&channel, topic))
    }

    /// `/ignore`, `/unignore` and `/ignored` (`dispatchIgnore`). The results are Status lines.
    fn dispatch_ignore(&mut self, verb: StoreVerb, argument: &str, surface: ComposerSurface) -> CommandOutcome {
        let Some(network) = self.store_network(surface) else {
            return self.store_refusal(surface);
        };
        if self.store_session_down(surface) {
            return CommandOutcome::NotConnected;
        }
        let features = self.reducer.server_features(&network).clone();
        let text = match verb {
            StoreVerb::List => {
                if !argument.is_empty() {
                    return CommandOutcome::Refused;
                }
                let nicks = self.ignored_nicks(&network);
                if nicks.is_empty() {
                    "Not ignoring anyone".to_owned()
                } else {
                    format!("Ignoring: {}", nicks.join(", "))
                }
            }
            StoreVerb::Add | StoreVerb::Remove => {
                let words = Words::split(argument);
                let nick = words.first.clone();
                if !words.rest.is_empty() || !ignore_nick_is_usable(&nick, &features) {
                    return CommandOutcome::Refused;
                }
                match verb {
                    StoreVerb::Add => {
                        if self.ignore_nick(&network, &nick) {
                            format!("Ignoring {nick}")
                        } else {
                            format!("Already ignoring {nick}")
                        }
                    }
                    _ => {
                        if self.unignore_nick(&network, &nick) {
                            format!("No longer ignoring {nick}")
                        } else {
                            format!("Not ignoring {nick}")
                        }
                    }
                }
            }
        };
        self.log_outcome(&network, &text);
        CommandOutcome::Sent
    }

    /// `/mute`, `/unmute` and `/muted` (`dispatchMute`). A bare `/mute` takes the selected target.
    fn dispatch_mute(&mut self, verb: StoreVerb, argument: &str, surface: ComposerSurface) -> CommandOutcome {
        let Some(network) = self.store_network(surface) else {
            return self.store_refusal(surface);
        };
        if self.store_session_down(surface) {
            return CommandOutcome::NotConnected;
        }
        let features = self.reducer.server_features(&network).clone();
        let text = match verb {
            StoreVerb::List => {
                if !argument.is_empty() {
                    return CommandOutcome::Refused;
                }
                let targets = self.mutes.listed(&network, features.case_mapping());
                if targets.is_empty() {
                    "Not muting anything".to_owned()
                } else {
                    format!("Muted: {}", targets.join(", "))
                }
            }
            StoreVerb::Add | StoreVerb::Remove => {
                let words = Words::split(argument);
                if !words.rest.is_empty() {
                    return CommandOutcome::Refused;
                }
                let mut target = words.first.clone();
                if target.is_empty() {
                    if surface == ComposerSurface::Status
                        || self.selected.is_none()
                    {
                        return CommandOutcome::WrongScope;
                    }
                    target = self.selected_target().to_owned();
                }
                if !mute_target_is_usable(&target, &features) {
                    return CommandOutcome::Refused;
                }
                match verb {
                    StoreVerb::Add => {
                        if self.apply_mute(&network, &target, true) {
                            format!("Muted {target}")
                        } else {
                            format!("Already muted {target}")
                        }
                    }
                    _ => {
                        if self.apply_mute(&network, &target, false) {
                            format!("No longer muted {target}")
                        } else {
                            format!("Not muted {target}")
                        }
                    }
                }
            }
        };
        self.log_outcome(&network, &text);
        CommandOutcome::Sent
    }

    /// `/highlight`, `/unhighlight` and `/highlights` (`dispatchHighlight`). The word list feeds
    /// the reducer, so a mention matches at once.
    fn dispatch_highlight(&mut self, verb: StoreVerb, argument: &str, surface: ComposerSurface) -> CommandOutcome {
        let Some(network) = self.store_network(surface) else {
            return self.store_refusal(surface);
        };
        if self.store_session_down(surface) {
            return CommandOutcome::NotConnected;
        }
        let text = match verb {
            StoreVerb::List => {
                if !argument.is_empty() {
                    return CommandOutcome::Refused;
                }
                let words = self.highlight_words(&network);
                if words.is_empty() {
                    "No highlight words".to_owned()
                } else {
                    format!("Highlights: {}", words.join(", "))
                }
            }
            StoreVerb::Add | StoreVerb::Remove => {
                let words = Words::split(argument);
                if words.first.is_empty() || !words.rest.is_empty() {
                    return CommandOutcome::Refused;
                }
                let word = words.first.clone();
                match verb {
                    StoreVerb::Add => {
                        if self.add_highlight_word(&network, &word) {
                            format!("Highlighting {word}")
                        } else {
                            format!("Already highlighting {word}")
                        }
                    }
                    _ => {
                        if self.remove_highlight_word(&network, &word) {
                            format!("No longer highlighting {word}")
                        } else {
                            format!("Not highlighting {word}")
                        }
                    }
                }
            }
        };
        self.log_outcome(&network, &text);
        CommandOutcome::Sent
    }

    /// `/monitor`, `/unmonitor` and `/monitored`. The coordinator owns the list, the frames and
    /// the Status text, so the dispatcher maps its outcome.
    fn dispatch_monitor(&mut self, verb: StoreVerb, argument: &str, surface: ComposerSurface) -> CommandOutcome {
        let network = self.surface_network(surface);
        if network.is_empty() {
            return refused_or_wrong_scope(surface);
        }
        let outcome = match verb {
            StoreVerb::Add => self.monitor_add(&network, argument),
            StoreVerb::Remove => self.monitor_remove(&network, argument),
            StoreVerb::List => self.monitor_list(&network, argument),
        };
        match outcome {
            MonitorOutcome::Sent => CommandOutcome::Sent,
            MonitorOutcome::Refused => CommandOutcome::Refused,
            MonitorOutcome::NotConnected => CommandOutcome::NotConnected,
        }
    }

    /// `/help` prints the verb catalog: into the conversation when a conversation is the
    /// surface, else to Status (`dispatchHelp`).
    fn dispatch_help(&mut self, surface: ComposerSurface) -> CommandOutcome {
        let mut network = self.surface_network(surface);
        if network.is_empty() {
            network = self.console_network.clone();
        }
        if network.is_empty() {
            return CommandOutcome::Refused;
        }
        let text = help_text();
        if surface == ComposerSurface::Conversation {
            let Some(key) = self.selected.clone() else {
                return CommandOutcome::WrongScope;
            };
            self.apply_whois_transcript(&key, &text);
            return CommandOutcome::Sent;
        }
        self.log_outcome(&network, &text);
        CommandOutcome::Sent
    }

    /// `/autoaway...`. The timers and settings are the auto-away runtime's; the reply is one
    /// line (`dispatchAutoaway`).
    fn dispatch_autoaway(&mut self, argument: &str, surface: ComposerSurface) -> CommandOutcome {
        let request = parse_autoaway_argument(argument);
        let text = self.autoaway_command(request);
        self.echo_autoaway_feedback(surface, &text)
    }

    /// Where an auto-away reply goes: the selected conversation, or Status (`autoawayEchoFeedback`).
    fn echo_autoaway_feedback(&mut self, surface: ComposerSurface, text: &str) -> CommandOutcome {
        let mut network = self.surface_network(surface);
        if network.is_empty() {
            network = self.console_network.clone();
        }
        if network.is_empty() {
            return CommandOutcome::Refused;
        }
        if surface == ComposerSurface::Conversation {
            let Some(key) = self.selected.clone() else {
                return CommandOutcome::WrongScope;
            };
            self.apply_whois_transcript(&key, text);
            return CommandOutcome::Sent;
        }
        self.log_outcome(&network, text);
        CommandOutcome::Sent
    }

    /// `/pref`, read or set (`dispatchPref`). A set writes the preference at once, and a
    /// membership-noise change applies to the lines that follow it.
    fn dispatch_pref(&mut self, request: PrefRequest, surface: ComposerSurface) -> CommandOutcome {
        let reply = pref_reply(request, &mut self.prefs);
        if reply.changed {
            if request.name == crate::irc::pref::PrefName::Joins {
                let noise = self.prefs.joins;
                self.reducer.set_membership_noise(noise);
                self.queue(vec![ViewNotify::Messages]);
            }
            self.after_preference_change(request.name);
        }
        self.echo_pref_feedback(surface, &reply.text)
    }

    /// Prints `/pref` output into the selected conversation, or to Status (`echoPrefFeedback`).
    fn echo_pref_feedback(&mut self, surface: ComposerSurface, text: &str) -> CommandOutcome {
        if surface == ComposerSurface::Conversation {
            if let Some(key) = self.selected.clone() {
                for line in text.lines() {
                    self.apply_whois_transcript(&key, line);
                }
            }
            return CommandOutcome::Sent;
        }
        let network = self.console_network.clone();
        if !network.is_empty() {
            for line in text.lines() {
                self.log_outcome(&network, line);
            }
        }
        CommandOutcome::Sent
    }

    /// The network a store verb works on, or `None` when the surface cannot name one.
    fn store_network(&self, surface: ComposerSurface) -> Option<String> {
        let network = self.surface_network(surface);
        (!network.is_empty()).then_some(network)
    }

    /// The refusal for a store verb whose surface names no network: a conversation with no
    /// selection is a wrong scope, and Status with no network is a refusal.
    fn store_refusal(&self, surface: ComposerSurface) -> CommandOutcome {
        if surface == ComposerSurface::Conversation && self.selected.is_none() {
            return CommandOutcome::WrongScope;
        }
        CommandOutcome::Refused
    }

    /// Whether the surface's session is down (idle or failed), which a store verb refuses.
    fn store_session_down(&self, surface: ComposerSurface) -> bool {
        match self.session_for(surface) {
            None => true,
            Some(session) => matches!(session.state(), SessionState::Idle | SessionState::Failed),
        }
    }

    /// The session on `network_id`, when it is registered.
    fn registered_session(&self, network_id: &str) -> Option<Session> {
        self.session(network_id)
            .filter(|session| session.state() == SessionState::Registered)
    }

}

/// The saved preferences the dispatcher and the Connect toggles share. The controller loads
/// them in `load_stored` and stores each change as it is made.
impl Controller {
    /// The preferences `/pref` reads and writes.
    pub fn preferences(&self) -> &Preferences {
        &self.prefs
    }

    /// Sets one preference (a Connect toggle). Returns false, and changes nothing, when the
    /// value's kind does not fit the name. A change is stored at once, with its side effects.
    pub fn set_preference(&mut self, name: PrefName, value: PrefValue) -> bool {
        if !self.prefs.set(name, value) {
            return false;
        }
        if name == PrefName::Joins {
            self.reducer.set_membership_noise(self.prefs.joins);
            self.queue(vec![ViewNotify::Messages]);
        }
        self.after_preference_change(name);
        true
    }
}

/// The outcome of one written frame: sent when it wrote, refused when it did not.
fn outcome_of(sent: bool) -> CommandOutcome {
    if sent {
        CommandOutcome::Sent
    } else {
        CommandOutcome::Refused
    }
}

/// The outcome for a verb whose network the surface cannot name: a conversation with no
/// selection is a wrong scope, and Status with no network is a refusal (`dispatchQuery` and the
/// quiet-send family).
pub(super) fn refused_or_wrong_scope(surface: ComposerSurface) -> CommandOutcome {
    match surface {
        ComposerSurface::Conversation => CommandOutcome::WrongScope,
        ComposerSurface::Status => CommandOutcome::Refused,
    }
}
