package irc

import "strings"

// Request is one CAP REQ outcome: the lines to send, whether SASL is being
// requested, and the chosen SASL mechanism. It mirrors
// IrcCapabilityNegotiation::Request.
type Request struct {
	Lines         []string
	RequestsSasl  bool
	SaslMechanism string
}

type tokenState int

const (
	tokenAdvertised tokenState = iota
	tokenRequested
	tokenEnabled
	tokenRefused
)

// wantedCapability is one row of the fixed wanted table.
type wantedCapability struct {
	capability       Capability
	token            string
	dependency       Capability
	hasDependency    bool
	needsCredentials bool
	// ownRequestLine marks a token that must ride its own CAP REQ line, so a
	// NAK aimed at another token in the same request cannot refuse it.
	ownRequestLine bool
	acceptsValue   func(string) bool
}

// CapabilityNegotiation tracks advertised, requested, enabled, and refused
// tokens across one CAP LS/REQ/ACK/NAK/END exchange. It mirrors
// IrcCapabilityNegotiation.
type CapabilityNegotiation struct {
	tokens                   map[string]tokenState
	saslAdvertisedValue      string
	saslCredentialsAvailable bool
}

// NewCapabilityNegotiation returns a negotiation primed with whether SASL
// credentials are available.
func NewCapabilityNegotiation(saslCredentialsAvailable bool) *CapabilityNegotiation {
	negotiation := &CapabilityNegotiation{}
	negotiation.Reset(saslCredentialsAvailable)
	return negotiation
}

// Reset clears every token and restores the credentials availability.
func (n *CapabilityNegotiation) Reset(saslCredentialsAvailable bool) {
	n.tokens = make(map[string]tokenState)
	n.saslAdvertisedValue = ""
	n.saslCredentialsAvailable = saslCredentialsAvailable
}

// Advertise records the tokens the server offered in CAP LS. Unknown tokens
// and tokens whose advertised value we cannot use are ignored.
func (n *CapabilityNegotiation) Advertise(tokens []string) {
	for _, token := range tokens {
		wanted := wantedFor(tokenName(token))
		if wanted == nil || !wanted.acceptsValue(tokenValue(token)) {
			continue
		}
		name := foldedName(token)
		if state, ok := n.stateOf(name); ok &&
			(state == tokenEnabled || state == tokenRequested) {
			continue
		}
		if wanted.capability == CapabilitySasl {
			n.saslAdvertisedValue = tokenValue(token)
		}
		n.tokens[name] = tokenAdvertised
	}
}

// Withdraw forgets tokens the server removed in CAP DEL / NEW.
func (n *CapabilityNegotiation) Withdraw(tokens []string) {
	for _, token := range tokens {
		if wantedFor(tokenName(token)) == nil {
			continue
		}
		name := foldedName(token)
		if name == "sasl" {
			n.saslAdvertisedValue = ""
		}
		delete(n.tokens, name)
	}
}

// TakeRequest returns the CAP REQ lines for every requestable token and marks
// them requested. Repeated calls return nothing until something new is
// advertised.
func (n *CapabilityNegotiation) TakeRequest() Request {
	var request Request
	var presence []string

	for _, wanted := range wantedTable {
		if !n.isRequestable(wanted) {
			continue
		}
		if wanted.ownRequestLine {
			request.Lines = append(request.Lines, wanted.token)
		} else {
			presence = append(presence, wanted.token)
		}
		if wanted.capability == CapabilitySasl {
			request.RequestsSasl = true
			request.SaslMechanism = chosenSaslMechanism(n.saslAdvertisedValue)
		}
		n.tokens[foldedToken(wanted)] = tokenRequested
	}

	if len(presence) > 0 {
		request.Lines = append(request.Lines, strings.Join(presence, " "))
	}
	return request
}

// Acknowledge applies a CAP ACK, returning the capabilities newly enabled.
func (n *CapabilityNegotiation) Acknowledge(tokens []string) CapabilitySet {
	var newlyEnabled CapabilitySet
	for _, token := range tokens {
		wanted := wantedFor(tokenName(token))
		if wanted == nil {
			continue
		}
		name := foldedName(token)
		state, ok := n.stateOf(name)
		if strings.HasPrefix(token, "-") {
			if ok && state == tokenEnabled {
				n.tokens[name] = tokenRefused
			}
			continue
		}
		// Only a token we are still waiting on can be granted. An ACK that
		// arrives after the capability timeout gave up must not re-enable it.
		if !ok || state != tokenRequested {
			continue
		}
		wasEnabled := n.anyToken(wanted.capability, tokenEnabled)
		n.tokens[name] = tokenEnabled
		if !wasEnabled {
			newlyEnabled.Insert(wanted.capability)
		}
	}
	return newlyEnabled
}

// Reject applies a CAP NAK, returning the refused capabilities.
func (n *CapabilityNegotiation) Reject(tokens []string) CapabilitySet {
	var refused CapabilitySet
	for _, token := range tokens {
		wanted := wantedFor(tokenName(token))
		if wanted == nil {
			continue
		}
		name := foldedName(token)
		if state, ok := n.stateOf(name); !ok || state != tokenRequested {
			continue
		}
		n.tokens[name] = tokenRefused
		refused.Insert(wanted.capability)
	}
	return refused
}

// AbandonOutstanding refuses every still-requested token after the CAP
// timeout, returning the abandoned capabilities.
func (n *CapabilityNegotiation) AbandonOutstanding() CapabilitySet {
	var abandoned CapabilitySet
	for _, wanted := range wantedTable {
		name := foldedToken(wanted)
		if state, ok := n.stateOf(name); !ok || state != tokenRequested {
			continue
		}
		n.tokens[name] = tokenRefused
		abandoned.Insert(wanted.capability)
	}
	return abandoned
}

// Settled reports whether nothing is still awaiting an ACK or NAK.
func (n *CapabilityNegotiation) Settled() bool {
	for _, state := range n.tokens {
		if state == tokenRequested {
			return false
		}
	}
	return true
}

// Enabled returns the capabilities the server has acknowledged.
func (n *CapabilityNegotiation) Enabled() CapabilitySet {
	var set CapabilitySet
	for _, wanted := range wantedTable {
		if state, ok := n.stateOf(foldedToken(wanted)); ok && state == tokenEnabled {
			set.Insert(wanted.capability)
		}
	}
	return set
}

func (n *CapabilityNegotiation) stateOf(foldedToken string) (tokenState, bool) {
	state, ok := n.tokens[foldedToken]
	return state, ok
}

func (n *CapabilityNegotiation) anyToken(capability Capability, states ...tokenState) bool {
	for _, wanted := range wantedTable {
		if wanted.capability != capability {
			continue
		}
		state, ok := n.stateOf(foldedToken(wanted))
		if !ok {
			continue
		}
		for _, candidate := range states {
			if state == candidate {
				return true
			}
		}
	}
	return false
}

func (n *CapabilityNegotiation) isRequestable(wanted wantedCapability) bool {
	state, ok := n.stateOf(foldedToken(wanted))
	if !ok || state != tokenAdvertised {
		return false
	}
	if n.anyToken(wanted.capability, tokenEnabled, tokenRequested) {
		return false
	}
	if wanted.needsCredentials && !n.saslCredentialsAvailable {
		return false
	}
	if wanted.hasDependency {
		return n.anyToken(wanted.dependency, tokenAdvertised, tokenRequested, tokenEnabled)
	}
	return true
}

// wantedTable is the fixed capability policy, ported verbatim from the C++
// wantedTable. Order determines CAP REQ line order.
var wantedTable = []wantedCapability{
	{CapabilitySasl, "sasl", 0, false, true, true, acceptsSaslValue},
	{CapabilityAwayNotify, "away-notify", 0, false, false, false, acceptsAnyValue},
	{CapabilityBatch, "batch", 0, false, false, false, acceptsAnyValue},
	{CapabilityMemberMetadata, "draft/metadata-2", CapabilityBatch, true, false, false, acceptsAnyValue},
	{CapabilityMessageTags, "message-tags", 0, false, false, true, acceptsAnyValue},
	{CapabilityMultiPrefix, "multi-prefix", 0, false, false, false, acceptsAnyValue},
	{CapabilityChghost, "chghost", 0, false, false, false, acceptsAnyValue},
	{CapabilityCapNotify, "cap-notify", 0, false, false, false, acceptsAnyValue},
	{CapabilityEchoMessage, "echo-message", 0, false, false, false, acceptsAnyValue},
	{CapabilityServerTime, "server-time", 0, false, false, true, acceptsAnyValue},
	{CapabilityChatHistory, "chathistory", CapabilityBatch, true, false, false, acceptsAnyValue},
	{CapabilityChatHistory, "draft/chathistory", CapabilityBatch, true, false, false, acceptsAnyValue},
	{CapabilityZncPlayback, "znc.in/playback", CapabilityBatch, true, false, true, acceptsAnyValue},
	{CapabilityLabeledResponse, "labeled-response", CapabilityMessageTags, true, false, true, acceptsAnyValue},
	{CapabilityAccountTag, "account-tag", CapabilityMessageTags, true, false, false, acceptsAnyValue},
	{CapabilityAccountNotify, "account-notify", 0, false, false, false, acceptsAnyValue},
	{CapabilityExtendedJoin, "extended-join", 0, false, false, false, acceptsAnyValue},
}

func wantedFor(name string) *wantedCapability {
	for index := range wantedTable {
		if strings.EqualFold(name, wantedTable[index].token) {
			return &wantedTable[index]
		}
	}
	return nil
}

func foldedToken(wanted wantedCapability) string {
	return strings.ToLower(wanted.token)
}

func acceptsAnyValue(string) bool {
	return true
}

func saslValueLists(value, mechanism string) bool {
	for _, offered := range strings.Split(value, ",") {
		if offered == "" {
			continue
		}
		if strings.EqualFold(offered, mechanism) {
			return true
		}
	}
	return false
}

func acceptsSaslValue(value string) bool {
	return value == "" ||
		saslValueLists(value, "PLAIN") ||
		saslValueLists(value, "SCRAM-SHA-256")
}

func chosenSaslMechanism(value string) string {
	if saslValueLists(value, "SCRAM-SHA-256") {
		return "SCRAM-SHA-256"
	}
	return "PLAIN"
}

// tokenName returns the capability name of a CAP token, dropping any
// "=value" suffix and a leading '-'.
func tokenName(token string) string {
	name := token
	if separator := strings.IndexByte(name, '='); separator != -1 {
		name = name[:separator]
	}
	if strings.HasPrefix(name, "-") {
		name = name[1:]
	}
	return name
}

// tokenValue returns everything after the first '=', or "".
func tokenValue(token string) string {
	if separator := strings.IndexByte(token, '='); separator != -1 {
		return token[separator+1:]
	}
	return ""
}

func foldedName(token string) string {
	return strings.ToLower(tokenName(token))
}
