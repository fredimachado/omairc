//! Header and window titles. Ports Qt's duplicate-name rule: a conversation
//! name shows its network only when that name exists on two networks, so
//! `#omarchy` reads `#omarchy · irc.example · fred` while `#ricing` stays bare.

use super::ids::ConversationRef;
use super::mock::MockWorld;
use super::status::status_title;

/// What the centre pane shows, which picks the window title.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TitleView<'a> {
    /// A conversation, or `None` when nothing is selected.
    Conversation(Option<&'a ConversationRef>),
    /// The Status pane of this network.
    #[allow(dead_code)] // Phase 2 Status wiring: the shell passes this when Status is open.
    Status(&'a str),
}

/// True when some other network also has a conversation with this name.
pub fn needs_network_suffix(world: &MockWorld, name: &str) -> bool {
    world
        .networks
        .iter()
        .filter(|network| network.conversations.iter().any(|c| c.name == name))
        .count()
        > 1
}

/// The header title for `target`, or `None` when the conversation is gone.
pub fn conversation_title(world: &MockWorld, target: &ConversationRef) -> Option<String> {
    let conversation = world.find(target)?;
    if needs_network_suffix(world, &conversation.name) {
        let network = world.network(&target.network_id)?;
        Some(format!("{} · {}", conversation.name, network.display_name))
    } else {
        Some(conversation.name.clone())
    }
}

/// The window title for the view. A conversation reads `<title> - Omairc`, or
/// just `Omairc` with nothing selected. A Status title has no ` - Omairc`
/// suffix, as Qt's `statusTitleText` does.
pub fn window_title(world: &MockWorld, view: TitleView<'_>) -> String {
    match view {
        TitleView::Conversation(target) => {
            match target.and_then(|t| conversation_title(world, t)) {
                Some(title) => format!("{title} - Omairc"),
                None => "Omairc".to_string(),
            }
        }
        TitleView::Status(network_id) => {
            status_title(world, network_id).unwrap_or_else(|| "Omairc".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::mock::load_world;

    fn omarchy(key: &str) -> ConversationRef {
        ConversationRef::new("omarchy", key)
    }

    #[test]
    fn duplicate_name_shows_network_suffix() {
        let world = load_world();
        assert_eq!(
            conversation_title(&world, &omarchy("#omarchy")).as_deref(),
            Some("#omarchy · irc.example · fred")
        );
        assert_eq!(
            conversation_title(&world, &ConversationRef::new("oftc", "#omarchy")).as_deref(),
            Some("#omarchy · irc.example · oak")
        );
        assert_eq!(
            window_title(&world, TitleView::Conversation(Some(&omarchy("#omarchy")))),
            "#omarchy · irc.example · fred - Omairc"
        );
    }

    #[test]
    fn unique_name_is_bare() {
        let world = load_world();
        assert_eq!(
            conversation_title(&world, &omarchy("#ricing")).as_deref(),
            Some("#ricing")
        );
        assert_eq!(
            window_title(&world, TitleView::Conversation(Some(&omarchy("#ricing")))),
            "#ricing - Omairc"
        );
    }

    #[test]
    fn a_direct_message_takes_the_suffix_only_when_its_name_is_shared() {
        let mut world = load_world();
        assert!(!needs_network_suffix(&world, "anna"));
        crate::model::directs::open_direct(&mut world, "oftc", "anna");
        assert!(needs_network_suffix(&world, "anna"));
        assert_eq!(
            conversation_title(&world, &ConversationRef::new("oftc", "anna")).as_deref(),
            Some("anna · irc.example · oak")
        );
    }

    #[test]
    fn missing_selection_is_plain_omairc() {
        let world = load_world();
        assert_eq!(
            window_title(&world, TitleView::Conversation(None)),
            "Omairc"
        );
        assert_eq!(
            window_title(&world, TitleView::Conversation(Some(&omarchy("#gone")))),
            "Omairc"
        );
        assert_eq!(conversation_title(&world, &omarchy("#gone")), None);
    }

    #[test]
    fn status_title_has_no_app_suffix() {
        let world = load_world();
        assert_eq!(
            window_title(&world, TitleView::Status("omarchy")),
            "irc.example · fred Status"
        );
        assert_eq!(
            window_title(&world, TitleView::Status("oftc")),
            "irc.example · oak Status"
        );
        assert!(!window_title(&world, TitleView::Status("omarchy")).ends_with(" - Omairc"));
    }

    #[test]
    fn status_of_a_missing_network_is_plain_omairc() {
        let world = load_world();
        assert_eq!(window_title(&world, TitleView::Status("nope")), "Omairc");
    }

    #[test]
    fn status_view_ignores_the_selected_conversation() {
        let world = load_world();
        let selected = omarchy("#ricing");
        assert_eq!(
            window_title(&world, TitleView::Status("oftc")),
            "irc.example · oak Status"
        );
        assert_eq!(
            window_title(&world, TitleView::Conversation(Some(&selected))),
            "#ricing - Omairc"
        );
    }
}
