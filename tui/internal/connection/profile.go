package connection

import (
	crand "crypto/rand"
	"encoding/base64"
	"maps"
	mrand "math/rand/v2"
	"slices"
	"strings"
	"unicode"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

// Problem identifies the first reason a NetworkProfile cannot be applied. It
// mirrors IrcNetworkProfile::Problem.
type Problem int

const (
	// ProblemNone means the profile is complete and sendable.
	ProblemNone Problem = iota
	// ProblemMissingHost means the host is empty.
	ProblemMissingHost
	// ProblemMissingNick means the nick is empty.
	ProblemMissingNick
	// ProblemInvalidPort means the port is zero.
	ProblemInvalidPort
	// ProblemUnsendableIdentity means the nick, username, or realname cannot
	// be encoded as a NICK/USER pair.
	ProblemUnsendableIdentity
	// ProblemUnsendableAccount means the SASL account contains a space, NUL,
	// or slash.
	ProblemUnsendableAccount
	// ProblemUnsendableBouncerNetwork means the bouncer network contains a
	// space, NUL, or slash.
	ProblemUnsendableBouncerNetwork
	// ProblemUnsendableChannel means an autojoin channel cannot be encoded as
	// a JOIN.
	ProblemUnsendableChannel
)

const (
	// IconColorCount is the number of palette slots a network may use.
	IconColorCount = 5
	// NoIconColor marks a profile that has not been assigned a palette slot.
	NoIconColor = -1
)

// NetworkProfile is the Go shape of IrcNetworkProfile: one network's identity,
// credentials metadata, autojoin state, and presentation fields.
type NetworkProfile struct {
	NetworkID        string
	Name             string
	Host             string
	Port             uint16
	TLSEnabled       bool
	ConnectOnStartup bool
	SecretSaved      bool
	NickServSaved    bool
	Nick             string
	Username         string
	Realname         string
	Account          string
	BouncerNetwork   string
	AutojoinChannels []string
	AutojoinKeys     map[string]string
	IconColor        int
	AvatarURL        string
}

// CreateProfile builds a fresh profile with a random network id: eight random
// bytes encoded with unpadded URL-safe base64, mirroring
// IrcNetworkProfile::create. The remaining defaults match the C++ struct
// (port 6697, TLS on, no icon color).
func CreateProfile() NetworkProfile {
	var bytes [8]byte
	// crypto/rand.Read never returns an error and always fills its buffer on
	// supported platforms, matching the infallible C++ generator.
	_, _ = crand.Read(bytes[:])
	return NetworkProfile{
		NetworkID:  base64.RawURLEncoding.EncodeToString(bytes[:]),
		Port:       6697,
		TLSEnabled: true,
		IconColor:  NoIconColor,
	}
}

// SuggestedProfile returns the first-run Libera profile, mirroring
// IrcNetworkProfile::suggested.
func SuggestedProfile() NetworkProfile {
	profile := CreateProfile()
	profile.Host = "irc.libera.chat"
	profile.Name = profile.Host
	profile.Port = 6697
	profile.TLSEnabled = true
	profile.AutojoinChannels = []string{"#omarchy"}
	return profile
}

// ParseAutojoin splits a typed autojoin string on runs of commas and
// whitespace, drops an empty token and a channel key that follows a channel,
// and keeps the remaining tokens verbatim (without prefixing a missing '#').
// It mirrors IrcNetworkProfile::parseAutojoin.
func ParseAutojoin(channels string) []string {
	var result []string
	previousWasChannel := false
	for _, part := range splitAutojoinTokens(channels) {
		token := strings.TrimSpace(part)
		if !keepAutojoinToken(token, previousWasChannel) {
			continue
		}
		result = append(result, token)
		previousWasChannel = tokenIsChannel(token)
	}
	return result
}

// ResolvedName returns the trimmed name when it is non-empty, else the trimmed
// host. It mirrors IrcNetworkProfile::resolvedName.
func ResolvedName(name, host string) string {
	if trimmed := strings.TrimSpace(name); trimmed != "" {
		return trimmed
	}
	return strings.TrimSpace(host)
}

// ResolvedName applies ResolvedName to the profile.
func (p NetworkProfile) ResolvedName() string {
	return ResolvedName(p.Name, p.Host)
}

// SASLAccount returns the SASL account name: the normalized account, else the
// normalized nick, with the bouncer's upstream network appended after a slash.
// It mirrors IrcNetworkProfile::saslAccount.
func (p NetworkProfile) SASLAccount() string {
	profile := p.Normalized()
	name := profile.Account
	if name == "" {
		name = profile.Nick
	}
	if profile.BouncerNetwork == "" {
		return name
	}
	return name + "/" + profile.BouncerNetwork
}

// Normalized returns a trimmed copy with a canonical autojoin list and only the
// keys that still match a kept channel. It mirrors
// IrcNetworkProfile::normalized.
func (p NetworkProfile) Normalized() NetworkProfile {
	profile := p
	profile.Name = strings.TrimSpace(profile.Name)
	profile.Host = strings.TrimSpace(profile.Host)
	profile.Nick = strings.TrimSpace(profile.Nick)
	profile.Username = strings.TrimSpace(profile.Username)
	profile.Realname = strings.TrimSpace(profile.Realname)
	profile.Account = strings.TrimSpace(profile.Account)
	profile.BouncerNetwork = strings.TrimSpace(profile.BouncerNetwork)
	profile.AutojoinChannels = normalizedAutojoinChannels(profile.AutojoinChannels)
	profile.AutojoinKeys = retainAutojoinKeys(profile.AutojoinKeys, profile.AutojoinChannels)
	return profile
}

// Validate reports the first reason the normalized profile cannot be applied.
// It mirrors IrcNetworkProfile::validate, including the check order.
func (p NetworkProfile) Validate() Problem {
	profile := p.Normalized()
	if profile.Host == "" {
		return ProblemMissingHost
	}
	if profile.Port == 0 {
		return ProblemInvalidPort
	}
	if profile.Nick == "" {
		return ProblemMissingNick
	}
	if _, err := irc.Nick(profile.Nick); err != nil {
		return ProblemUnsendableIdentity
	}
	if profile.Username != "" || profile.Realname != "" {
		userField := profile.Username
		if userField == "" {
			userField = profile.Nick
		}
		realField := profile.Realname
		if realField == "" {
			realField = profile.Nick
		}
		if _, err := irc.User(userField, realField); err != nil {
			return ProblemUnsendableIdentity
		}
	}
	if !fitsOneLoginField(profile.Account) {
		return ProblemUnsendableAccount
	}
	if !fitsOneLoginField(profile.BouncerNetwork) {
		return ProblemUnsendableBouncerNetwork
	}
	for _, channel := range profile.AutojoinChannels {
		if _, err := irc.Join(channel, ""); err != nil {
			return ProblemUnsendableChannel
		}
	}
	return ProblemNone
}

// IsComplete reports whether Validate finds no problem.
func (p NetworkProfile) IsComplete() bool {
	return p.Validate() == ProblemNone
}

// ProblemText renders a Problem the way IrcNetworkProfile::problemText does.
func ProblemText(problem Problem) string {
	switch problem {
	case ProblemNone:
		return ""
	case ProblemMissingHost:
		return "Host is required"
	case ProblemMissingNick:
		return "Nick is required"
	case ProblemInvalidPort:
		return "Port is required"
	case ProblemUnsendableIdentity:
		return "Nick, username, or real name cannot be sent"
	case ProblemUnsendableAccount:
		return "Account cannot contain a space or a slash"
	case ProblemUnsendableBouncerNetwork:
		return "Bouncer network cannot contain a space or a slash"
	case ProblemUnsendableChannel:
		return "An autojoin channel cannot be sent"
	}
	return ""
}

// PickIconColor returns a palette slot not present in used. With every slot
// taken it picks any slot; with a single free slot it picks that one. It
// mirrors IrcNetworkProfile::pickIconColor.
func PickIconColor(used []int) int {
	free := make([]int, 0, IconColorCount)
	for slot := 0; slot < IconColorCount; slot++ {
		if !slices.Contains(used, slot) {
			free = append(free, slot)
		}
	}
	if len(free) == 0 {
		return mrand.IntN(IconColorCount)
	}
	if len(free) == 1 {
		return free[0]
	}
	return free[mrand.IntN(len(free))]
}

// EnsureIconColor assigns a palette slot when the profile has none. It reports
// whether it changed the profile, mirroring
// IrcNetworkProfile::ensureIconColor.
func (p *NetworkProfile) EnsureIconColor(used []int) bool {
	if p.IconColor >= 0 && p.IconColor < IconColorCount {
		return false
	}
	p.IconColor = PickIconColor(used)
	return true
}

// profilesEqual compares every profile field. It replaces the C++ operator==
// that Dirty relies on; slices and maps need explicit comparison.
func profilesEqual(left, right NetworkProfile) bool {
	return left.NetworkID == right.NetworkID &&
		left.Name == right.Name &&
		left.Host == right.Host &&
		left.Port == right.Port &&
		left.TLSEnabled == right.TLSEnabled &&
		left.ConnectOnStartup == right.ConnectOnStartup &&
		left.SecretSaved == right.SecretSaved &&
		left.NickServSaved == right.NickServSaved &&
		left.Nick == right.Nick &&
		left.Username == right.Username &&
		left.Realname == right.Realname &&
		left.Account == right.Account &&
		left.BouncerNetwork == right.BouncerNetwork &&
		slices.Equal(left.AutojoinChannels, right.AutojoinChannels) &&
		maps.Equal(left.AutojoinKeys, right.AutojoinKeys) &&
		left.IconColor == right.IconColor &&
		left.AvatarURL == right.AvatarURL
}

// splitAutojoinTokens splits on runs of commas and whitespace, dropping empty
// fields exactly like a Qt split on "[,\s]+" with Qt::SkipEmptyParts.
func splitAutojoinTokens(value string) []string {
	return strings.FieldsFunc(value, func(character rune) bool {
		return character == ',' || unicode.IsSpace(character)
	})
}

func keepAutojoinToken(token string, previousWasChannel bool) bool {
	if token == "" {
		return false
	}
	if tokenIsChannel(token) {
		return true
	}
	return !previousWasChannel
}

func tokenIsChannel(token string) bool {
	if token == "" {
		return false
	}
	features := irc.NewServerFeatures()
	return features.IsChannel(token)
}

func prefixChannel(token string) string {
	token = strings.TrimSpace(token)
	if token == "" {
		return ""
	}
	if tokenIsChannel(token) {
		return token
	}
	return "#" + token
}

// normalizedAutojoinChannels prefixes a bare token with '#', keeps an explicit
// channel, and drops a channel key that follows a channel. It mirrors the
// anonymous splitAutojoin in ircnetworkprofile.cpp.
func normalizedAutojoinChannels(channels []string) []string {
	var result []string
	previousWasChannel := false
	for _, entry := range channels {
		for _, part := range splitAutojoinTokens(entry) {
			token := strings.TrimSpace(part)
			if !keepAutojoinToken(token, previousWasChannel) {
				continue
			}
			channel := prefixChannel(token)
			if channel == "" {
				continue
			}
			result = append(result, channel)
			previousWasChannel = tokenIsChannel(token)
		}
	}
	return result
}

// retainAutojoinKeys keeps the keys whose channel survived normalization,
// matched with the server case mapping and re-keyed to the canonical channel
// name. It mirrors the anonymous retainAutojoinKeys.
func retainAutojoinKeys(keys map[string]string, channels []string) map[string]string {
	features := irc.NewServerFeatures()
	mapping := features.CaseMapping()
	retained := make(map[string]string, len(channels))
	sortedKeys := slices.Sorted(maps.Keys(keys))
	for _, channel := range channels {
		for _, key := range sortedKeys {
			if mapping.Equals(key, channel) {
				retained[channel] = keys[key]
				break
			}
		}
	}
	return retained
}

// fitsOneLoginField rejects a value carrying a space, NUL, or slash, mirroring
// IrcNetworkProfile's anonymous fitsOneLoginField (QChar::isSpace is
// Unicode-aware).
func fitsOneLoginField(value string) bool {
	for _, mark := range value {
		if unicode.IsSpace(mark) || mark == 0 || mark == '/' {
			return false
		}
	}
	return true
}
