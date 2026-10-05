package irc

import "testing"

func TestChannelNameSpansUseAdvertisedTypes(t *testing.T) {
	hash := NewServerFeatures()
	hash.ApplyToken("CHANTYPES=#")
	if got := ChannelNameAt("see #desktop now", indexOf(t, "see #desktop now", "#desktop"), hash); got != "#desktop" {
		t.Fatalf("hash channel = %q, want #desktop", got)
	}
	if got := ChannelNameAt("see &local now", indexOf(t, "see &local now", "&local"), hash); got != "" {
		t.Fatalf("&local under CHANTYPES=# = %q, want empty", got)
	}

	amp := NewServerFeatures()
	amp.ApplyToken("CHANTYPES=&")
	if got := ChannelNameAt("see #desktop now", indexOf(t, "see #desktop now", "#"), amp); got != "" {
		t.Fatalf("#desktop under CHANTYPES=& = %q, want empty", got)
	}
	if got := ChannelNameAt("see &local now", indexOf(t, "see &local now", "&local"), amp); got != "&local" {
		t.Fatalf("&local under CHANTYPES=& = %q, want &local", got)
	}

	dollar := NewServerFeatures()
	dollar.ApplyToken("CHANTYPES=$")
	text := "try $secret and #nope"
	if got := ChannelNameAt(text, indexOf(t, text, "$secret"), dollar); got != "$secret" {
		t.Fatalf("$secret under CHANTYPES=$ = %q, want $secret", got)
	}
	if got := ChannelNameAt(text, indexOf(t, text, "#nope"), dollar); got != "" {
		t.Fatalf("#nope under CHANTYPES=$ = %q, want empty", got)
	}

	empty := NewServerFeatures()
	empty.ApplyToken("CHANTYPES=")
	if got := ChannelNameAt("see #desktop", indexOf(t, "see #desktop", "#"), empty); got != "" {
		t.Fatalf("empty CHANTYPES matched %q", got)
	}
}

func TestChannelNameSpansBoundariesAndTrim(t *testing.T) {
	features := NewServerFeatures()
	features.ApplyToken("CHANTYPES=#")

	if got := ChannelNameAt("foo#desktop", indexOf(t, "foo#desktop", "#"), features); got != "" {
		t.Fatalf("mid-word channel = %q, want empty", got)
	}
	if got := ChannelNameAt("https://example.com/#desktop", indexOf(t, "https://example.com/#desktop", "#desktop"), features); got != "" {
		t.Fatalf("url fragment = %q, want empty", got)
	}
	if got := ChannelNameAt("just # here", indexOf(t, "just # here", "#"), features); got != "" {
		t.Fatalf("lone hash = %q, want empty", got)
	}

	paren := "(#desktop)."
	if got := ChannelNameAt(paren, indexOf(t, paren, "#desktop"), features); got != "#desktop" {
		t.Fatalf("parenthesized channel = %q, want #desktop", got)
	}
	if got := ChannelNameAt(paren, len(paren)-1, features); got != "" {
		t.Fatalf("trailing period = %q, want empty", got)
	}

	bang := "join #foo!"
	if got := ChannelNameAt(bang, indexOf(t, bang, "#foo"), features); got != "#foo" {
		t.Fatalf("trailing bang = %q, want #foo", got)
	}
	if got := ChannelNameAt(bang, len(bang)-1, features); got != "" {
		t.Fatalf("the bang itself = %q, want empty", got)
	}

	plus := "see #c++ now"
	if got := ChannelNameAt(plus, indexOf(t, plus, "#c++"), features); got != "#c++" {
		t.Fatalf("#c++ = %q, want #c++", got)
	}

	pair := "try #foo,#bar please"
	spans := ChannelNameSpans(pair, features)
	if len(spans) != 2 || spans[0].Name != "#foo" || spans[1].Name != "#bar" {
		t.Fatalf("comma pair = %+v, want #foo and #bar", spans)
	}

	kept := "#foo_(bar)"
	if got := ChannelNameAt(kept, 0, features); got != "#foo_(bar)" {
		t.Fatalf("balanced parens = %q, want #foo_(bar)", got)
	}
}

func indexOf(t *testing.T, text, needle string) int {
	t.Helper()
	index := len(text)
	for at := 0; at+len(needle) <= len(text); at++ {
		if text[at:at+len(needle)] == needle {
			return at
		}
	}
	t.Fatalf("%q does not contain %q", text, needle)
	return index
}
