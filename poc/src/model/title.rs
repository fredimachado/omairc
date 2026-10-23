//! Header and window titles. Ports Qt's duplicate-name rule: a conversation
//! name shows its network only when that name exists on two networks, so
//! `#omarchy` reads `#omarchy · irc.example · fred` while `#ricing` stays bare.
//!
//! The names and the conversation list come from the controller, so the titles
//! follow the same state the header and the sidebar draw.

use std::collections::HashSet;

use super::ids::ConversationRef;
use super::status::status_title;
use crate::controller::controller::Controller;

/// An unfocused mention or direct message held in the window title. Body is plain
/// IRC text; [`attention_title`] collapses leftover controls. Opening that
/// conversation, or focusing the window, drops the mark. Matches `TitleMark.qml`
/// and `tui/internal/ui/title.go`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TitleMark {
    pub author: String,
    pub plain_body: String,
    pub network_id: String,
    pub target: String,
}

/// What the centre pane shows, which picks the window title.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TitleView<'a> {
    /// A conversation, or `None` when nothing is selected.
    Conversation(Option<&'a ConversationRef>),
    /// The Status pane, named by its network's display name (empty when it has none).
    Status(&'a str),
}

/// True when some other network also has a conversation with this name.
pub fn needs_network_suffix(controller: &Controller, name: &str) -> bool {
    let networks: HashSet<String> = controller
        .conversations()
        .into_iter()
        .filter(|row| row.target == name)
        .map(|row| row.key.network_id)
        .collect();
    networks.len() > 1
}

/// The header title for `target`, or `None` when the conversation is gone.
pub fn conversation_title(controller: &Controller, target: &ConversationRef) -> Option<String> {
    let row = controller.conversations().into_iter().find(|row| {
        row.key.network_id == target.network_id && row.key.normalized_target == target.key
    })?;
    if needs_network_suffix(controller, &row.target) {
        let network = controller.network_name(&target.network_id);
        Some(format!("{} · {}", row.target, network))
    } else {
        Some(row.target)
    }
}

/// Drops C0, C1, and DEL (ESC, BEL, newlines) and folds the gap into one space.
/// IRC formatting is stripped before this runs.
pub fn collapse_title_space(text: &str) -> String {
    let mut out = String::new();
    let mut pending_space = false;
    for ch in text.chars() {
        let code = ch as u32;
        if code <= 0x20 || code == 0x7f || (code >= 0x80 && code <= 0x9f) {
            if !out.is_empty() {
                pending_space = true;
            }
            continue;
        }
        if pending_space {
            out.push(' ');
            pending_space = false;
        }
        out.push(ch);
    }
    out
}

/// Formats an unfocused mention or direct message. `place` is the conversation,
/// plus the network display name when that target is duplicated. An author that
/// already is the place (a direct message) is not repeated.
pub fn attention_title(author: &str, body: &str, place: &str) -> String {
    let mut who = collapse_title_space(author);
    let text = collapse_title_space(body);
    if who.is_empty() {
        who = place.to_owned();
    }
    let repeat_place = place == who;
    let lead = if text.is_empty() {
        who
    } else {
        format!("{who}: {text}")
    };
    if !place.is_empty() && !repeat_place {
        format!("{lead} · {place} - Omairc")
    } else {
        format!("{lead} - Omairc")
    }
}

/// The conversation label inside the attention title, with the network display name
/// when that target exists on more than one network.
pub fn title_mark_place(controller: &Controller, mark: &TitleMark) -> String {
    let conversation = controller.conversations().into_iter().find(|row| {
        row.key.network_id == mark.network_id && row.target == mark.target
    });
    if let Some(row) = conversation {
        return conversation_title(controller, &ConversationRef::from_key(&row.key))
            .unwrap_or(mark.target.clone());
    }
    if needs_network_suffix(controller, &mark.target) {
        let network = controller.network_name(&mark.network_id);
        format!("{} · {}", mark.target, network)
    } else {
        mark.target.clone()
    }
}

/// Records an unfocused mention or direct message. A focused window, or an arrival
/// with no conversation, leaves `mark` unchanged.
pub fn note_title_mark(
    mark: &mut Option<TitleMark>,
    window_active: bool,
    author: &str,
    plain_body: &str,
    network_id: &str,
    target: &str,
) {
    if window_active || target.is_empty() {
        return;
    }
    *mark = Some(TitleMark {
        author: author.to_owned(),
        plain_body: plain_body.to_owned(),
        network_id: network_id.to_owned(),
        target: target.to_owned(),
    });
}

/// Drops the mark when the open conversation is the one it names. Status is not
/// that conversation, so a console keeps the mark.
pub fn clear_title_mark_if_opened(
    mark: &mut Option<TitleMark>,
    showing_status: bool,
    network_id: &str,
    target: &str,
) {
    if showing_status || target.is_empty() {
        return;
    }
    if mark
        .as_ref()
        .is_some_and(|m| m.network_id == network_id && m.target == target)
    {
        *mark = None;
    }
}

impl TitleMark {
    /// The OSC window title while this mark is set.
    pub fn window_title(&self, controller: &Controller) -> String {
        attention_title(
            &self.author,
            &self.plain_body,
            &title_mark_place(controller, self),
        )
    }
}

/// The window title for the view. A conversation reads `<title> - Omairc`, or
/// just `Omairc` with nothing selected. A Status title has no ` - Omairc`
/// suffix, as Qt's `statusTitleText` does.
pub fn window_title(controller: &Controller, view: TitleView<'_>) -> String {
    match view {
        TitleView::Conversation(target) => {
            match target.and_then(|t| conversation_title(controller, t)) {
                Some(title) => format!("{title} - Omairc"),
                None => "Omairc".to_string(),
            }
        }
        TitleView::Status(name) => {
            if name.is_empty() {
                "Omairc".to_string()
            } else {
                status_title(name)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn omarchy(key: &str) -> ConversationRef {
        ConversationRef::new("omarchy", key)
    }

    #[test]
    fn a_name_on_two_networks_shows_its_network_and_a_unique_one_does_not() {
        let controller = Controller::seeded_for_tests();
        assert_eq!(
            conversation_title(&controller, &omarchy("#omarchy")).as_deref(),
            Some("#omarchy · irc.example · fred")
        );
        assert_eq!(
            conversation_title(&controller, &ConversationRef::new("oftc", "#omarchy")).as_deref(),
            Some("#omarchy · irc.example · oak")
        );
        assert_eq!(
            conversation_title(&controller, &omarchy("#ricing")).as_deref(),
            Some("#ricing")
        );
    }

    #[test]
    fn the_window_title_adds_the_app_name_and_status_does_not() {
        let controller = Controller::seeded_for_tests();
        assert_eq!(
            window_title(&controller, TitleView::Conversation(Some(&omarchy("#omarchy")))),
            "#omarchy · irc.example · fred - Omairc"
        );
        assert_eq!(
            window_title(&controller, TitleView::Conversation(Some(&omarchy("#ricing")))),
            "#ricing - Omairc"
        );
        let omarchy = controller.network_name("omarchy");
        assert_eq!(
            window_title(&controller, TitleView::Status(&omarchy)),
            "irc.example · fred Status"
        );
        assert_eq!(window_title(&controller, TitleView::Conversation(None)), "Omairc");
        let nowhere = controller.network_name("nope");
        assert_eq!(window_title(&controller, TitleView::Status(&nowhere)), "Omairc");
    }

    #[test]
    fn attention_title_matches_qt_and_go() {
        assert_eq!(
            attention_title("alice", "hey fred", "#ricing"),
            "alice: hey fred · #ricing - Omairc"
        );
        assert_eq!(
            attention_title("dax", "hello there", "dax"),
            "dax: hello there - Omairc"
        );
        assert_eq!(
            attention_title("alice", "hey fred", "#omarchy · irc.example · fred"),
            "alice: hey fred · #omarchy · irc.example · fred - Omairc"
        );
        assert_eq!(
            collapse_title_space("hey \x07fred\nthere"),
            "hey fred there"
        );
        assert_eq!(collapse_title_space("hey\u{9c}fred"), "hey fred");
    }

    #[test]
    fn title_mark_note_and_clear_follow_focus_and_selection() {
        let mut mark = None;
        note_title_mark(&mut mark, false, "alice", "hey", "omarchy", "#ricing");
        assert!(mark.is_some());
        note_title_mark(&mut mark, true, "bob", "nope", "omarchy", "#lab");
        assert_eq!(mark.as_ref().map(|m| m.author.as_str()), Some("alice"));
        mark = None;
        note_title_mark(&mut mark, false, "alice", "hey", "omarchy", "");
        assert!(mark.is_none());

        note_title_mark(&mut mark, false, "alice", "hey", "omarchy", "#ricing");
        clear_title_mark_if_opened(&mut mark, true, "omarchy", "#ricing");
        assert!(mark.is_some());
        clear_title_mark_if_opened(&mut mark, false, "omarchy", "#desktop");
        assert!(mark.is_some());
        clear_title_mark_if_opened(&mut mark, false, "omarchy", "#ricing");
        assert!(mark.is_none());
    }

    #[test]
    fn a_direct_message_on_a_second_network_takes_the_suffix() {
        let mut controller = Controller::seeded_for_tests();
        assert!(!needs_network_suffix(&controller, "rio"));
        controller.select_conversation("oftc", "#lab");
        assert!(controller.open_direct("anna"));
        assert!(needs_network_suffix(&controller, "anna"));
        assert_eq!(
            conversation_title(&controller, &ConversationRef::new("oftc", "anna")).as_deref(),
            Some("anna · irc.example · oak")
        );
    }
}
