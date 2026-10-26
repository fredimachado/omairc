use crate::irc::event::ConversationKey;

/// Names one conversation by its network id and its normalized target, so a stored
/// reference survives reordering networks or closing a DM. The normalized target is
/// the controller's key (`ConversationKey::normalized_target`), folded under the
/// network's case mapping, so it matches the controller's rows exactly.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ConversationRef {
    pub network_id: String,
    pub key: String,
}

impl ConversationRef {
    pub fn new(network_id: impl Into<String>, key: impl Into<String>) -> Self {
        Self {
            network_id: network_id.into(),
            key: key.into(),
        }
    }

    pub fn from_key(key: &ConversationKey) -> Self {
        Self::new(key.network_id.clone(), key.normalized_target.clone())
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refs_compare_by_network_and_key() {
        assert_eq!(
            ConversationRef::new("omarchy", "#omarchy"),
            ConversationRef::new("omarchy".to_string(), "#omarchy".to_string())
        );
        assert_ne!(
            ConversationRef::new("omarchy", "#omarchy"),
            ConversationRef::new("oftc", "#omarchy")
        );
    }

    #[test]
    fn a_ref_takes_the_normalized_target_of_its_controller_key() {
        let key = ConversationKey::new("oftc", "#omarchy");
        let reference = ConversationRef::from_key(&key);
        assert_eq!(reference.network_id, "oftc");
        assert_eq!(reference.key, "#omarchy");
    }
}
