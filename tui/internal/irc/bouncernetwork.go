package irc

import "strings"

// BouncerNetworkState is the read-only state of one upstream network.
type BouncerNetworkState int

const (
	BouncerDisconnected BouncerNetworkState = iota
	BouncerConnecting
	BouncerConnected
)

// BouncerSubcommand is one case-insensitive BOUNCER subcommand.
type BouncerSubcommand int

const (
	BouncerBind BouncerSubcommand = iota
	BouncerListNetworks
	BouncerAddNetwork
	BouncerChangeNetwork
	BouncerDelNetwork
	BouncerSubcommandNetwork
)

// BouncerAttribute is one key from a message-tag attribute blob.
// Removed is set when the key is present with an empty or missing value.
type BouncerAttribute struct {
	Key     string
	Removed bool
	Value   string
}

// ParsedBouncer is one BOUNCER command. Network with Removed drops that netid.
type ParsedBouncer struct {
	Subcommand BouncerSubcommand
	NetID      string
	Removed    bool
	Attributes []BouncerAttribute
}

// BouncerNetwork is one upstream network. TLS defaults on and Port follows
// TLS (6697 or 6667) until the server names them. Pass is kept for the
// connection and must not be copied into Status.
type BouncerNetwork struct {
	ID           string
	Name         string
	Host         string
	Port         int
	TLS          bool
	Nickname     string
	Username     string
	Realname     string
	Pass         string
	State        BouncerNetworkState
	Error        string
	PortExplicit bool
	TLSExplicit  bool
	StateSet     bool
}

// BouncerAttributeValue is one attribute a client sends.
type BouncerAttributeValue struct {
	Key   string
	Value string
}

// BouncerFailureCode is one FAIL BOUNCER code from the extension.
type BouncerFailureCode int

const (
	BouncerFailUnknownCommand BouncerFailureCode = iota
	BouncerFailAccountRequired
	BouncerFailRegistrationIsCompleted
	BouncerFailInvalidNetID
	BouncerFailInvalidAttribute
	BouncerFailReadOnlyAttribute
	BouncerFailUnknownAttribute
	BouncerFailNeedAttribute
)

// BouncerFailure is one parsed FAIL BOUNCER line.
type BouncerFailure struct {
	Code        BouncerFailureCode
	CodeName    string
	Subcommand  string
	NetID       string
	Attribute   string
	Description string
}

// BouncerCommandResult is a matched ADDNETWORK, CHANGENETWORK, or DELNETWORK
// reply, or the FAIL that answered one.
type BouncerCommandResult struct {
	Subcommand  string
	OK          bool
	NetID       string
	Code        string
	Attribute   string
	Description string
}

// BouncerNetworkTable is the netid table for one connection.
type BouncerNetworkTable struct {
	networks []BouncerNetwork
}

// Clear drops every network.
func (t *BouncerNetworkTable) Clear() {
	t.networks = nil
}

// Apply merges one BOUNCER NETWORK line, or removes the netid when Removed.
func (t *BouncerNetworkTable) Apply(command ParsedBouncer) {
	if command.Subcommand != BouncerSubcommandNetwork || command.NetID == "" {
		return
	}
	if command.Removed {
		t.networks = removeNetwork(t.networks, command.NetID)
		return
	}
	index := networkIndex(t.networks, command.NetID)
	if index < 0 {
		t.networks = append(t.networks, freshBouncerNetwork(command.NetID))
		index = len(t.networks) - 1
	}
	applyBouncerAttributes(&t.networks[index], command.Attributes)
}

// ReplaceSnapshot installs the networks from a parameter-less
// soju.im/bouncer-networks batch, dropping anything the batch omitted.
func (t *BouncerNetworkTable) ReplaceSnapshot(lines []ParsedBouncer) {
	var next []BouncerNetwork
	for _, line := range lines {
		if line.Subcommand != BouncerSubcommandNetwork || line.NetID == "" {
			continue
		}
		if line.Removed {
			next = removeNetwork(next, line.NetID)
			continue
		}
		index := networkIndex(next, line.NetID)
		if index < 0 {
			next = append(next, freshBouncerNetwork(line.NetID))
			index = len(next) - 1
		}
		applyBouncerAttributes(&next[index], line.Attributes)
	}
	t.networks = next
}

// List returns the networks in the order they were first seen.
func (t *BouncerNetworkTable) List() []BouncerNetwork {
	out := make([]BouncerNetwork, len(t.networks))
	copy(out, t.networks)
	return out
}

// BouncerListCommand is the LISTNETWORKS request.
func BouncerListCommand() string {
	return "BOUNCER LISTNETWORKS"
}

// BouncerAddCommand encodes ADDNETWORK. ok is false when the attributes
// cannot be sent.
func BouncerAddCommand(attributes []BouncerAttributeValue) (string, bool) {
	encoded, ok := encodeBouncerAttributes(attributes)
	if !ok {
		return "", false
	}
	return "BOUNCER ADDNETWORK " + encoded, true
}

// BouncerChangeCommand encodes CHANGENETWORK.
func BouncerChangeCommand(netID string, attributes []BouncerAttributeValue) (string, bool) {
	if !validBouncerNetID(netID) {
		return "", false
	}
	encoded, ok := encodeBouncerAttributes(attributes)
	if !ok {
		return "", false
	}
	return "BOUNCER CHANGENETWORK " + netID + " " + encoded, true
}

// BouncerDeleteCommand encodes DELNETWORK.
func BouncerDeleteCommand(netID string) (string, bool) {
	if !validBouncerNetID(netID) {
		return "", false
	}
	return "BOUNCER DELNETWORK " + netID, true
}

// BouncerBindCommand encodes BIND.
func BouncerBindCommand(netID string) (string, bool) {
	if !validBouncerNetID(netID) {
		return "", false
	}
	return "BOUNCER BIND " + netID, true
}

// ParseBouncerFailure reads FAIL BOUNCER. ok is false for any other FAIL.
func ParseBouncerFailure(message Message) (BouncerFailure, bool) {
	if !strings.EqualFold(message.Command, "FAIL") || len(message.Params) < 4 {
		return BouncerFailure{}, false
	}
	if !strings.EqualFold(message.Params[0], "BOUNCER") {
		return BouncerFailure{}, false
	}
	codeName := message.Params[1]
	code, ok := bouncerFailureCode(codeName)
	if !ok {
		return BouncerFailure{}, false
	}
	failure := BouncerFailure{
		Code:        code,
		CodeName:    codeName,
		Subcommand:  message.Params[2],
		Description: message.Params[len(message.Params)-1],
	}
	context := message.Params[3 : len(message.Params)-1]
	switch code {
	case BouncerFailInvalidNetID:
		if len(context) > 0 {
			failure.NetID = context[0]
		}
	case BouncerFailInvalidAttribute, BouncerFailReadOnlyAttribute,
		BouncerFailUnknownAttribute, BouncerFailNeedAttribute:
		if len(context) >= 2 {
			failure.NetID = context[0]
			failure.Attribute = context[1]
		} else if len(context) == 1 {
			failure.Attribute = context[0]
		}
	}
	return failure, true
}

func freshBouncerNetwork(id string) BouncerNetwork {
	return BouncerNetwork{
		ID:    id,
		TLS:   true,
		Port:  defaultBouncerPort(true),
		State: BouncerDisconnected,
	}
}

func defaultBouncerPort(tls bool) int {
	if tls {
		return 6697
	}
	return 6667
}

func recomputeBouncerPort(network *BouncerNetwork) {
	if !network.PortExplicit {
		network.Port = defaultBouncerPort(network.TLS)
	}
}

func networkIndex(networks []BouncerNetwork, id string) int {
	for index := range networks {
		if networks[index].ID == id {
			return index
		}
	}
	return -1
}

func removeNetwork(networks []BouncerNetwork, id string) []BouncerNetwork {
	index := networkIndex(networks, id)
	if index < 0 {
		return networks
	}
	return append(networks[:index], networks[index+1:]...)
}

func knownBouncerKey(key string) bool {
	switch strings.ToLower(key) {
	case "name", "host", "port", "tls", "nickname", "username", "realname", "pass", "state", "error":
		return true
	default:
		return false
	}
}

func applyBouncerAttributes(network *BouncerNetwork, attributes []BouncerAttribute) {
	for _, attribute := range attributes {
		if !knownBouncerKey(attribute.Key) {
			continue
		}
		switch strings.ToLower(attribute.Key) {
		case "name":
			network.Name = cleared(attribute)
		case "host":
			network.Host = cleared(attribute)
		case "nickname":
			network.Nickname = cleared(attribute)
		case "username":
			network.Username = cleared(attribute)
		case "realname":
			network.Realname = cleared(attribute)
		case "pass":
			network.Pass = cleared(attribute)
		case "error":
			network.Error = cleared(attribute)
		case "state":
			applyBouncerState(network, attribute)
		case "tls":
			applyBouncerTLS(network, attribute)
		case "port":
			applyBouncerPort(network, attribute)
		}
	}
}

func cleared(attribute BouncerAttribute) string {
	if attribute.Removed {
		return ""
	}
	return attribute.Value
}

func applyBouncerState(network *BouncerNetwork, attribute BouncerAttribute) {
	if attribute.Removed {
		network.State = BouncerDisconnected
		network.StateSet = false
		return
	}
	switch strings.ToLower(attribute.Value) {
	case "connected":
		network.State = BouncerConnected
		network.StateSet = true
	case "connecting":
		network.State = BouncerConnecting
		network.StateSet = true
	case "disconnected":
		network.State = BouncerDisconnected
		network.StateSet = true
	}
}

func applyBouncerTLS(network *BouncerNetwork, attribute BouncerAttribute) {
	if attribute.Removed {
		network.TLS = true
		network.TLSExplicit = false
		recomputeBouncerPort(network)
		return
	}
	if attribute.Value == "1" || attribute.Value == "0" {
		network.TLS = attribute.Value == "1"
		network.TLSExplicit = true
		recomputeBouncerPort(network)
	}
}

func applyBouncerPort(network *BouncerNetwork, attribute BouncerAttribute) {
	if attribute.Removed {
		network.PortExplicit = false
		recomputeBouncerPort(network)
		return
	}
	port := 0
	for _, character := range attribute.Value {
		if character < '0' || character > '9' {
			return
		}
		port = port*10 + int(character-'0')
		if port > 65535 {
			return
		}
	}
	if port > 0 && port <= 65535 {
		network.Port = port
		network.PortExplicit = true
	}
}

func validBouncerNetID(netID string) bool {
	if netID == "" || strings.ContainsAny(netID, " \r\n\x00:") {
		return false
	}
	return true
}

func validBouncerAttributeKey(key string) bool {
	if key == "" {
		return false
	}
	for index := 0; index < len(key); index++ {
		character := key[index]
		ok := (character >= 'A' && character <= 'Z') ||
			(character >= 'a' && character <= 'z') ||
			(character >= '0' && character <= '9') ||
			character == '-'
		if !ok {
			return false
		}
	}
	return true
}

func escapeTagValue(value string) string {
	var builder strings.Builder
	builder.Grow(len(value))
	for index := 0; index < len(value); index++ {
		switch value[index] {
		case '\\':
			builder.WriteString(`\\`)
		case ';':
			builder.WriteString(`\:`)
		case ' ':
			builder.WriteString(`\s`)
		case '\r':
			builder.WriteString(`\r`)
		case '\n':
			builder.WriteString(`\n`)
		default:
			builder.WriteByte(value[index])
		}
	}
	return builder.String()
}

func encodeBouncerAttributes(attributes []BouncerAttributeValue) (string, bool) {
	if len(attributes) == 0 {
		return "", false
	}
	pieces := make([]string, 0, len(attributes))
	for _, attribute := range attributes {
		if !validBouncerAttributeKey(attribute.Key) || strings.ContainsAny(attribute.Value, "\r\n\x00") {
			return "", false
		}
		if attribute.Value == "" {
			pieces = append(pieces, attribute.Key+"=")
			continue
		}
		pieces = append(pieces, attribute.Key+"="+escapeTagValue(attribute.Value))
	}
	return strings.Join(pieces, ";"), true
}

func bouncerFailureCode(name string) (BouncerFailureCode, bool) {
	switch strings.ToUpper(name) {
	case "UNKNOWN_COMMAND":
		return BouncerFailUnknownCommand, true
	case "ACCOUNT_REQUIRED":
		return BouncerFailAccountRequired, true
	case "REGISTRATION_IS_COMPLETED":
		return BouncerFailRegistrationIsCompleted, true
	case "INVALID_NETID":
		return BouncerFailInvalidNetID, true
	case "INVALID_ATTRIBUTE":
		return BouncerFailInvalidAttribute, true
	case "READ_ONLY_ATTRIBUTE":
		return BouncerFailReadOnlyAttribute, true
	case "UNKNOWN_ATTRIBUTE":
		return BouncerFailUnknownAttribute, true
	case "NEED_ATTRIBUTE":
		return BouncerFailNeedAttribute, true
	default:
		return 0, false
	}
}
