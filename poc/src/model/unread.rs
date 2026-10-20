//! Unread counts, the "New messages" mark and the unseen jump. Ports the visit
//! and arrival rules of `src/irc/irceventreducer.cpp` (`markSelected`,
//! `markRead`, and the live path of the chat arrival), `unreadMarkRow` in
//! `src/irc/messagelistmodel.cpp`, and `jumpToUnseen` / `pinToUnread` in
//! `src/qml/TranscriptList.qml`.
//!
//! The mark is a stored line of kind `UNREAD_LINE`, planted in front of the first
//! chat line that arrived unseen. `transcript::rows_from_messages` turns it into a
//! `RowKind::UnreadMark` row, and drops it while no row sits above it, as Qt does.
//! Keeping it among the stored lines means `/clear` and `/pref joins` folding see
//! it without an index to keep in step.
//!
//! Arrivals come only through `Shell::ingest`. The demo never calls it after
//! launch, so the seeded world shows no mark. Qt's demo replays its seed and
//! reads it, which also leaves no mark, while its unread counts remain.

use super::ids::ConversationRef;
use super::mention::mentions_nick;
use super::mock::{Conversation, MockMessage, MockWorld};
use super::transcript::{row_for_message, Row, RowKind};

/// The kind of the stored line that marks where unread lines begin.
pub const UNREAD_LINE: &str = "unread";

/// The stored mark line.
pub fn mark_line() -> MockMessage {
    MockMessage {
        kind: UNREAD_LINE.to_string(),
        author: String::new(),
        time: String::new(),
        day: String::new(),
        body: String::new(),
    }
}

/// Whether a stored line is the mark.
pub fn is_mark(message: &MockMessage) -> bool {
    message.kind == UNREAD_LINE
}

/// Whether a line counts toward unread (Qt `chatCountsTowardUnread`). The PoC has
/// no notices.
fn counts_toward_unread(message: &MockMessage) -> bool {
    matches!(message.kind.as_str(), "message" | "action")
}

fn remove_marks(conv: &mut Conversation) {
    conv.messages.retain(|message| !is_mark(message));
}

/// What one arrival did to its conversation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Arrival {
    /// The line planted a mark in front of itself.
    pub marked: bool,
}

/// Entering a conversation (Qt `markSelected`). The unread count and mention
/// flag clear. The mark clears only when nothing was unread, so the visit that
/// reads the unread still opens at its mark, and the next visit does not.
pub fn enter(conv: &mut Conversation) {
    if conv.unread == 0 {
        remove_marks(conv);
    }
    conv.mark_read();
}

/// The window regains focus (Qt `IrcController::setWindowActive(true)`). The open
/// conversation reads, and its mark stays until the next visit.
pub fn focus_gained(conv: &mut Conversation) {
    conv.mark_read();
}

/// Lands a chat line in `conv`, as the live arrival path does. `nick` is the local
/// nick, whose own lines never count. `seen` is true when the reader sees the line
/// now: the conversation is the selected one and the window is active. An unseen
/// line counts, and when nothing was unread it plants the mark in front of itself.
/// A mention sets the flag. Other lines are simply appended.
pub fn arrive(conv: &mut Conversation, nick: &str, line: MockMessage, seen: bool) -> Arrival {
    let mut arrival = Arrival::default();
    let from_self = line.author.eq_ignore_ascii_case(nick);
    if !seen && !from_self && counts_toward_unread(&line) {
        if conv.unread == 0 {
            remove_marks(conv);
            conv.messages.push(mark_line());
            arrival.marked = true;
        }
        conv.unread += 1;
        if mentions_nick(&line.body, nick) {
            conv.mention = true;
        }
    }
    conv.messages.push(line);
    arrival
}

/// The conversation a line for `network` and `name` lands in, matched the way the
/// sidebar names it. `None` when the world does not hold it: an arrival never
/// invents a conversation in the PoC.
pub fn find_target(world: &MockWorld, network: &str, name: &str) -> Option<ConversationRef> {
    let network = world.network(network)?;
    let conv = network
        .conversations
        .iter()
        .find(|conv| conv.name.eq_ignore_ascii_case(name))?;
    Some(ConversationRef::new(network.id.clone(), conv.key()))
}

/// Lands `line` in the conversation `target`, using that network's own nick.
/// `None` when the world does not hold `target`, and the world is left alone.
pub fn arrive_in(
    world: &mut MockWorld,
    target: &ConversationRef,
    line: MockMessage,
    seen: bool,
) -> Option<Arrival> {
    let nick = world.network(&target.network_id)?.nick.clone();
    let conv = world.find_mut(target)?;
    Some(arrive(conv, &nick, line, seen))
}

/// The row of the open "New messages" mark, if the rows hold one.
pub fn unread_mark_row(rows: &[Row]) -> Option<usize> {
    rows.iter().position(|row| row.kind == RowKind::UnreadMark)
}

/// The row a conversation opens at when `open_at_unread` is on: its mark (Qt
/// `pinTranscriptToUnreadOr`). `None` opens at the end.
pub fn opening_row(rows: &[Row], open_at_unread: bool) -> Option<usize> {
    open_at_unread.then(|| unread_mark_row(rows)).flatten()
}

/// Where the unseen jump lands (`↓` chip and Alt+U).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnseenLanding {
    /// The first line that arrived while the reader was scrolled up.
    Row(usize),
    /// The end, while a mark is open and no arrival is waiting.
    End,
}

/// What the unseen jump does from this view (Qt `jumpArmed` and `jumpToUnseen`).
/// Nothing while the reader follows the end. A waiting arrival wins over the mark.
pub fn unseen_landing(
    detached: bool,
    first_unseen: Option<usize>,
    rows: &[Row],
) -> Option<UnseenLanding> {
    if !detached {
        return None;
    }
    if let Some(row) = first_unseen.filter(|&row| row < rows.len()) {
        return Some(UnseenLanding::Row(row));
    }
    unread_mark_row(rows).map(|_| UnseenLanding::End)
}

/// Whether a stored line shows a row, given the row above it. A mark with no row
/// above is not shown (Qt `!view.empty()`). Shared by `rows_from_messages` and the
/// transcript's arrival path.
pub fn shows_row(message: &MockMessage, previous: Option<&Row>) -> bool {
    !(is_mark(message) && previous.is_none())
}

/// The row for one arrival appended under `previous`, or `None` when it shows
/// none (a mark with nothing above it).
pub fn arrival_row(id: u64, message: &MockMessage, previous: Option<&Row>) -> Option<Row> {
    shows_row(message, previous).then(|| row_for_message(id, message, previous))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::load_world;
    use crate::model::membership::membership_view;
    use crate::model::prefs::MembershipNoise;
    use crate::model::transcript::rows_from_messages;

    const NICK: &str = "fred";

    fn chat(author: &str, body: &str) -> MockMessage {
        MockMessage {
            kind: "message".to_string(),
            author: author.to_string(),
            time: "09:00".to_string(),
            day: "2026-10-20".to_string(),
            body: body.to_string(),
        }
    }

    fn event(kind: &str, author: &str) -> MockMessage {
        MockMessage {
            kind: kind.to_string(),
            author: author.to_string(),
            time: String::new(),
            day: String::new(),
            body: String::new(),
        }
    }

    fn channel(messages: Vec<MockMessage>) -> Conversation {
        Conversation {
            name: "#omarchy".to_string(),
            channel: true,
            topic: None,
            peer_realname: None,
            unread: 0,
            mention: false,
            typing: false,
            members: Vec::new(),
            messages,
        }
    }

    fn kinds(rows: &[Row]) -> Vec<RowKind> {
        rows.iter().map(|row| row.kind).collect()
    }

    fn mark_count(conv: &Conversation) -> usize {
        conv.messages.iter().filter(|m| is_mark(m)).count()
    }

    #[test]
    fn the_first_unseen_line_plants_the_mark_and_later_ones_leave_it() {
        let mut conv = channel(vec![chat("anna", "old")]);
        assert!(arrive(&mut conv, NICK, chat("dax", "one"), false).marked);
        assert!(!arrive(&mut conv, NICK, chat("dax", "two"), false).marked);
        assert_eq!(conv.unread, 2);

        let rows = rows_from_messages(&conv.messages);
        assert_eq!(
            kinds(&rows),
            [
                RowKind::Message,
                RowKind::UnreadMark,
                RowKind::Message,
                RowKind::Message
            ]
        );
        assert_eq!(unread_mark_row(&rows), Some(1));
    }

    #[test]
    fn a_line_seen_on_screen_neither_counts_nor_marks() {
        let mut conv = channel(vec![chat("anna", "old")]);
        let arrival = arrive(&mut conv, NICK, chat("dax", "live"), true);
        assert!(!arrival.marked);
        assert_eq!(conv.unread, 0);
        assert_eq!(mark_count(&conv), 0);
        assert_eq!(conv.messages.len(), 2);
    }

    #[test]
    fn own_lines_and_events_never_count_or_mark() {
        let mut conv = channel(Vec::new());
        arrive(&mut conv, NICK, chat(NICK, "mine"), false);
        arrive(&mut conv, NICK, event("join", "sol"), false);
        assert_eq!(conv.unread, 0);
        assert_eq!(mark_count(&conv), 0);
        assert_eq!(conv.messages.len(), 2);
    }

    #[test]
    fn a_mention_arrival_sets_the_flag_and_a_longer_word_does_not() {
        let mut named = channel(Vec::new());
        arrive(&mut named, NICK, chat("mira", "looks good, fred."), false);
        assert!(named.mention);

        let mut other = channel(Vec::new());
        arrive(&mut other, NICK, chat("mira", "fredo is here"), false);
        assert!(!other.mention);
        assert_eq!(other.unread, 1);
    }

    #[test]
    fn a_mark_with_no_row_above_is_not_shown() {
        let mut conv = channel(Vec::new());
        arrive(&mut conv, NICK, chat("dax", "first"), false);
        assert_eq!(mark_count(&conv), 1);

        let rows = rows_from_messages(&conv.messages);
        assert_eq!(kinds(&rows), [RowKind::Message]);
        assert_eq!(unread_mark_row(&rows), None);
    }

    #[test]
    fn a_mark_after_a_day_line_sits_below_the_day_rule() {
        let mut conv = channel(vec![MockMessage {
            kind: "day".to_string(),
            author: String::new(),
            time: String::new(),
            day: String::new(),
            body: "-- Monday --".to_string(),
        }]);
        arrive(&mut conv, NICK, chat("dax", "after"), false);
        let rows = rows_from_messages(&conv.messages);
        assert_eq!(
            kinds(&rows),
            [RowKind::DaySeparator, RowKind::UnreadMark, RowKind::Message]
        );
    }

    #[test]
    fn a_later_unseen_line_moves_the_mark_after_a_read() {
        let mut conv = channel(Vec::new());
        arrive(&mut conv, NICK, chat("dax", "one"), false);
        focus_gained(&mut conv);
        assert_eq!(conv.unread, 0);
        // The read keeps the mark, so the next unseen line replaces it.
        assert_eq!(mark_count(&conv), 1);

        arrive(&mut conv, NICK, chat("dax", "two"), false);
        assert_eq!(mark_count(&conv), 1);
        let rows = rows_from_messages(&conv.messages);
        let mark = unread_mark_row(&rows).expect("a mark");
        assert_eq!(rows[mark + 1].body.plain, "two");
    }

    #[test]
    fn focus_gained_reads_the_open_conversation_and_keeps_its_mark() {
        let mut conv = channel(Vec::new());
        arrive(&mut conv, NICK, chat("dax", "one"), false);
        arrive(&mut conv, NICK, chat("dax", "two"), false);
        focus_gained(&mut conv);
        assert_eq!(conv.unread, 0);
        assert_eq!(mark_count(&conv), 1);
    }

    #[test]
    fn entering_keeps_the_mark_for_the_visit_that_reads_it() {
        let mut conv = channel(Vec::new());
        arrive(&mut conv, NICK, chat("dax", "one"), false);
        arrive(&mut conv, NICK, chat("dax", "two"), false);

        enter(&mut conv);
        assert_eq!(conv.unread, 0);
        assert_eq!(
            mark_count(&conv),
            1,
            "the visit that reads the unread opens at it"
        );

        enter(&mut conv);
        assert_eq!(mark_count(&conv), 0, "the next visit clears it");
    }

    #[test]
    fn the_open_at_unread_pref_picks_the_mark_or_the_end() {
        let mut conv = channel(vec![chat("anna", "old")]);
        arrive(&mut conv, NICK, chat("dax", "new"), false);
        let rows = rows_from_messages(&conv.messages);
        assert_eq!(opening_row(&rows, true), Some(1));
        assert_eq!(opening_row(&rows, false), None);
    }

    #[test]
    fn a_folded_join_run_keeps_the_mark_in_front_of_its_line() {
        let mut conv = channel(vec![
            chat("anna", "before"),
            event("join", "sol"),
            event("join", "nora"),
        ]);
        arrive(&mut conv, NICK, chat("dax", "after"), false);
        let shown = membership_view(&conv.messages, MembershipNoise::Folded);
        let rows = rows_from_messages(&shown);
        assert_eq!(
            kinds(&rows),
            [
                RowKind::Message,
                RowKind::Event,
                RowKind::UnreadMark,
                RowKind::Message
            ]
        );
    }

    #[test]
    fn the_unseen_landing_follows_the_end_and_the_first_arrival() {
        let mut conv = channel(vec![chat("anna", "old")]);
        arrive(&mut conv, NICK, chat("dax", "new"), false);
        let rows = rows_from_messages(&conv.messages);
        assert_eq!(unread_mark_row(&rows), Some(1));

        // Following the end never jumps, mark or not.
        assert_eq!(unseen_landing(false, Some(2), &rows), None);
        // A waiting arrival wins over the mark.
        assert_eq!(
            unseen_landing(true, Some(2), &rows),
            Some(UnseenLanding::Row(2))
        );
        // No arrival waits, so the mark lands on the end.
        assert_eq!(unseen_landing(true, None, &rows), Some(UnseenLanding::End));
        // A waiting row that no longer exists is not a landing.
        assert_eq!(unseen_landing(true, Some(9), &[]), None);
        // Nothing waits and no mark is open.
        let plain = rows_from_messages(&[chat("anna", "only")]);
        assert_eq!(unseen_landing(true, None, &plain), None);
    }

    #[test]
    fn the_seeded_world_shows_no_mark_at_launch() {
        let world = load_world();
        for network in &world.networks {
            for conv in &network.conversations {
                let rows = rows_from_messages(&conv.messages);
                assert_eq!(
                    unread_mark_row(&rows),
                    None,
                    "{} in {} opens with a mark",
                    conv.name,
                    network.id
                );
            }
        }
        // The unread counts stay: they drive the sidebar badges and Alt+A.
        let ricing = world
            .find(&ConversationRef::new("omarchy", "#ricing"))
            .expect("#ricing");
        assert_eq!(ricing.unread, 12);
    }

    #[test]
    fn an_arrival_lands_only_in_a_conversation_the_world_holds() {
        let mut world = load_world();
        let found = find_target(&world, "omarchy", "#OMARCHY").expect("a case-insensitive channel");
        assert_eq!(found, ConversationRef::new("omarchy", "#omarchy"));
        assert_eq!(find_target(&world, "omarchy", "#nope"), None);
        assert_eq!(find_target(&world, "nope", "anna"), None);

        let lines = |w: &MockWorld| {
            w.networks
                .iter()
                .map(|n| {
                    n.conversations
                        .iter()
                        .map(|c| c.messages.len())
                        .sum::<usize>()
                })
                .sum::<usize>()
        };
        let before = lines(&world);
        let missing = ConversationRef::new("omarchy", "nobody");
        assert_eq!(
            arrive_in(&mut world, &missing, chat("dax", "hi"), false),
            None
        );
        assert_eq!(lines(&world), before);
    }

    #[test]
    fn own_lines_follow_the_nick_of_the_network_they_land_on() {
        let mut world = load_world();
        // Oak is the OFTC nick, so a line from oak is the local user's there.
        let target = ConversationRef::new("oftc", "#omarchy");
        let arrival = arrive_in(&mut world, &target, chat("oak", "mine"), false).expect("landed");
        assert!(!arrival.marked);
        assert_eq!(world.find(&target).expect("oftc #omarchy").unread, 0);
    }

    #[test]
    fn a_mark_line_is_only_a_mark_by_its_kind() {
        assert!(is_mark(&mark_line()));
        assert!(!is_mark(&chat("anna", "hi")));
    }

    #[test]
    fn alt_a_lands_on_an_arrival_until_a_visit_reads_it() {
        use crate::model::navigation::{mark_all_read, next_unread};
        use crate::model::sidebar::{sidebar_rows, SidebarState};
        let mut world = load_world();
        mark_all_read(&mut world);
        let desktop = ConversationRef::new("omarchy", "#desktop");
        arrive_in(&mut world, &desktop, chat("dax", "hello"), false).expect("landed");
        let rows = sidebar_rows(&world, &SidebarState::default());
        assert_eq!(next_unread(&world, &rows, None), Some(desktop.clone()));

        // Visiting reads it, so Alt+A has nothing left to land on.
        enter(world.find_mut(&desktop).expect("desktop"));
        assert_eq!(next_unread(&world, &rows, None), None);
    }
}
