package irc

import (
	"fmt"
	"strings"
	"testing"
)

func TestReactionValueIsShortVisibleText(t *testing.T) {
	for _, good := range []string{"👍", "+1", strings.Repeat("a", MaxReactionBytes)} {
		if !validReactionValue(good) {
			t.Errorf("validReactionValue(%q) = false", good)
		}
	}
	for _, bad := range []string{"", strings.Repeat("a", MaxReactionBytes+1), "a\nb", "a\rb", "\x07", "a\x7fb", "a\u0085b", "\x1b[31m", "a\x00b"} {
		if validReactionValue(bad) {
			t.Errorf("validReactionValue(%q) = true", bad)
		}
	}
}

func TestReplyTargetIsOneOpaqueToken(t *testing.T) {
	for _, good := range []string{"abc123", strings.Repeat("m", MaxReplyTargetBytes)} {
		if !validReplyTarget(good) {
			t.Errorf("validReplyTarget(%q) = false", good)
		}
	}
	for _, bad := range []string{"", strings.Repeat("m", MaxReplyTargetBytes+1), "a b", "a\tb", "a\nb", "a\x7fb", "a\u0085b"} {
		if validReplyTarget(bad) {
			t.Errorf("validReplyTarget(%q) = true", bad)
		}
	}
}

func TestReactionNickCountsOncePerEmojiAndTheLastOneTakesTheChip(t *testing.T) {
	var set ReactionSet
	requireTrue(t, "first add", set.Add("👍", "bob", "Bob"))
	requireFalse(t, "same nick twice", set.Add("👍", "bob", "bob"))
	requireTrue(t, "second nick", set.Add("👍", "carol", "carol"))
	requireTrue(t, "second emoji", set.Add("🎉", "bob", "Bob"))
	chips := set.Chips()
	requireInt(t, "chips", len(chips), 2)
	requireString(t, "first emoji", chips[0].Emoji, "👍")
	requireInt(t, "first count", chips[0].Count, 2)
	requireStrings(t, "first nicks", chips[0].Nicks, []string{"Bob", "carol"})
	requireString(t, "second emoji", chips[1].Emoji, "🎉")

	requireFalse(t, "absent nick", set.Remove("👍", "dave"))
	requireFalse(t, "absent emoji", set.Remove("🔥", "bob"))
	requireTrue(t, "remove", set.Remove("🎉", "bob"))
	requireInt(t, "last nick takes the chip", len(set.Chips()), 1)
	requireTrue(t, "remove bob", set.Remove("👍", "bob"))
	requireTrue(t, "remove carol", set.Remove("👍", "carol"))
	requireTrue(t, "empty", set.IsEmpty())
}

func TestReactionCapsDropAFloodWithoutGrowing(t *testing.T) {
	var set ReactionSet
	for index := 0; index < MaxReactionEmojis+5; index++ {
		set.Add(fmt.Sprintf("e%d", index), "bob", "bob")
	}
	requireInt(t, "emojis", len(set.Chips()), MaxReactionEmojis)
	for index := 0; index < MaxReactionNicks+5; index++ {
		set.Add("e0", fmt.Sprintf("n%d", index), "n")
	}
	requireInt(t, "nicks", set.Chips()[0].Count, MaxReactionNicks)
}
