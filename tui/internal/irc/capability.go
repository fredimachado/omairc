package irc

// Capability is one IRCv3 capability bit. It mirrors IrcCapability; the order
// is significant because CapabilitySet packs them into a bitmask.
type Capability int

const (
	CapabilitySasl Capability = iota
	CapabilityAwayNotify
	CapabilityBatch
	CapabilityMemberMetadata
	CapabilityMessageTags
	CapabilityMultiPrefix
	CapabilityChghost
	CapabilityCapNotify
	CapabilityEchoMessage
	CapabilityChatHistory
	CapabilityServerTime
	CapabilityLabeledResponse
	CapabilityAccountTag
	CapabilityAccountNotify
	CapabilityExtendedJoin
	CapabilityZncPlayback
)

// CapabilitySet is a bitmask of Capability values. It mirrors IrcCapabilitySet.
type CapabilitySet uint32

func capabilityBit(capability Capability) CapabilitySet {
	return CapabilitySet(1) << uint(capability)
}

// Contains reports whether capability is set.
func (s CapabilitySet) Contains(capability Capability) bool {
	return s&capabilityBit(capability) != 0
}

// Insert sets capability.
func (s *CapabilitySet) Insert(capability Capability) {
	*s |= capabilityBit(capability)
}

// Remove clears capability.
func (s *CapabilitySet) Remove(capability Capability) {
	*s &^= capabilityBit(capability)
}

// IsEmpty reports whether no capability is set.
func (s CapabilitySet) IsEmpty() bool {
	return s == 0
}
