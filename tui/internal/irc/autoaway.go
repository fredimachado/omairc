package irc

import (
	"math"
	"strconv"
	"strings"
	"unicode"
)

// This file is the Go port of src/irc/ircautoaway.{h,cpp}: the /autoaway
// argument parser, the duration parser and formatter, and the query,
// confirmation, and Status strings.

// AutoawayMinTimeoutSeconds is the shortest accepted auto-away timeout.
const AutoawayMinTimeoutSeconds = 30

// AutoawayMaxTimeoutSeconds mirrors numeric_limits<int>::max()/1000.
const AutoawayMaxTimeoutSeconds = 2147483

// AutoawayKind is the parsed shape of a /autoaway argument. It mirrors
// IrcAutoawayKind.
type AutoawayKind int

const (
	// AutoawayQuery reports the current configuration.
	AutoawayQuery AutoawayKind = iota
	// AutoawayDisable turns auto-away off.
	AutoawayDisable
	// AutoawayEnableOn turns auto-away on with the current timeout.
	AutoawayEnableOn
	// AutoawaySetTimeout sets the timeout and an optional one-shot reason.
	AutoawaySetTimeout
	// AutoawaySetDefaultReason sets the persistent away reason.
	AutoawaySetDefaultReason
	// AutoawayClearDefaultReason clears the persistent away reason.
	AutoawayClearDefaultReason
	// AutoawayUsage is the fallback that prints the usage line.
	AutoawayUsage
)

// AutoawayRequest is a parsed /autoaway argument. It mirrors
// IrcAutoawayRequest.
type AutoawayRequest struct {
	Kind           AutoawayKind
	TimeoutSeconds int
	Text           string
}

// AutoawayConfig is the auto-away state. It mirrors IrcAutoawayConfig.
type AutoawayConfig struct {
	Enabled        bool
	TimeoutSeconds int
	DefaultReason  string
	OneShotReason  string
}

// ParseAutoawayDuration parses a duration token. It mirrors
// ircParseAutoawayDuration: a bare number or an 'm' suffix is minutes, 's' is
// seconds, and 'h' is hours; at most one decimal place; the result must round
// into [AutoawayMinTimeoutSeconds, AutoawayMaxTimeoutSeconds].
func ParseAutoawayDuration(token string) (int, bool) {
	if token == "" {
		return 0, false
	}

	runes := []rune(token)
	index := 0
	count := len(runes)
	for index < count && unicode.IsDigit(runes[index]) {
		index++
	}
	if index == 0 {
		return 0, false
	}
	if index < count && runes[index] == '.' {
		index++
		fractionStart := index
		for index < count && unicode.IsDigit(runes[index]) {
			index++
		}
		if index == fractionStart {
			return 0, false
		}
	}

	number := string(runes[:index])
	var suffix rune
	hasSuffix := false
	if index < count {
		if index+1 != count {
			return 0, false
		}
		suffix = unicode.ToLower(runes[index])
		if suffix != 's' && suffix != 'm' && suffix != 'h' {
			return 0, false
		}
		hasSuffix = true
	}

	value, err := strconv.ParseFloat(number, 64)
	if err != nil || value < 0 {
		return 0, false
	}

	seconds := value
	if !hasSuffix || suffix == 'm' {
		seconds = value * 60.0
	} else if suffix == 'h' {
		seconds = value * 3600.0
	}

	if seconds > float64(AutoawayMaxTimeoutSeconds) {
		return 0, false
	}
	if seconds < float64(AutoawayMinTimeoutSeconds) {
		return 0, false
	}
	rounded := int(math.Round(seconds))
	if rounded < 0 || rounded > AutoawayMaxTimeoutSeconds {
		return 0, false
	}
	return rounded, true
}

// ParseAutoawayArgument parses a /autoaway argument. It mirrors
// ircParseAutoawayArgument.
func ParseAutoawayArgument(argument string) AutoawayRequest {
	request := AutoawayRequest{Kind: AutoawayUsage}
	trimmed := strings.TrimSpace(argument)
	if trimmed == "" {
		request.Kind = AutoawayQuery
		return request
	}

	token := firstSpaceToken(trimmed)
	rest := restAfterFirstSpaceToken(trimmed)
	folded := strings.ToLower(token)

	switch folded {
	case "off", "0":
		if rest != "" {
			return request
		}
		request.Kind = AutoawayDisable
		return request
	case "on":
		if rest != "" {
			return request
		}
		request.Kind = AutoawayEnableOn
		return request
	case "reason":
		if rest == "" {
			request.Kind = AutoawayClearDefaultReason
			return request
		}
		request.Kind = AutoawaySetDefaultReason
		request.Text = rest
		return request
	}

	seconds, ok := ParseAutoawayDuration(token)
	if !ok || seconds < AutoawayMinTimeoutSeconds {
		return request
	}
	request.Kind = AutoawaySetTimeout
	request.TimeoutSeconds = seconds
	request.Text = rest
	return request
}

// FormatAutoawayDuration renders a whole number of hours, minutes, or seconds.
// It mirrors ircFormatAutoawayDuration.
func FormatAutoawayDuration(seconds int) string {
	if seconds >= 3600 && seconds%3600 == 0 {
		hours := seconds / 3600
		if hours == 1 {
			return "1 hour"
		}
		return strconv.Itoa(hours) + " hours"
	}
	if seconds >= 60 && seconds%60 == 0 {
		minutes := seconds / 60
		if minutes == 1 {
			return "1 minute"
		}
		return strconv.Itoa(minutes) + " minutes"
	}
	if seconds == 1 {
		return "1 second"
	}
	return strconv.Itoa(seconds) + " seconds"
}

// FormatAutoawayQuery renders the current configuration for a query. It
// mirrors ircFormatAutoawayQuery.
func FormatAutoawayQuery(config AutoawayConfig) string {
	if !config.Enabled {
		return formatAutoawayDisabled(config, true)
	}
	return formatAutoawayEnabled(config, true)
}

// FormatAutoawayConfirmation renders the confirmation after a change. It
// mirrors ircFormatAutoawayConfirmation.
func FormatAutoawayConfirmation(config AutoawayConfig) string {
	if !config.Enabled {
		return formatAutoawayDisabled(config, false)
	}
	return formatAutoawayEnabled(config, false)
}

// FormatAutoawayTrippedStatus renders the Status line for a fired auto-away.
// It mirrors ircFormatAutoawayTrippedStatus.
func FormatAutoawayTrippedStatus(reason string) string {
	trimmed := strings.TrimSpace(reason)
	if trimmed == "" {
		return "Auto-away triggered."
	}
	return "Auto-away triggered: " + trimmed
}

// FormatAutoawayClearedStatus renders the Status line when auto-away clears.
// It mirrors ircFormatAutoawayClearedStatus.
func FormatAutoawayClearedStatus() string {
	return "Auto-away cleared — back online."
}

// AutoawayGraceSeconds returns the grace period before auto-away fires,
// clamped to [5, 10] and 0 for a non-positive timeout. It mirrors
// ircAutoawayGraceSeconds.
func AutoawayGraceSeconds(timeoutSeconds int) int {
	if timeoutSeconds <= 0 {
		return 0
	}
	grace := timeoutSeconds / 20
	if grace < 5 {
		return 5
	}
	if grace > 10 {
		return 10
	}
	return grace
}

// formatAutoawayEnabled renders an enabled configuration. When
// includeDefaultWithOneShot is false the default reason is omitted while a
// one-shot reason is set, matching the C++ confirmation shape.
func formatAutoawayEnabled(config AutoawayConfig, includeDefaultWithOneShot bool) string {
	if config.OneShotReason != "" {
		text := "Auto-away in " + FormatAutoawayDuration(config.TimeoutSeconds) +
			": " + config.OneShotReason + " (this time only)"
		if includeDefaultWithOneShot && config.DefaultReason != "" {
			text += ", reason: " + config.DefaultReason
		}
		return text
	}
	text := "Auto-away " + FormatAutoawayDuration(config.TimeoutSeconds)
	if config.DefaultReason != "" {
		text += ", reason: " + config.DefaultReason
	}
	return text
}

// formatAutoawayDisabled renders a disabled configuration. includeTimeout
// controls whether the current timeout is echoed back.
func formatAutoawayDisabled(config AutoawayConfig, includeTimeout bool) string {
	text := "Auto-away off"
	if includeTimeout && config.TimeoutSeconds > 0 {
		text += ", " + FormatAutoawayDuration(config.TimeoutSeconds)
	}
	if config.OneShotReason != "" {
		text += ": " + config.OneShotReason + " (this time only)"
	}
	if config.DefaultReason != "" {
		text += ", reason: " + config.DefaultReason
	}
	return text
}

// firstSpaceToken returns the text before the first ASCII space, or the whole
// string when there is none.
func firstSpaceToken(argument string) string {
	space := strings.IndexByte(argument, ' ')
	if space < 0 {
		return argument
	}
	return argument[:space]
}

// restAfterFirstSpaceToken returns the trimmed text after the first ASCII
// space, or "" when there is none.
func restAfterFirstSpaceToken(argument string) string {
	space := strings.IndexByte(argument, ' ')
	if space < 0 {
		return ""
	}
	return strings.TrimSpace(argument[space+1:])
}
