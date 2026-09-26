package irc

import (
	"strconv"
	"strings"
)

// PrefixSet is a bitmask of PREFIX letters, one bit per 'a'..'z'. It mirrors
// IrcPrefixSet: the zero value is empty.
type PrefixSet uint32

// IsEmpty reports whether no rank is set.
func (p PrefixSet) IsEmpty() bool {
	return p == 0
}

// PrefixChange is one mode-driven rank grant or revoke. It mirrors
// IrcPrefixChange.
type PrefixChange struct {
	Nick   string
	Letter byte
	Grant  bool
}

// ParsedName is one 353 NAMES token split into ranks and nick. It mirrors
// IrcParsedName.
type ParsedName struct {
	Nick  string
	Ranks PrefixSet
}

// ModeParamRule describes whether a channel-mode letter consumes a parameter.
// It mirrors IrcServerFeatures::ModeParamRule.
type ModeParamRule uint8

const (
	modeParamNever ModeParamRule = iota
	modeParamAlways
	modeParamSetOnly
	modeParamPrefix
)

type prefixPair struct {
	mode   byte
	symbol byte
}

// ServerFeatures holds the ISUPPORT state for one network. It mirrors
// IrcServerFeatures: PREFIX, CHANTYPES, and CHANMODES are parsed, never
// hardcoded by callers. Build one with NewServerFeatures.
type ServerFeatures struct {
	caseMapping       CaseMapping
	caseMappingKnown  bool
	channelTypes      string
	prefixPairs       []prefixPair
	prefixModes       string
	prefixSymbols     string
	nickLength        int
	nickLengthSet     bool
	monitorAdvertised bool
	monitorLimit      int
	monitorLimitSet   bool
	chanModesA        string
	chanModesB        string
	chanModesC        string
	chanModesD        string
	iconURL           string
	modeRules         [256]ModeParamRule
}

// NewServerFeatures returns features with the same defaults as the C++
// constructor: PREFIX (qaohv)~&@%+, CHANTYPES #&, CHANMODES b,k,l,imnpst.
func NewServerFeatures() ServerFeatures {
	features := ServerFeatures{
		channelTypes: "#&",
		prefixPairs: []prefixPair{
			{'q', '~'},
			{'a', '&'},
			{'o', '@'},
			{'h', '%'},
			{'v', '+'},
		},
		chanModesA: "b",
		chanModesB: "k",
		chanModesC: "l",
		chanModesD: "imnpst",
	}
	features.rebuildPrefixDumps()
	return features
}

// ApplyToken consumes one ISUPPORT token, with or without its leading '-'
// removal form.
func (f *ServerFeatures) ApplyToken(token string) {
	if token == "" {
		return
	}

	if token[0] == '-' {
		rest := token[1:]
		separator := strings.IndexByte(rest, '=')
		name := rest
		if separator != -1 {
			name = rest[:separator]
		}
		if name == "draft/ICON" {
			f.iconURL = ""
		}
		if name == "MONITOR" {
			f.monitorAdvertised = false
			f.monitorLimit = 0
			f.monitorLimitSet = false
		}
		return
	}

	separator := strings.IndexByte(token, '=')
	name := token
	value := ""
	if separator != -1 {
		name = token[:separator]
		value = token[separator+1:]
	}

	switch name {
	case "CASEMAPPING":
		if mapping, ok := FromName(value); ok {
			f.caseMapping = mapping
			f.caseMappingKnown = true
		}
	case "CHANTYPES":
		f.channelTypes = value
	case "PREFIX":
		f.applyPrefix(value)
	case "CHANMODES":
		parts, ok := splitChanModes(value)
		if !ok {
			return
		}
		f.chanModesA = parts[0]
		f.chanModesB = parts[1]
		f.chanModesC = parts[2]
		f.chanModesD = parts[3]
		f.rebuildModeRules()
	case "NICKLEN":
		if length, ok := parsePositiveDecimal(value); ok && length > 0 {
			f.nickLength = length
			f.nickLengthSet = true
		}
	case "MONITOR":
		if value == "" {
			f.monitorAdvertised = true
			f.monitorLimit = 0
			f.monitorLimitSet = false
			return
		}
		if limit, ok := parsePositiveDecimal(value); ok && limit > 0 {
			f.monitorAdvertised = true
			f.monitorLimit = limit
			f.monitorLimitSet = true
		}
	case "draft/ICON":
		if value != "" {
			f.iconURL = value
		}
	}
}

func (f *ServerFeatures) applyPrefix(value string) {
	if len(value) < 4 || value[0] != '(' {
		return
	}
	close := strings.IndexByte(value, ')')
	if close == -1 || close < 2 {
		return
	}
	modes := value[1:close]
	symbols := value[close+1:]
	if len(modes) != len(symbols) {
		return
	}
	f.prefixPairs = make([]prefixPair, 0, len(modes))
	for index := 0; index < len(modes); index++ {
		f.prefixPairs = append(f.prefixPairs, prefixPair{
			mode:   asciiLower(modes[index]),
			symbol: symbols[index],
		})
	}
	f.rebuildPrefixDumps()
}

// ApplyTokens consumes a batch of ISUPPORT tokens in order.
func (f *ServerFeatures) ApplyTokens(tokens []string) {
	for _, token := range tokens {
		f.ApplyToken(token)
	}
}

// CaseMapping returns the active case-mapping rule.
func (f *ServerFeatures) CaseMapping() CaseMapping {
	return f.caseMapping
}

// CaseMappingKnown reports whether CASEMAPPING arrived or registration
// marked it known.
func (f *ServerFeatures) CaseMappingKnown() bool {
	return f.caseMappingKnown
}

// MarkCaseMappingKnown records that registration implies the default mapping.
func (f *ServerFeatures) MarkCaseMappingKnown() {
	f.caseMappingKnown = true
}

// ChannelTypes returns the advertised CHANTYPES string.
func (f *ServerFeatures) ChannelTypes() string {
	return f.channelTypes
}

// IsChannel reports whether target starts with an advertised channel type.
func (f *ServerFeatures) IsChannel(target string) bool {
	return target != "" && strings.IndexByte(f.channelTypes, target[0]) != -1
}

// NickLength returns the advertised NICKLEN, and whether it was advertised.
func (f *ServerFeatures) NickLength() (int, bool) {
	return f.nickLength, f.nickLengthSet
}

// MonitorAdvertised reports whether MONITOR is available.
func (f *ServerFeatures) MonitorAdvertised() bool {
	return f.monitorAdvertised
}

// MonitorLimit returns the advertised MONITOR limit, and whether it was
// advertised. A bare MONITOR token advertises the capability without a limit.
func (f *ServerFeatures) MonitorLimit() (int, bool) {
	return f.monitorLimit, f.monitorLimitSet
}

// PrefixModes returns the advertised PREFIX mode letters, highest rank first.
func (f *ServerFeatures) PrefixModes() string {
	return f.prefixModes
}

// PrefixSymbols returns the advertised PREFIX symbols, highest rank first.
func (f *ServerFeatures) PrefixSymbols() string {
	return f.prefixSymbols
}

// ChanModesA returns the type-A (list) CHANMODES group.
func (f *ServerFeatures) ChanModesA() string {
	return f.chanModesA
}

// ChanModesB returns the type-B (always parameter) CHANMODES group.
func (f *ServerFeatures) ChanModesB() string {
	return f.chanModesB
}

// ChanModesC returns the type-C (set-only parameter) CHANMODES group.
func (f *ServerFeatures) ChanModesC() string {
	return f.chanModesC
}

// ChanModesD returns the type-D (no parameter) CHANMODES group.
func (f *ServerFeatures) ChanModesD() string {
	return f.chanModesD
}

// IconURL returns the advertised draft/ICON template, or "".
func (f *ServerFeatures) IconURL() string {
	return f.iconURL
}

// ParseNamesToken splits one 353 NAMES token into ranks and nick. ok is false
// when the token is only rank glyphs.
func (f *ServerFeatures) ParseNamesToken(token string) (ParsedName, bool) {
	var bits uint32
	index := 0
	for index < len(token) {
		letter := f.letterForSymbol(token[index])
		if letter == 0 {
			break
		}
		bits |= bitFor(letter)
		index++
	}
	nick := token[index:]
	if nick == "" {
		return ParsedName{}, false
	}
	return ParsedName{Nick: nick, Ranks: PrefixSet(bits)}, true
}

// PrefixChanges derives the rank grants and revokes from one MODE line,
// consuming non-prefix parameters as it walks the letters.
func (f *ServerFeatures) PrefixChanges(mode string, arguments []string) []PrefixChange {
	var changes []PrefixChange
	grant := true
	argument := 0
	for index := 0; index < len(mode); index++ {
		raw := mode[index]
		if raw == '+' {
			grant = true
			continue
		}
		if raw == '-' {
			grant = false
			continue
		}
		letter := asciiLower(raw)
		rule := f.ruleFor(raw)
		consumes := rule == modeParamAlways ||
			rule == modeParamPrefix ||
			(rule == modeParamSetOnly && grant)
		if rule == modeParamPrefix {
			if argument >= len(arguments) {
				continue
			}
			changes = append(changes, PrefixChange{
				Nick:   arguments[argument],
				Letter: letter,
				Grant:  grant,
			})
			argument++
			continue
		}
		if !consumes {
			continue
		}
		if argument >= len(arguments) {
			continue
		}
		argument++
	}
	return changes
}

// Apply returns ranks with change applied. It is idempotent.
func (f *ServerFeatures) Apply(ranks PrefixSet, change PrefixChange) PrefixSet {
	bit := bitFor(change.Letter)
	if bit == 0 {
		return ranks
	}
	if change.Grant {
		return PrefixSet(uint32(ranks) | bit)
	}
	return PrefixSet(uint32(ranks) &^ bit)
}

// RankPriority returns the index of the highest set rank, or len(PREFIX modes)
// when none is set.
func (f *ServerFeatures) RankPriority(ranks PrefixSet) int {
	for index, pair := range f.prefixPairs {
		if uint32(ranks)&bitFor(pair.mode) != 0 {
			return index
		}
	}
	return len(f.prefixPairs)
}

// MemberLabel prefixes nick with the highest-rank symbol, mirroring
// memberLabel in the C++ core.
func (f *ServerFeatures) MemberLabel(ranks PrefixSet, nick string) string {
	priority := f.RankPriority(ranks)
	if priority >= len(f.prefixPairs) {
		return nick
	}
	return string(f.prefixPairs[priority].symbol) + nick
}

func (f *ServerFeatures) rebuildPrefixDumps() {
	f.prefixModes = ""
	f.prefixSymbols = ""
	for _, pair := range f.prefixPairs {
		f.prefixModes += string(pair.mode)
		f.prefixSymbols += string(pair.symbol)
	}
	f.rebuildModeRules()
}

func (f *ServerFeatures) rebuildModeRules() {
	for index := range f.modeRules {
		f.modeRules[index] = modeParamNever
	}
	assign := func(letters string, rule ModeParamRule) {
		for index := 0; index < len(letters); index++ {
			f.modeRules[letters[index]] = rule
		}
	}
	assign(f.chanModesA, modeParamAlways)
	assign(f.chanModesB, modeParamAlways)
	assign(f.chanModesC, modeParamSetOnly)
	assign(f.chanModesD, modeParamNever)
	for _, pair := range f.prefixPairs {
		f.modeRules[pair.mode] = modeParamPrefix
	}
}

func (f *ServerFeatures) letterForSymbol(symbol byte) byte {
	for _, pair := range f.prefixPairs {
		if pair.symbol == symbol {
			return pair.mode
		}
	}
	return 0
}

func (f *ServerFeatures) ruleFor(raw byte) ModeParamRule {
	lowered := f.modeRules[asciiLower(raw)]
	if lowered == modeParamPrefix {
		return modeParamPrefix
	}
	return f.modeRules[raw]
}

func splitChanModes(value string) ([4]string, bool) {
	var parts [4]string
	count := 0
	start := 0
	for index := 0; index <= len(value); index++ {
		if index != len(value) && value[index] != ',' {
			continue
		}
		if count == 4 {
			return parts, false
		}
		parts[count] = value[start:index]
		count++
		start = index + 1
	}
	return parts, count == 4
}

func parsePositiveDecimal(value string) (int, bool) {
	// ParseUint with base 10 mirrors std::from_chars for an unsigned size_t:
	// it rejects an empty string, a sign, and overflow.
	parsed, err := strconv.ParseUint(value, 10, 64)
	if err != nil {
		return 0, false
	}
	return int(parsed), true
}

func bitFor(letter byte) uint32 {
	if letter < 'a' || letter > 'z' {
		return 0
	}
	return uint32(1) << (letter - 'a')
}
