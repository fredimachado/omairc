package irc

import (
	"strings"
	"time"
)

// TypingClock selects the hold duration for a stored typing hint. It mirrors
// IrcTypingClock; the zero value is Active.
type TypingClock int

const (
	// TypingClockActive is an active typing hint, held for TypingActiveHoldMs.
	TypingClockActive TypingClock = iota
	// TypingClockPaused is a paused typing hint, held for TypingPausedHoldMs.
	TypingClockPaused
)

// TypingPhase is a typing phase from the +typing client tag. It mirrors
// IrcTypingPhase; the zero value is Active.
type TypingPhase int

const (
	// TypingActive reports that the sender is typing.
	TypingActive TypingPhase = iota
	// TypingPaused reports that the sender paused, still assumed typing.
	TypingPaused
	// TypingDone reports that the sender stopped; never stored.
	TypingDone
)

// Typing hold durations, mirrored from irctyping.h.
const (
	// TypingActiveHoldMs is how long an active hint is retained.
	TypingActiveHoldMs = 6000
	// TypingPausedHoldMs is how long a paused hint is retained, following the
	// IRCv3 typing client-tag recommendation to assume the sender is still
	// typing for at least 30 seconds after typing=paused.
	TypingPausedHoldMs = 30000
	// TypingSendIntervalMs is the per-target outbound throttle.
	TypingSendIntervalMs = 3000
)

// TypingHint is a stored inbound typing hint. It mirrors IrcTypingHint.
type TypingHint struct {
	Clock       TypingClock
	ReceivedAt  time.Time
	DisplayNick string
}

// StoredTypingHint builds the hint for a received phase. Done never stores a
// hint, so ok is false and the returned hint is the zero value.
func StoredTypingHint(phase TypingPhase, receivedAt time.Time, displayNick string) (TypingHint, bool) {
	if phase == TypingDone {
		return TypingHint{}, false
	}
	hint := TypingHint{
		Clock:       TypingClockActive,
		ReceivedAt:  receivedAt,
		DisplayNick: displayNick,
	}
	if phase == TypingPaused {
		hint.Clock = TypingClockPaused
	}
	return hint, true
}

// TypingExpiresAt returns when the hint's clock-specific hold ends.
func TypingExpiresAt(hint TypingHint) time.Time {
	holdMs := TypingActiveHoldMs
	if hint.Clock == TypingClockPaused {
		holdMs = TypingPausedHoldMs
	}
	return hint.ReceivedAt.Add(time.Duration(holdMs) * time.Millisecond)
}

// TypingHintRetained reports whether the hint is still stored. An unset
// ReceivedAt or now is never retained.
func TypingHintRetained(hint TypingHint, now time.Time) bool {
	if hint.ReceivedAt.IsZero() || now.IsZero() {
		return false
	}
	return now.Before(TypingExpiresAt(hint))
}

// TypingShowsIndicator reports whether to paint the typing indicator. Active
// and Paused both show; they differ only in hold duration.
func TypingShowsIndicator(hint TypingHint, now time.Time) bool {
	return TypingHintRetained(hint, now)
}

// TypingPhaseFromTag resolves a +typing tag value, case-insensitively.
func TypingPhaseFromTag(value string) (TypingPhase, bool) {
	switch {
	case strings.EqualFold(value, "active"):
		return TypingActive, true
	case strings.EqualFold(value, "paused"):
		return TypingPaused, true
	case strings.EqualFold(value, "done"):
		return TypingDone, true
	}
	return TypingActive, false
}

// TypingTagmsg builds the TAGMSG frame for a typing notification, byte-for-byte
// like ircTypingTagmsg. A target containing a space, CR, or LF, or an empty
// target, yields nil.
func TypingTagmsg(target string, phase TypingPhase) []byte {
	if target == "" {
		return nil
	}
	for index := 0; index < len(target); index++ {
		switch target[index] {
		case ' ', '\r', '\n':
			return nil
		}
	}

	value := "active"
	if phase == TypingPaused {
		value = "paused"
	} else if phase == TypingDone {
		value = "done"
	}

	line := "@+typing=" + value + " TAGMSG " + target + "\r\n"
	return []byte(line)
}
