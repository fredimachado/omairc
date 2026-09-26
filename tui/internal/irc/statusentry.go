package irc

import (
	"strconv"
	"strings"
	"time"
)

// LogSource identifies who produced a Status entry. It mirrors IrcLogSource.
type LogSource int

const (
	// LogSourceServer is a line the server sent.
	LogSourceServer LogSource = iota
	// LogSourceClient is a line this client sent.
	LogSourceClient
	// LogSourceLocal is a locally generated lifecycle or outcome line.
	LogSourceLocal
)

// LogSeverity ranks a Status entry. It mirrors IrcLogSeverity.
type LogSeverity int

const (
	// LogSeverityTrace covers PING/PONG/AUTHENTICATE chatter.
	LogSeverityTrace LogSeverity = iota
	// LogSeverityInfo is the default.
	LogSeverityInfo
	// LogSeverityAlert covers errors and standard-reply failures.
	LogSeverityAlert
)

// WhoisProgress reports where a WHOIS sub-line sits in its reply sequence. It
// mirrors IrcWhoisLine::Progress.
type WhoisProgress int

const (
	// WhoisDetail is a mid-sequence detail line.
	WhoisDetail WhoisProgress = iota
	// WhoisTerminal is the 318 end-of-whois line.
	WhoisTerminal
	// WhoisFailed is a 401/402 no-such-nick/server line.
	WhoisFailed
)

// WhoisLine is the WHOIS detail carried by a server Status entry. It mirrors
// IrcWhoisLine.
type WhoisLine struct {
	nick     string
	text     string
	progress WhoisProgress
}

// Nick returns the nick the WHOIS line describes.
func (l WhoisLine) Nick() string { return l.nick }

// Text returns the formatted detail.
func (l WhoisLine) Text() string { return l.text }

// Progress returns the sequence position.
func (l WhoisLine) Progress() WhoisProgress { return l.progress }

// Terminal reports whether the line ends the WHOIS exchange.
func (l WhoisLine) Terminal() bool { return l.progress != WhoisDetail }

// CtcpReplyLine is the CTCP reply metadata carried by a server Status entry.
// It mirrors IrcCtcpReplyLine.
type CtcpReplyLine struct {
	nick    string
	command string
}

// Nick returns the reply's source nick.
func (l CtcpReplyLine) Nick() string { return l.nick }

// Command returns the uppercased CTCP command.
func (l CtcpReplyLine) Command() string { return l.command }

// StatusEntry is one classified Status line. It mirrors IrcStatusEntry: the
// session emits the raw line, and the entry carries the redacted text.
//
// Timestamps are clock-free. The C++ stamps QDateTime::currentDateTimeUtc();
// the Go core never reads the clock, so message-backed entries use the IRCv3
// server-time tag when present and the zero time otherwise. Local entries have
// no message, so their timestamp is the zero time.
type StatusEntry struct {
	networkID    string
	timestamp    time.Time
	source       LogSource
	severity     LogSeverity
	label        string
	requestLabel string
	text         string
	whoisLine    *WhoisLine
	ctcpReply    *CtcpReplyLine
}

func newStatusEntry(networkID string,
	timestamp time.Time,
	source LogSource,
	severity LogSeverity,
	label string,
	text string,
	whoisLine *WhoisLine,
	ctcpReply *CtcpReplyLine) StatusEntry {
	return StatusEntry{
		networkID: networkID,
		timestamp: timestamp,
		source:    source,
		severity:  severity,
		label:     label,
		text:      text,
		whoisLine: whoisLine,
		ctcpReply: ctcpReply,
	}
}

// NetworkID returns the owning network.
func (e StatusEntry) NetworkID() string { return e.networkID }

// Timestamp returns the entry timestamp.
func (e StatusEntry) Timestamp() time.Time { return e.timestamp }

// Source returns whether the server, the client, or the app produced the line.
func (e StatusEntry) Source() LogSource { return e.source }

// Severity returns the display severity.
func (e StatusEntry) Severity() LogSeverity { return e.severity }

// Label returns the short Status label.
func (e StatusEntry) Label() string { return e.label }

// RequestLabel returns the correlation label, or "".
func (e StatusEntry) RequestLabel() string { return e.requestLabel }

// SetRequestLabel records the correlation label on the entry.
func (e *StatusEntry) SetRequestLabel(requestLabel string) {
	e.requestLabel = requestLabel
}

// Text returns the redacted entry text.
func (e StatusEntry) Text() string { return e.text }

// WhoisLine returns the WHOIS detail, or nil.
func (e StatusEntry) WhoisLine() *WhoisLine { return e.whoisLine }

// CtcpReplyLine returns the CTCP reply metadata, or nil.
func (e StatusEntry) CtcpReplyLine() *CtcpReplyLine { return e.ctcpReply }

func statusCommandOf(message Message) string {
	return strings.ToUpper(WireText([]byte(message.Command)))
}

// statusTimestamp uses the IRCv3 server-time tag when present and parseable.
// It never reads the clock, so callers stay deterministic.
func statusTimestamp(message Message) time.Time {
	for _, tag := range message.Tags {
		if tag.Name != "time" || tag.Value == nil {
			continue
		}
		parsed, err := time.Parse(time.RFC3339Nano, *tag.Value)
		if err != nil {
			return time.Time{}
		}
		return parsed.UTC()
	}
	return time.Time{}
}

func statusMessageParameters(message Message) []string {
	parameters := make([]string, 0, len(message.Params))
	for _, parameter := range message.Params {
		parameters = append(parameters, WireText([]byte(parameter)))
	}
	return parameters
}

func statusParameterAt(parameters []string, index int) string {
	if index < 0 || index >= len(parameters) {
		return ""
	}
	return parameters[index]
}

// statusSplitFields splits on spaces and drops empty fields, mirroring
// QString::split(' ', Qt::SkipEmptyParts).
func statusSplitFields(text string) []string {
	var fields []string
	for _, field := range strings.Split(text, " ") {
		if field != "" {
			fields = append(fields, field)
		}
	}
	return fields
}

type statusStandardReply struct {
	command  string
	severity LogSeverity
}

var statusStandardReplies = []statusStandardReply{
	{"FAIL", LogSeverityAlert},
	{"WARN", LogSeverityInfo},
	{"NOTE", LogSeverityInfo},
}

func statusStandardReplyVerb(command string) *statusStandardReply {
	for index := range statusStandardReplies {
		if command == statusStandardReplies[index].command {
			return &statusStandardReplies[index]
		}
	}
	return nil
}

func statusSeverityFor(command string) LogSeverity {
	switch command {
	case "PING", "PONG", "AUTHENTICATE":
		return LogSeverityTrace
	}
	if reply := statusStandardReplyVerb(command); reply != nil {
		return reply.severity
	}
	if command == "ERROR" {
		return LogSeverityAlert
	}
	if len(command) == 3 && isASCIIDigit(command[0]) &&
		(command[0] == '4' || command[0] == '5') {
		return LogSeverityAlert
	}
	return LogSeverityInfo
}

func statusTrailingBodyOnly(command string) bool {
	if statusStandardReplyVerb(command) != nil {
		return true
	}
	switch command {
	case "PRIVMSG", "PING", "PONG", "ERROR":
		return true
	}
	if len(command) != 3 || !isASCIIDigit(command[0]) {
		return false
	}
	code, err := strconv.Atoi(command)
	if err != nil {
		return false
	}
	return (code >= 1 && code <= 3) || code == 372 || code == 375 ||
		code == 376 || code == 422
}

func statusJoinPipeSeparated(tokens []string) string {
	return strings.Join(tokens, " | ")
}

func statusCapTokens(message Message, subcommandIndex int) []string {
	if len(message.Params) <= subcommandIndex+1 {
		return nil
	}
	return statusSplitFields(WireText([]byte(message.Params[len(message.Params)-1])))
}

func statusFormatIncomingCap(message Message) (string, bool) {
	if len(message.Params) < 2 {
		return "", false
	}
	subcommand := WireText([]byte(message.Params[1]))
	formats := []struct {
		subcommand string
		prefix     string
	}{
		{"LS", "Server supports"},
		{"ACK", "Acknowledged"},
		{"NAK", "Rejected"},
		{"NEW", "Server added"},
		{"DEL", "Server removed"},
	}
	for _, row := range formats {
		if !strings.EqualFold(subcommand, row.subcommand) {
			continue
		}
		tokens := statusCapTokens(message, 1)
		if len(tokens) == 0 {
			return row.prefix + ":", true
		}
		return row.prefix + ": " + statusJoinPipeSeparated(tokens), true
	}
	return "", false
}

func statusFormatOutgoingCapReq(display string) (string, bool) {
	trimmed := strings.TrimSpace(display)
	if !hasPrefixFold(trimmed, "CAP") {
		return "", false
	}
	rest := strings.TrimSpace(trimmed[3:])
	if !hasPrefixFold(rest, "REQ") {
		return "", false
	}
	rest = strings.TrimSpace(rest[3:])
	if strings.HasPrefix(rest, ":") {
		rest = rest[1:]
	}
	tokens := statusSplitFields(rest)
	if len(tokens) == 0 {
		return "Requesting:", true
	}
	return "Requesting: " + statusJoinPipeSeparated(tokens), true
}

func hasPrefixFold(text, prefix string) bool {
	return len(text) >= len(prefix) && strings.EqualFold(text[:len(prefix)], prefix)
}

func statusFormatLusersNumeric(code int, parameters []string) (string, bool) {
	rest := parameters
	if len(rest) > 0 {
		rest = rest[1:]
	}
	if len(rest) == 0 {
		return "", false
	}
	switch code {
	case 252, 253, 254:
		if len(rest) >= 2 {
			return rest[0] + " " + rest[len(rest)-1], true
		}
		return rest[0], true
	case 20, 42, 221, 250, 251, 255, 265, 266, 396:
		return rest[len(rest)-1], true
	}
	return "", false
}

func statusIncomingText(message Message, command string, redacted bool) string {
	if len(message.Params) == 0 {
		return command
	}
	if redacted && len(message.Params) == 1 &&
		WireText([]byte(message.Params[0])) == "***" && !statusTrailingBodyOnly(command) {
		return command + " ***"
	}
	if statusTrailingBodyOnly(command) {
		return WireText([]byte(message.Params[len(message.Params)-1]))
	}
	parts := statusMessageParameters(message)
	if command != "" && isASCIIDigit(command[0]) && len(parts) >= 2 {
		parts = parts[1:]
	}
	return strings.Join(parts, " ")
}

type statusWhoisLayout int

const (
	statusWhoisUser statusWhoisLayout = iota
	statusWhoisChannels
	statusWhoisServer
	statusWhoisAway
	statusWhoisIdle
	statusWhoisEnd
	statusWhoisNickRest
	statusWhoisAccount
	statusWhoisNoSuchNick
	statusWhoisNoSuchServer
)

type statusWhoisSpec struct {
	code   int
	label  string
	layout statusWhoisLayout
}

var statusWhoisNumerics = []statusWhoisSpec{
	{311, "whois", statusWhoisUser},
	{319, "whois", statusWhoisChannels},
	{312, "whois", statusWhoisServer},
	{301, "whois", statusWhoisAway},
	{317, "whois", statusWhoisIdle},
	{318, "whois", statusWhoisEnd},
	{276, "whois", statusWhoisNickRest},
	{307, "whois", statusWhoisNickRest},
	{313, "whois", statusWhoisNickRest},
	{320, "whois", statusWhoisNickRest},
	{330, "whois", statusWhoisAccount},
	{338, "whois", statusWhoisNickRest},
	{378, "whois", statusWhoisNickRest},
	{379, "whois", statusWhoisNickRest},
	{671, "whois", statusWhoisNickRest},
	{401, "", statusWhoisNoSuchNick},
	{402, "", statusWhoisNoSuchServer},
}

func statusWhoisSpecFor(code int) *statusWhoisSpec {
	for index := range statusWhoisNumerics {
		if statusWhoisNumerics[index].code == code {
			return &statusWhoisNumerics[index]
		}
	}
	return nil
}

type statusFormattedWhois struct {
	label    string
	text     string
	nick     string
	progress WhoisProgress
}

func statusFormatWhois(message Message) (statusFormattedWhois, bool) {
	command := statusCommandOf(message)
	if len(command) != 3 {
		return statusFormattedWhois{}, false
	}
	code, err := strconv.Atoi(command)
	if err != nil {
		return statusFormattedWhois{}, false
	}
	spec := statusWhoisSpecFor(code)
	if spec == nil {
		return statusFormattedWhois{}, false
	}

	parameters := statusMessageParameters(message)
	text := ""
	switch spec.layout {
	case statusWhoisUser:
		if len(parameters) < 6 {
			return statusFormattedWhois{}, false
		}
		text = statusParameterAt(parameters, 1) + " is " +
			statusParameterAt(parameters, 2) + "@" + statusParameterAt(parameters, 3) +
			" (" + statusParameterAt(parameters, 5) + ")"
	case statusWhoisChannels:
		if len(parameters) < 3 {
			return statusFormattedWhois{}, false
		}
		text = statusParameterAt(parameters, 1) + " is on " + statusParameterAt(parameters, 2)
	case statusWhoisServer:
		if len(parameters) < 4 {
			return statusFormattedWhois{}, false
		}
		text = statusParameterAt(parameters, 1) + " using " +
			statusParameterAt(parameters, 2) + " (" + statusParameterAt(parameters, 3) + ")"
	case statusWhoisAway:
		if len(parameters) < 3 {
			return statusFormattedWhois{}, false
		}
		text = statusParameterAt(parameters, 1) + " is away: " + statusParameterAt(parameters, 2)
	case statusWhoisIdle:
		if len(parameters) < 3 {
			return statusFormattedWhois{}, false
		}
		text = statusParameterAt(parameters, 1) + " idle " + statusParameterAt(parameters, 2) + "s"
		if len(parameters) >= 5 && statusParameterAt(parameters, 3) != "" {
			text += ", signon " + statusParameterAt(parameters, 3)
		}
	case statusWhoisEnd:
		if len(parameters) < 2 {
			return statusFormattedWhois{}, false
		}
		text = "End of WHOIS for " + statusParameterAt(parameters, 1)
	case statusWhoisNickRest:
		if len(parameters) < 3 {
			return statusFormattedWhois{}, false
		}
		text = statusParameterAt(parameters, 1) + " " +
			strings.Join(parameters[2:], " ")
	case statusWhoisAccount:
		if len(parameters) < 3 {
			return statusFormattedWhois{}, false
		}
		text = statusParameterAt(parameters, 1) + " is logged in as " + statusParameterAt(parameters, 2)
	case statusWhoisNoSuchNick:
		if len(parameters) < 2 {
			return statusFormattedWhois{}, false
		}
		text = "No such nick: " + statusParameterAt(parameters, 1)
	case statusWhoisNoSuchServer:
		if len(parameters) < 2 {
			return statusFormattedWhois{}, false
		}
		text = "No such server: " + statusParameterAt(parameters, 1)
	}

	progress := WhoisDetail
	if spec.code == 318 {
		progress = WhoisTerminal
	} else if spec.code == 401 || spec.code == 402 {
		progress = WhoisFailed
	}

	label := spec.label
	if label == "" {
		label = command
	}
	return statusFormattedWhois{
		label:    label,
		text:     text,
		nick:     statusParameterAt(parameters, 1),
		progress: progress,
	}, true
}

type statusNoticeSpeaker struct {
	token string
}

func statusTryMakeNoticeSpeaker(token string) (statusNoticeSpeaker, bool) {
	trimmed := strings.TrimSpace(token)
	if trimmed == "" {
		return statusNoticeSpeaker{}, false
	}
	if strings.EqualFold(trimmed, "*") {
		return statusNoticeSpeaker{}, false
	}
	return statusNoticeSpeaker{token: trimmed}, true
}

type statusNoticeCopyKind int

const (
	statusNoticeWrapped statusNoticeCopyKind = iota
	statusNoticeBare
)

type statusNoticeStatusCopy struct {
	label      string
	kind       statusNoticeCopyKind
	hasSpeaker bool
	speaker    statusNoticeSpeaker
	body       string
}

func (c statusNoticeStatusCopy) text() string {
	if c.kind == statusNoticeWrapped && c.hasSpeaker {
		return "-" + c.speaker.token + "- " + c.body
	}
	return c.body
}

func statusWrappedNotice(speaker statusNoticeSpeaker, body string) statusNoticeStatusCopy {
	return statusNoticeStatusCopy{
		label:      "NOTICE",
		kind:       statusNoticeWrapped,
		hasSpeaker: true,
		speaker:    speaker,
		body:       body,
	}
}

func statusBareNotice(body string) statusNoticeStatusCopy {
	return statusNoticeStatusCopy{label: "NOTICE", kind: statusNoticeBare, body: body}
}

type statusNoticeWireParts struct {
	prefixNick    string
	hasPrefixNick bool
	prefixRaw     string
	hasPrefixRaw  bool
	target        string
	body          string
}

func statusResolveIncomingNoticeSpeaker(parts statusNoticeWireParts) (statusNoticeSpeaker, bool) {
	if parts.hasPrefixNick {
		if speaker, ok := statusTryMakeNoticeSpeaker(parts.prefixNick); ok {
			return speaker, true
		}
	}
	if parts.hasPrefixRaw {
		if speaker, ok := statusTryMakeNoticeSpeaker(parts.prefixRaw); ok {
			return speaker, true
		}
	}
	return statusTryMakeNoticeSpeaker(parts.target)
}

func statusParseIncomingNotice(message Message) (statusNoticeWireParts, bool) {
	if statusCommandOf(message) != "NOTICE" {
		return statusNoticeWireParts{}, false
	}
	if len(message.Params) == 0 {
		return statusNoticeWireParts{}, false
	}

	var parts statusNoticeWireParts
	if message.Prefix != nil {
		if message.Prefix.Nick != "" {
			parts.prefixNick = WireText([]byte(message.Prefix.Nick))
			parts.hasPrefixNick = true
		}
		if message.Prefix.Raw != "" {
			parts.prefixRaw = WireText([]byte(message.Prefix.Raw))
			parts.hasPrefixRaw = true
		}
	}
	parts.target = WireText([]byte(message.Params[0]))
	if len(message.Params) >= 2 {
		parts.body = WireText([]byte(message.Params[len(message.Params)-1]))
	}
	return parts, true
}

func statusPresentIncomingNotice(parts statusNoticeWireParts) statusNoticeStatusCopy {
	if speaker, ok := statusResolveIncomingNoticeSpeaker(parts); ok {
		return statusWrappedNotice(speaker, parts.body)
	}
	return statusBareNotice(parts.body)
}

func statusPresentIncomingInvite(message Message) (string, bool) {
	if statusCommandOf(message) != "INVITE" {
		return "", false
	}
	if len(message.Params) < 2 {
		return "", false
	}
	nick := PrefixNick(message)
	channel := WireText([]byte(message.Params[len(message.Params)-1]))
	if nick == "" || channel == "" {
		return "", false
	}
	return nick + " invited you to " + channel, true
}

func statusFirstToken(line string) string {
	trimmed := strings.TrimSpace(line)
	if trimmed == "" {
		return ""
	}
	end := len(trimmed)
	for index := 0; index < len(trimmed); index++ {
		if trimmed[index] == ' ' || trimmed[index] == '\t' {
			end = index
			break
		}
	}
	return strings.ToUpper(trimmed[:end])
}

type statusKeep int

const (
	statusKeepDrop statusKeep = iota
	statusKeepKeep
	statusKeepIncomingPrivmsg
)

type statusKeepRow struct {
	command  string
	incoming statusKeep
	outgoing statusKeep
}

// statusKeepRows mirrors kStatusKeep in src/irc/ircstatusentry.cpp.
var statusKeepRows = []statusKeepRow{
	{"PING", statusKeepDrop, statusKeepDrop},
	{"PONG", statusKeepDrop, statusKeepDrop},
	{"CAP", statusKeepKeep, statusKeepDrop},
	{"AUTHENTICATE", statusKeepDrop, statusKeepDrop},
	{"JOIN", statusKeepDrop, statusKeepDrop},
	{"PART", statusKeepDrop, statusKeepDrop},
	{"QUIT", statusKeepDrop, statusKeepDrop},
	{"NICK", statusKeepDrop, statusKeepDrop},
	{"KICK", statusKeepDrop, statusKeepDrop},
	{"TOPIC", statusKeepDrop, statusKeepDrop},
	{"MODE", statusKeepDrop, statusKeepDrop},
	{"BATCH", statusKeepDrop, statusKeepDrop},
	{"TAGMSG", statusKeepDrop, statusKeepDrop},
	{"AWAY", statusKeepDrop, statusKeepDrop},
	{"CHATHISTORY", statusKeepDrop, statusKeepDrop},
	{"PRIVMSG", statusKeepIncomingPrivmsg, statusKeepDrop},
	{"NOTICE", statusKeepKeep, statusKeepDrop},
	{"FAIL", statusKeepKeep, statusKeepDrop},
	{"WARN", statusKeepKeep, statusKeepDrop},
	{"NOTE", statusKeepKeep, statusKeepDrop},
	{"ERROR", statusKeepKeep, statusKeepDrop},
	{"INVITE", statusKeepKeep, statusKeepDrop},
	{"PASS", statusKeepDrop, statusKeepKeep},
}

func init() {
	if !statusKeepRowsAreWellFormed() {
		panic("irc: status keep rows must be uppercase and unique")
	}
}

func statusKeepRowsAreWellFormed() bool {
	for index, row := range statusKeepRows {
		if row.command == "" {
			return false
		}
		for position := 0; position < len(row.command); position++ {
			if row.command[position] >= 'a' && row.command[position] <= 'z' {
				return false
			}
		}
		for earlier := 0; earlier < index; earlier++ {
			if statusKeepRows[earlier].command == row.command {
				return false
			}
		}
	}
	return true
}

func statusKeepRowFor(command string) *statusKeepRow {
	for index := range statusKeepRows {
		if command == statusKeepRows[index].command {
			return &statusKeepRows[index]
		}
	}
	return nil
}

// statusFeaturesForChannelTypes builds the feature set the classifier uses
// when only CHANTYPES is on hand. The remaining fields keep the constructor
// defaults, exactly like the C++ featuresForChannelTypes.
func statusFeaturesForChannelTypes(channelTypes string) ServerFeatures {
	features := NewServerFeatures()
	if channelTypes == "" {
		return features
	}
	features.ApplyToken("CHANTYPES=" + channelTypes)
	return features
}

func statusIsNetworkNoticeTarget(target string) bool {
	return target == "" || target == "*" || strings.EqualFold(target, "AUTH")
}

func statusHasUserPrefix(message Message) bool {
	return message.Prefix != nil && message.Prefix.Nick != "" && message.Prefix.User != ""
}

func statusIsServiceUser(message Message, features ServerFeatures) bool {
	if message.Prefix == nil {
		return false
	}
	return IsServiceIdentity(
		WireText([]byte(message.Prefix.Nick)),
		WireText([]byte(message.Prefix.Host)),
		features.ChannelTypes())
}

// statusConversationEngaged mirrors the engaged outcome of ircConversationFor
// in src/irc/irceventtranslator.cpp for the subset ircStatusKeepsIncoming
// consults. The full predicate lands with the reducer's conversation policy in
// a later wave; keeping this private subset here keeps Status classification
// exact until then.
func statusConversationEngaged(message Message, target, currentNick string, features ServerFeatures) bool {
	if message.Prefix != nil && message.Prefix.Nick != "" &&
		!NickIsRoutable(WireText([]byte(message.Prefix.Nick))) {
		return false
	}
	if features.IsChannel(target) {
		return true
	}
	if statusIsNetworkNoticeTarget(target) || !statusHasUserPrefix(message) ||
		statusIsServiceUser(message, features) {
		return false
	}
	sender := PrefixNick(message)
	if sender == "" {
		return false
	}
	mapping := features.CaseMapping()
	if mapping.Equals(target, currentNick) {
		return true
	}
	if mapping.Equals(sender, currentNick) {
		return true
	}
	return false
}

func statusKeepsIncomingNumeric(command string) bool {
	if len(command) != 3 {
		return false
	}
	for index := 0; index < len(command); index++ {
		if !isASCIIDigit(command[index]) {
			return false
		}
	}
	code, err := strconv.Atoi(command)
	if err != nil {
		return false
	}
	if code >= 1 && code <= 4 {
		return true
	}
	switch code {
	case 20, 42, 221, 250, 251, 252, 253, 254, 255, 263, 265, 266,
		396, 372, 375, 376, 422, 764, 767, 769, 903, 904, 905:
		return true
	}
	if statusWhoisSpecFor(code) != nil {
		return true
	}
	return command[0] == '4' || command[0] == '5'
}

func statusKeepsIncomingPrivmsg(message Message, currentNick, channelTypes string) bool {
	if len(message.Params) >= 2 {
		if request, ok := ParseCtcpRequest(WireText([]byte(message.Params[len(message.Params)-1]))); ok {
			if request.Command != "ACTION" {
				return true
			}
		}
	}

	features := statusFeaturesForChannelTypes(channelTypes)
	types := features.ChannelTypes()
	target := ""
	if len(message.Params) > 0 {
		target = WireText([]byte(message.Params[0]))
	}
	// Channel chat belongs in the channel transcript. Ergo replays join/quit
	// history as HistServ PRIVMSG to the channel; the service-nick rule below
	// would otherwise flood Status with that replay.
	if features.IsChannel(target) {
		return false
	}
	sender := ""
	host := ""
	if message.Prefix != nil {
		if message.Prefix.Nick != "" {
			sender = WireText([]byte(message.Prefix.Nick))
		}
		if message.Prefix.Host != "" {
			host = WireText([]byte(message.Prefix.Host))
		}
	}
	if IsServiceIdentity(sender, host, types) || IsServiceIdentity(target, "", types) {
		return true
	}
	return !statusConversationEngaged(message, target, currentNick, features)
}

// StatusKeepsIncoming reports whether an incoming message belongs in Status.
// It mirrors ircStatusKeepsIncoming.
func StatusKeepsIncoming(message Message, currentNick, channelTypes string) bool {
	command := statusCommandOf(message)
	if row := statusKeepRowFor(command); row != nil {
		switch row.incoming {
		case statusKeepKeep:
			return true
		case statusKeepDrop:
			return false
		case statusKeepIncomingPrivmsg:
			return statusKeepsIncomingPrivmsg(message, currentNick, channelTypes)
		}
		return false
	}
	return statusKeepsIncomingNumeric(command)
}

// StatusKeepsOutgoing reports whether an outgoing line belongs in Status. It
// mirrors ircStatusKeepsOutgoing.
func StatusKeepsOutgoing(line []byte) bool {
	wire := line
	if len(wire) >= 2 && wire[len(wire)-2] == '\r' && wire[len(wire)-1] == '\n' {
		wire = wire[:len(wire)-2]
	} else if len(wire) >= 1 && wire[len(wire)-1] == '\n' {
		wire = wire[:len(wire)-1]
	}
	display := WireText(wire)
	verb := statusFirstToken(display)
	if verb == "" {
		return false
	}
	row := statusKeepRowFor(verb)
	if row == nil {
		return false
	}
	if row.outgoing == statusKeepKeep {
		return true
	}
	if verb == "CAP" {
		trimmed := strings.TrimSpace(display)
		if len(trimmed) > 3 {
			rest := strings.TrimSpace(trimmed[3:])
			if hasPrefixFold(rest, "REQ") {
				return true
			}
		}
	}
	return false
}

// IncomingAll classifies one incoming message into every Status entry it
// produces. Most messages yield one entry; a 004 yields one per advertised
// field. It mirrors IrcStatusEntry::incomingAll.
func IncomingAll(networkID string, message Message, channelTypes string) []StatusEntry {
	command := statusCommandOf(message)
	timestamp := statusTimestamp(message)

	if command == "004" {
		fields := []struct {
			label string
			index int
		}{
			{"Host", 1},
			{"IRCd", 2},
			{"User modes", 3},
			{"Channel modes", 4},
			{"Parametric channel modes", 5},
		}
		parameters := statusMessageParameters(message)
		var entries []StatusEntry
		for _, field := range fields {
			if field.index >= len(parameters) {
				break
			}
			if field.index == 5 && len(parameters) <= 5 {
				break
			}
			entries = append(entries, newStatusEntry(networkID, timestamp,
				LogSourceServer, statusSeverityFor(command), "004",
				field.label+": "+parameters[field.index], nil, nil))
		}
		if len(entries) > 0 {
			return entries
		}
	}

	if command == "CAP" {
		if formatted, ok := statusFormatIncomingCap(message); ok {
			return []StatusEntry{newStatusEntry(networkID, timestamp,
				LogSourceServer, statusSeverityFor(command), "CAP", formatted, nil, nil)}
		}
	}

	if len(command) == 3 && isASCIIDigit(command[0]) {
		if code, err := strconv.Atoi(command); err == nil {
			parameters := statusMessageParameters(message)
			if formatted, ok := statusFormatLusersNumeric(code, parameters); ok {
				return []StatusEntry{newStatusEntry(networkID, timestamp,
					LogSourceServer, statusSeverityFor(command), command, formatted, nil, nil)}
			}
		}
	}

	return []StatusEntry{statusBuildDefaultIncoming(networkID, message, channelTypes)}
}

// Incoming classifies one incoming message into its first Status entry. It
// mirrors IrcStatusEntry::incoming.
func Incoming(networkID string, message Message, channelTypes string) StatusEntry {
	return IncomingAll(networkID, message, channelTypes)[0]
}

func statusBuildDefaultIncoming(networkID string, message Message, channelTypes string) StatusEntry {
	command := statusCommandOf(message)
	timestamp := statusTimestamp(message)

	if formatted, ok := statusFormatWhois(message); ok {
		var line *WhoisLine
		if formatted.nick != "" && formatted.text != "" {
			line = &WhoisLine{nick: formatted.nick, text: formatted.text, progress: formatted.progress}
		}
		return newStatusEntry(networkID, timestamp, LogSourceServer,
			statusSeverityFor(command), formatted.label, formatted.text, line, nil)
	}

	// Redaction happens at classification time: the stored entry text is the
	// masked form, never the raw secret-bearing line. Mirrors the overlay in
	// IrcStatusEntry::buildDefaultIncoming.
	overlay := message
	overlaid := false
	if masked, ok := RedactMessage(message, channelTypes); ok {
		overlay = message
		overlay.Command = masked.Verb
		overlay.Params = masked.Parameters
		overlaid = true
	}
	display := overlay

	if statusCommandOf(display) == "PRIVMSG" && len(display.Params) >= 2 {
		if request, ok := ParseCtcpRequest(WireText([]byte(display.Params[len(display.Params)-1]))); ok {
			sender := "unknown"
			if display.Prefix != nil && display.Prefix.Nick != "" {
				sender = WireText([]byte(display.Prefix.Nick))
			}
			text := request.Command
			if request.Argument != "" {
				text = request.Command + " " + request.Argument
			}
			return newStatusEntry(networkID, timestamp, LogSourceServer,
				LogSeverityInfo, "CTCP", text+" from "+sender, nil, nil)
		}
	}

	if statusCommandOf(display) == "NOTICE" && len(display.Params) >= 2 {
		if request, ok := ParseCtcpRequest(WireText([]byte(display.Params[len(display.Params)-1]))); ok {
			if request.Command != "ACTION" {
				sender := "unknown"
				if display.Prefix != nil && display.Prefix.Nick != "" {
					sender = WireText([]byte(display.Prefix.Nick))
				}
				return newStatusEntry(networkID, timestamp, LogSourceServer,
					LogSeverityInfo, "CTCP",
					FormatCtcpReplyText(request.Command, sender, request.Argument, timestamp),
					nil, &CtcpReplyLine{nick: sender, command: request.Command})
			}
		}
	}

	if parts, ok := statusParseIncomingNotice(display); ok {
		copy := statusPresentIncomingNotice(parts)
		return newStatusEntry(networkID, timestamp, LogSourceServer,
			statusSeverityFor(command), copy.label, copy.text(), nil, nil)
	}

	if invite, ok := statusPresentIncomingInvite(display); ok {
		return newStatusEntry(networkID, timestamp, LogSourceServer,
			statusSeverityFor(command), "INVITE", invite, nil, nil)
	}

	return newStatusEntry(networkID, timestamp, LogSourceServer,
		statusSeverityFor(command), command,
		statusIncomingText(display, statusCommandOf(display), overlaid), nil, nil)
}

// Outgoing classifies one outgoing wire line. The stored text is redacted with
// RedactWireLine, so a PASS, keyed JOIN, keyed MODE, or service request never
// carries its secret into Status. It mirrors IrcStatusEntry::outgoing.
func Outgoing(networkID string, line []byte, channelTypes string) StatusEntry {
	wire := line
	if len(wire) >= 2 && wire[len(wire)-2] == '\r' && wire[len(wire)-1] == '\n' {
		wire = wire[:len(wire)-2]
	} else if len(wire) >= 1 && wire[len(wire)-1] == '\n' {
		wire = wire[:len(wire)-1]
	}

	display := WireText(wire)
	verb := statusFirstToken(display)
	text := display
	if capReq, ok := statusFormatOutgoingCapReq(display); ok {
		text = capReq
	} else if safe, ok := RedactWireLine(display, channelTypes); ok {
		text = safe
	}
	return newStatusEntry(networkID, time.Time{}, LogSourceClient,
		statusSeverityFor(verb), verb, text, nil, nil)
}

// Lifecycle builds a locally generated Status entry. It mirrors
// IrcStatusEntry::lifecycle.
func Lifecycle(networkID string, severity LogSeverity, label, text string) StatusEntry {
	return newStatusEntry(networkID, time.Time{}, LogSourceLocal, severity, label, text, nil, nil)
}

// Outcome builds a locally generated command-outcome Status entry. It mirrors
// IrcStatusEntry::outcome.
func Outcome(networkID, text string) StatusEntry {
	return newStatusEntry(networkID, time.Time{}, LogSourceLocal,
		LogSeverityInfo, "command", text, nil, nil)
}
