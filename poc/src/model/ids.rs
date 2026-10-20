/// Names one conversation by its network id and its stable key, so a stored
/// reference survives reordering networks or closing a DM. Positions are
/// resolved through `MockWorld::find` when needed.
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
}
