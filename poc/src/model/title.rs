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
