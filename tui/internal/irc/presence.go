package irc

import (
	"strconv"
	"strings"
	"unicode/utf16"
)

// MaximumValueBytes is the client ceiling on a metadata value, in UTF-8 bytes.
// It mirrors IrcMetadata::maximumValueBytes.
const MaximumValueBytes = 512

// MetadataCapability is the draft/metadata-2 limits a server advertised. A nil
// field means the server did not advertise that limit. It mirrors
// IrcMetadataCapability.
type MetadataCapability struct {
	MaxSubs       *int
	MaxValueBytes *int
}

// StatusKey returns the IRCv3 `status` metadata key.
func StatusKey() string { return "status" }

// AvatarKey returns the IRCv3 `avatar` metadata key.
func AvatarKey() string { return "avatar" }

// BotKey returns the IRCv3 `bot` metadata key.
func BotKey() string { return "bot" }

// DisplayNameKey returns the IRCv3 `display-name` metadata key.
func DisplayNameKey() string { return "display-name" }

// PronounsKey returns the IRCv3 `pronouns` metadata key.
func PronounsKey() string { return "pronouns" }

// HomepageKey returns the IRCv3 `homepage` metadata key.
func HomepageKey() string { return "homepage" }

// ColorKey returns the IRCv3 `color` metadata key.
func ColorKey() string { return "color" }

// subscribedKeys is the fixed subscription list, ported verbatim from
// IrcMetadata::subscribedKeys. Status comes first so /status works under a
// max-subs limit.
var subscribedKeys = []string{
	StatusKey(),
	AvatarKey(),
	BotKey(),
	DisplayNameKey(),
	PronounsKey(),
	HomepageKey(),
	ColorKey(),
}

// SubscribedKeys returns every metadata key Omairc subscribes to, in
// subscription priority order. The result is a copy.
func SubscribedKeys() []string {
	keys := make([]string, len(subscribedKeys))
	copy(keys, subscribedKeys)
	return keys
}

// SubscriptionKeys returns the subscription list truncated to maxSubs. A nil
// maxSubs returns the full list; a non-positive maxSubs returns nothing.
func SubscriptionKeys(maxSubs *int) []string {
	all := SubscribedKeys()
	if maxSubs == nil {
		return all
	}
	if *maxSubs <= 0 {
		return []string{}
	}
	if *maxSubs >= len(all) {
		return all
	}
	return all[:*maxSubs]
}

// CanonicalKey resolves a metadata key to its stored spelling, or "" when the
// key is not one Omairc subscribes to. Matching is case-insensitive.
func CanonicalKey(key string) string {
	for _, known := range subscribedKeys {
		if strings.EqualFold(key, known) {
			return known
		}
	}
	return ""
}

// IsKnownKey reports whether key is one Omairc subscribes to.
func IsKnownKey(key string) bool {
	return CanonicalKey(key) != ""
}

// Clamped truncates value to MaximumValueBytes UTF-8 bytes.
func Clamped(value string) string {
	return ClampedTo(value, MaximumValueBytes)
}

// ClampedTo truncates value to maxBytes UTF-8 bytes. A non-positive maxBytes
// yields the empty string. Truncation mirrors QString::chop: one UTF-16 code
// unit is dropped at a time, so a split surrogate becomes U+FFFD.
func ClampedTo(value string, maxBytes int) string {
	if maxBytes <= 0 {
		return ""
	}
	units := utf16.Encode([]rune(value))
	for len(utf16Decode(units)) > maxBytes {
		units = units[:len(units)-1]
	}
	return utf16Decode(units)
}

// EffectiveMaxValueBytes resolves the value ceiling: the advertised limit when
// present, else the client default, never above MaximumValueBytes.
func EffectiveMaxValueBytes(advertised *int) int {
	// Absent/malformed → client default. Explicit 0 means no non-empty values.
	if advertised == nil {
		return MaximumValueBytes
	}
	if *advertised < MaximumValueBytes {
		return *advertised
	}
	return MaximumValueBytes
}

// ParseMetadataCapability extracts the draft/metadata-2 limits from CAP LS
// tokens. ok is false when no draft/metadata-2 token was present; a later
// matching token replaces an earlier one.
func ParseMetadataCapability(tokens []string) (MetadataCapability, bool) {
	var result MetadataCapability
	found := false
	for _, token := range tokens {
		if !strings.EqualFold(metadataTokenName(token), "draft/metadata-2") {
			continue
		}
		result = MetadataCapability{}
		found = true
		raw := metadataTokenValue(token)
		if raw == "" {
			continue
		}
		for _, part := range strings.Split(raw, ",") {
			if part == "" {
				continue
			}
			key := strings.ToLower(metadataTokenName(part))
			value, ok := parseNonNegativeInt(metadataTokenValue(part))
			if !ok {
				continue
			}
			if key == "max-subs" {
				if result.MaxSubs == nil {
					limit := value
					result.MaxSubs = &limit
				}
				continue
			}
			if key == "max-value-bytes" {
				if result.MaxValueBytes == nil {
					limit := value
					result.MaxValueBytes = &limit
				}
			}
		}
	}
	return result, found
}

// metadataTokenName returns the capability name of a CAP token, dropping any
// "=value" suffix. It deliberately does not drop a leading '-', matching the
// anonymous tokenName in ircpresence.cpp.
func metadataTokenName(token string) string {
	if separator := strings.IndexByte(token, '='); separator != -1 {
		return token[:separator]
	}
	return token
}

// metadataTokenValue returns everything after the first '=', or "".
func metadataTokenValue(token string) string {
	if separator := strings.IndexByte(token, '='); separator != -1 {
		return token[separator+1:]
	}
	return ""
}

// parseNonNegativeInt mirrors the anonymous parseNonNegativeInt in
// ircpresence.cpp: an empty, non-numeric, overflowing, or negative value fails.
func parseNonNegativeInt(raw string) (int, bool) {
	if raw == "" {
		return 0, false
	}
	value, err := strconv.Atoi(strings.TrimSpace(raw))
	if err != nil || value < 0 {
		return 0, false
	}
	return value, true
}

// utf16Decode converts UTF-16 code units to a string, replacing unpaired
// surrogates with U+FFFD, exactly like QString::toUtf8.
func utf16Decode(units []uint16) string {
	return string(utf16.Decode(units))
}

// Away is an away notice with its optional reason. It mirrors IrcAway.
type Away struct {
	Reason string
}

// NickPresence is the per-nick presence Omairc tracks on one network: away
// state, metadata, and the services account. It mirrors IrcNickPresence.
type NickPresence struct {
	Away *Away
	// Keys holds canonical metadata keys mapped to their values.
	Keys map[string]string
	// Account is the services account from account-tag, account-notify,
	// extended-join, or WHOIS 330. Empty means unknown or logged out; both
	// display as nothing.
	Account string
}

// Metadata returns the stored value for key, or "" when unset. The key is
// resolved through CanonicalKey first.
func (p NickPresence) Metadata(key string) string {
	stored := CanonicalKey(key)
	if stored == "" {
		return ""
	}
	return p.Keys[stored]
}

// HasKey reports whether a metadata key is present, regardless of its value.
func (p NickPresence) HasKey(key string) bool {
	stored := CanonicalKey(key)
	if stored == "" {
		return false
	}
	_, ok := p.Keys[stored]
	return ok
}

// IsBot reports whether the bot key marks the client. The registry treats
// setting `bot` as the flag; the value names the software, so "0", "false",
// and "no" (case-insensitively, trimmed) are not bots.
func (p NickPresence) IsBot() bool {
	software := p.Metadata(BotKey())
	if software == "" {
		return false
	}
	trimmed := strings.TrimSpace(software)
	return !strings.EqualFold(trimmed, "0") &&
		!strings.EqualFold(trimmed, "false") &&
		!strings.EqualFold(trimmed, "no")
}

// Status returns the status metadata value, or "".
func (p NickPresence) Status() string {
	return p.Metadata(StatusKey())
}

// Avatar returns the avatar metadata value, or "".
func (p NickPresence) Avatar() string {
	return p.Metadata(AvatarKey())
}

// IsDefault reports whether the presence carries no fact worth keeping.
func (p NickPresence) IsDefault() bool {
	return p.Away == nil && len(p.Keys) == 0 && p.Account == ""
}

func (p NickPresence) clone() NickPresence {
	var away *Away
	if p.Away != nil {
		copied := *p.Away
		away = &copied
	}
	var keys map[string]string
	if p.Keys != nil {
		keys = make(map[string]string, len(p.Keys))
		for key, value := range p.Keys {
			keys[key] = value
		}
	}
	return NickPresence{Away: away, Keys: keys, Account: p.Account}
}

// NetworkPresence is the per-network presence store. It mirrors
// IrcNetworkPresence. The zero value is usable and empty.
type NetworkPresence struct {
	nicks map[string]NickPresence
}

func (n *NetworkPresence) ensure() {
	if n.nicks == nil {
		n.nicks = make(map[string]NickPresence)
	}
}

// SetAway stores away state for normalizedNick. A nil away clears it.
func (n *NetworkPresence) SetAway(normalizedNick string, away *Away) {
	if normalizedNick == "" {
		return
	}
	n.ensure()
	presence := n.nicks[normalizedNick]
	if away == nil {
		presence.Away = nil
	} else {
		copied := *away
		presence.Away = &copied
	}
	n.nicks[normalizedNick] = presence
	n.eraseIfDefault(normalizedNick)
}

// SetMetadata stores one metadata value for normalizedNick. An empty value, an
// unknown key, or an empty nick is ignored.
func (n *NetworkPresence) SetMetadata(normalizedNick, key, value string) {
	if normalizedNick == "" {
		return
	}
	stored := CanonicalKey(key)
	if stored == "" {
		return
	}
	if value == "" {
		presence, ok := n.nicks[normalizedNick]
		if !ok {
			return
		}
		delete(presence.Keys, stored)
		n.nicks[normalizedNick] = presence
		n.eraseIfDefault(normalizedNick)
		return
	}
	n.ensure()
	presence := n.nicks[normalizedNick]
	if presence.Keys == nil {
		presence.Keys = make(map[string]string)
	}
	presence.Keys[stored] = value
	n.nicks[normalizedNick] = presence
	n.eraseIfDefault(normalizedNick)
}

// SetAccount stores the services account for normalizedNick. `*` and an empty
// value clear it; a missing tag must never call this.
func (n *NetworkPresence) SetAccount(normalizedNick, account string) {
	if normalizedNick == "" {
		return
	}
	clear := account == "" || account == "*"
	if clear {
		presence, ok := n.nicks[normalizedNick]
		if !ok {
			return
		}
		presence.Account = ""
		n.nicks[normalizedNick] = presence
		n.eraseIfDefault(normalizedNick)
		return
	}
	n.ensure()
	presence := n.nicks[normalizedNick]
	presence.Account = account
	n.nicks[normalizedNick] = presence
}

// Rekey moves the presence from one normalized nick to another, replacing any
// entry already present at the destination.
func (n *NetworkPresence) Rekey(fromNormalized, toNormalized string) {
	if fromNormalized == toNormalized {
		return
	}
	delete(n.nicks, toNormalized)
	presence, ok := n.nicks[fromNormalized]
	if !ok {
		return
	}
	delete(n.nicks, fromNormalized)
	n.ensure()
	n.nicks[toNormalized] = presence
}

// Forget drops every fact about normalizedNick.
func (n *NetworkPresence) Forget(normalizedNick string) {
	delete(n.nicks, normalizedNick)
}

// Clear drops every nick.
func (n *NetworkPresence) Clear() {
	n.nicks = nil
}

// ClearAway drops away state for every nick, forgetting nicks that become
// default.
func (n *NetworkPresence) ClearAway() {
	for nick, presence := range n.nicks {
		presence.Away = nil
		if presence.IsDefault() {
			delete(n.nicks, nick)
			continue
		}
		n.nicks[nick] = presence
	}
}

// ClearMetadata drops metadata for every nick, forgetting nicks that become
// default.
func (n *NetworkPresence) ClearMetadata() {
	for nick, presence := range n.nicks {
		presence.Keys = nil
		if presence.IsDefault() {
			delete(n.nicks, nick)
			continue
		}
		n.nicks[nick] = presence
	}
}

// Knows reports whether any fact is stored for normalizedNick.
func (n *NetworkPresence) Knows(normalizedNick string) bool {
	_, ok := n.nicks[normalizedNick]
	return ok
}

// Lookup returns a copy of the stored presence, or the zero value when absent.
func (n *NetworkPresence) Lookup(normalizedNick string) NickPresence {
	presence, ok := n.nicks[normalizedNick]
	if !ok {
		return NickPresence{}
	}
	return presence.clone()
}

func (n *NetworkPresence) eraseIfDefault(normalizedNick string) {
	presence, ok := n.nicks[normalizedNick]
	if ok && presence.IsDefault() {
		delete(n.nicks, normalizedNick)
	}
}
