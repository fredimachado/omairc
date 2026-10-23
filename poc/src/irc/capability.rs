//! Port of `tui/internal/irc/capability.go` (Go) and `src/irc/irccapability.{h,cpp}` (C++).
//!
//! One IRCv3 capability bit, and a set of them packed into a `u32`. The variant
//! order is significant: each variant's discriminant is its bit position, as in
//! the Go `iota` list and the C++ enum.

/// One IRCv3 capability the client knows about (`IrcCapability`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IrcCapability {
    Sasl,
    AwayNotify,
    Batch,
    MemberMetadata,
    MessageTags,
    MultiPrefix,
    Chghost,
    CapNotify,
    EchoMessage,
    ChatHistory,
    ServerTime,
    LabeledResponse,
    AccountTag,
    AccountNotify,
    ExtendedJoin,
    ReadMarker,
    ZncPlayback,
}

impl IrcCapability {
    /// The bit this capability occupies in an `IrcCapabilitySet`.
    const fn bit(self) -> u32 {
        1 << (self as u32)
    }
}

/// A bitmask over `IrcCapability` (`IrcCapabilitySet`). The default is empty.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IrcCapabilitySet(u32);

impl IrcCapabilitySet {
    pub fn contains(self, capability: IrcCapability) -> bool {
        self.0 & capability.bit() != 0
    }

    pub fn insert(&mut self, capability: IrcCapability) {
        self.0 |= capability.bit();
    }

    #[cfg(test)]
    pub fn remove(&mut self, capability: IrcCapability) {
        self.0 &= !capability.bit();
    }

    #[cfg(test)]
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The Go package has no set test of its own; these pin the bit layout the
    // negotiation tests rely on.

    #[test]
    fn a_new_set_is_empty() {
        let set = IrcCapabilitySet::default();
        assert!(set.is_empty());
        assert!(!set.contains(IrcCapability::Sasl));
        assert!(!set.contains(IrcCapability::ZncPlayback));
    }

    #[test]
    fn insert_and_remove_touch_only_their_bit() {
        let mut set = IrcCapabilitySet::default();
        set.insert(IrcCapability::Batch);
        set.insert(IrcCapability::ZncPlayback);
        assert!(set.contains(IrcCapability::Batch));
        assert!(set.contains(IrcCapability::ZncPlayback));
        assert!(!set.contains(IrcCapability::Sasl));
        assert!(!set.is_empty());

        set.remove(IrcCapability::Batch);
        assert!(!set.contains(IrcCapability::Batch));
        assert!(set.contains(IrcCapability::ZncPlayback));

        set.remove(IrcCapability::ZncPlayback);
        assert!(set.is_empty());
    }

    #[test]
    fn bits_follow_declaration_order() {
        assert_eq!(IrcCapability::Sasl.bit(), 1);
        assert_eq!(IrcCapability::AwayNotify.bit(), 2);
        assert_eq!(IrcCapability::ZncPlayback.bit(), 1 << 16);
    }

    #[test]
    fn removing_an_absent_capability_is_a_no_op() {
        let mut set = IrcCapabilitySet::default();
        set.insert(IrcCapability::Chghost);
        let before = set;
        set.remove(IrcCapability::Sasl);
        assert_eq!(set, before);
    }
}
