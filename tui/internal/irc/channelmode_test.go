package irc

import (
	"slices"
	"testing"
)

func channelModeFeatures() ServerFeatures {
	features := NewServerFeatures()
	features.ApplyToken("CHANTYPES=#")
	return features
}

func TestParseChannelModeQuery(t *testing.T) {
	features := channelModeFeatures()
	for _, argument := range []string{"#chan", "  #chan  ", "#chan"} {
		request, ok := ParseChannelModeRequest(argument, features)
		if !ok {
			t.Fatalf("ParseChannelModeRequest(%q) refused", argument)
		}
		if !request.Query || request.Channel != "#chan" || request.Modes != "" ||
			len(request.Parameters) != 0 {
			t.Fatalf("ParseChannelModeRequest(%q) = %+v", argument, request)
		}
	}
}

func TestParseChannelModeChange(t *testing.T) {
	features := channelModeFeatures()
	request, ok := ParseChannelModeRequest("#chan +o alice", features)
	if !ok {
		t.Fatal("ParseChannelModeRequest refused a valid change")
	}
	if request.Query || request.Channel != "#chan" || request.Modes != "+o" {
		t.Fatalf("unexpected change: %+v", request)
	}
	if !slices.Equal(request.Parameters, []string{"alice"}) {
		t.Fatalf("parameters = %v, want [alice]", request.Parameters)
	}
}

func TestParseChannelModeMultiParameter(t *testing.T) {
	features := channelModeFeatures()
	request, ok := ParseChannelModeRequest("#chan +ov alice bob", features)
	if !ok {
		t.Fatal("ParseChannelModeRequest refused a valid change")
	}
	if request.Modes != "+ov" || !slices.Equal(request.Parameters, []string{"alice", "bob"}) {
		t.Fatalf("unexpected change: %+v", request)
	}
}

func TestParseChannelModeRejectsNonChannel(t *testing.T) {
	features := channelModeFeatures()
	for _, argument := range []string{"alice +o bob", "&other +o bob"} {
		if _, ok := ParseChannelModeRequest(argument, features); ok {
			t.Fatalf("ParseChannelModeRequest(%q) should refuse a non-channel", argument)
		}
	}
}

func TestParseChannelModeRejectsEmpty(t *testing.T) {
	features := channelModeFeatures()
	for _, argument := range []string{"", "   "} {
		if _, ok := ParseChannelModeRequest(argument, features); ok {
			t.Fatalf("ParseChannelModeRequest(%q) should refuse empty input", argument)
		}
	}
}

func TestParseChannelModeRejectsControlCharacters(t *testing.T) {
	features := channelModeFeatures()
	for _, argument := range []string{
		"#chan +o a\rb",
		"#chan\n+o alice",
		"#chan\x00",
		"#chan +o ali\x00ce",
	} {
		if _, ok := ParseChannelModeRequest(argument, features); ok {
			t.Fatalf("ParseChannelModeRequest(%q) should refuse a control character", argument)
		}
	}
}

func TestParseChannelModeHonorsChannelTypes(t *testing.T) {
	features := NewServerFeatures()
	features.ApplyToken("CHANTYPES=&")
	if _, ok := ParseChannelModeRequest("#chan", features); ok {
		t.Fatal("#chan should not be a channel when CHANTYPES=&")
	}
	if request, ok := ParseChannelModeRequest("&chan +o alice", features); !ok ||
		request.Channel != "&chan" {
		t.Fatalf("&chan should parse when CHANTYPES=&: %+v ok=%v", request, ok)
	}
}
