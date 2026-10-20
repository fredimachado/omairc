//! Header and window titles. Ports Qt's duplicate-name rule: a conversation
//! name shows its network only when that name exists on two networks, so
//! `#omarchy` reads `#omarchy · irc.example · fred` while `#ricing` stays bare.

use super::ids::ConversationRef;
use super::mock::MockWorld;

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

/// The window title, `<title> - Omairc`, or just `Omairc` with nothing selected.
pub fn window_title(world: &MockWorld, target: Option<&ConversationRef>) -> String {
    match target.and_then(|t| conversation_title(world, t)) {
        Some(title) => format!("{title} - Omairc"),
        None => "Omairc".to_string(),
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
            window_title(&world, Some(&omarchy("#omarchy"))),
            "#omarchy · irc.example · fred - Omairc"
        );
    }

    #[test]
    fn unique_name_is_bare() {
        let world = load_world();
        assert_eq!(conversation_title(&world, &omarchy("#ricing")).as_deref(), Some("#ricing"));
        assert_eq!(window_title(&world, Some(&omarchy("#ricing"))), "#ricing - Omairc");
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
        assert_eq!(window_title(&world, None), "Omairc");
        assert_eq!(window_title(&world, Some(&omarchy("#gone"))), "Omairc");
        assert_eq!(conversation_title(&world, &omarchy("#gone")), None);
    }
}
