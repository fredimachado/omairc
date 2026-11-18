package irc

import "unicode"

// The vocabulary of the IRCv3 +draft/reply, +draft/react and +draft/unreact
// client tags: what a wire value must look like to be believed, and the
// per-line aggregate the reducer keeps for reactions. It mirrors
// client/src/irc/reaction.rs. Pure: nothing here reads a clock, a socket or the
// case mapping. The caller hands in the normalized nick.

const (
	// MaxReactionBytes is the longest react value kept, in bytes.
	MaxReactionBytes = 64
	// MaxReplyTargetBytes is the longest +draft/reply target kept, in bytes.
	MaxReplyTargetBytes = 128
	// MaxReactionEmojis is how many distinct emoji one line keeps.
	MaxReactionEmojis = 32
	// MaxReactionNicks is how many nicks one emoji keeps.
	MaxReactionNicks = 256
)

// validReactionValue reports whether value may be a react or unreact value:
// non-empty, at most MaxReactionBytes, and free of any control character (C0,
// DEL, C1, and so every line break).
func validReactionValue(value string) bool {
	if value == "" || len(value) > MaxReactionBytes {
		return false
	}
	for _, character := range value {
		if unicode.IsControl(character) {
			return false
		}
	}
	return true
}

// validReplyTarget reports whether value may be a +draft/reply target:
// non-empty, at most MaxReplyTargetBytes, and free of control characters and
// whitespace.
func validReplyTarget(value string) bool {
	if value == "" || len(value) > MaxReplyTargetBytes {
		return false
	}
	for _, character := range value {
		if unicode.IsControl(character) || unicode.IsSpace(character) {
			return false
		}
	}
	return true
}

type reactor struct {
	normalized string
	display    string
}

type reaction struct {
	emoji    string
	reactors []reactor
}

// ReactionSet is the reactions on one line: emoji in first-seen order, each with
// the nicks that sent it. The zero value is empty and ready to use.
type ReactionSet struct {
	reactions []reaction
}

// ReactionChip is one chip under a line: the emoji, how many nicks sent it and
// who they are, in first-seen order.
type ReactionChip struct {
	Emoji string
	Count int
	Nicks []string
}

// Add adds normalized to emoji. The same nick twice counts once. A reaction past
// the emoji or nick cap is dropped. It reports whether the set changed.
func (s *ReactionSet) Add(emoji, normalized, display string) bool {
	for index := range s.reactions {
		existing := &s.reactions[index]
		if existing.emoji != emoji {
			continue
		}
		for _, who := range existing.reactors {
			if who.normalized == normalized {
				return false
			}
		}
		if len(existing.reactors) >= MaxReactionNicks {
			return false
		}
		existing.reactors = append(existing.reactors, reactor{normalized: normalized, display: display})
		return true
	}
	if len(s.reactions) >= MaxReactionEmojis {
		return false
	}
	s.reactions = append(s.reactions, reaction{
		emoji:    emoji,
		reactors: []reactor{{normalized: normalized, display: display}},
	})
	return true
}

// Remove removes normalized from emoji. The last nick takes the chip with it.
// An absent nick is a no-op. It reports whether the set changed.
func (s *ReactionSet) Remove(emoji, normalized string) bool {
	for index := range s.reactions {
		existing := &s.reactions[index]
		if existing.emoji != emoji {
			continue
		}
		for at, who := range existing.reactors {
			if who.normalized != normalized {
				continue
			}
			existing.reactors = append(existing.reactors[:at], existing.reactors[at+1:]...)
			if len(existing.reactors) == 0 {
				s.reactions = append(s.reactions[:index], s.reactions[index+1:]...)
			}
			return true
		}
		return false
	}
	return false
}

// IsEmpty reports whether the set holds no reaction.
func (s *ReactionSet) IsEmpty() bool { return s == nil || len(s.reactions) == 0 }

// Chips returns the chips to draw, in first-seen emoji order.
func (s *ReactionSet) Chips() []ReactionChip {
	if s == nil {
		return nil
	}
	chips := make([]ReactionChip, 0, len(s.reactions))
	for _, existing := range s.reactions {
		nicks := make([]string, 0, len(existing.reactors))
		for _, who := range existing.reactors {
			nicks = append(nicks, who.display)
		}
		chips = append(chips, ReactionChip{Emoji: existing.emoji, Count: len(nicks), Nicks: nicks})
	}
	return chips
}
