package irc

import (
	"bytes"
	"strings"
	"testing"
)

func TestFramesLatin1ThenUtf8(t *testing.T) {
	var framer Framer
	wire := []byte(":a!u@h PRIVMSG #c :")
	wire = append(wire, 0xe9)
	wire = append(wire, []byte("\r\n:b!u@h PRIVMSG #c :ok\r\n")...)

	result := framer.Feed(wire)
	if len(result.Faults) != 0 {
		t.Fatalf("faults = %+v, want none", result.Faults)
	}
	if len(result.Frames) != 2 {
		t.Fatalf("frames = %q, want 2", result.Frames)
	}

	first, err := Parse(result.Frames[0])
	if err != nil {
		t.Fatalf("first frame parse failed: %v", err)
	}
	second, err := Parse(result.Frames[1])
	if err != nil {
		t.Fatalf("second frame parse failed: %v", err)
	}
	if first.Params[1] != string([]byte{0xe9}) {
		t.Fatalf("first body = %q, want a lone 0xE9", first.Params[1])
	}
	if second.Params[1] != "ok" {
		t.Fatalf("second body = %q, want ok", second.Params[1])
	}
}

func TestFramesFragmentedAndCoalescedInput(t *testing.T) {
	var framer Framer
	if result := framer.Feed([]byte("PIN")); len(result.Frames) != 0 {
		t.Fatalf("partial feed produced frames: %q", result.Frames)
	}
	if result := framer.Feed([]byte("G :token")); len(result.Frames) != 0 {
		t.Fatalf("partial feed produced frames: %q", result.Frames)
	}

	result := framer.Feed([]byte("\r\n"))
	if len(result.Frames) != 1 || result.Frames[0] != "PING :token" {
		t.Fatalf("frames = %q, want [PING :token]", result.Frames)
	}

	result = framer.Feed([]byte("PING :one\r\nPING :two\r\n"))
	if len(result.Frames) != 2 {
		t.Fatalf("frames = %q, want 2", result.Frames)
	}
	if result.Frames[0] != "PING :one" || result.Frames[1] != "PING :two" {
		t.Fatalf("frames = %q, want [PING :one PING :two]", result.Frames)
	}
}

func TestRejectsNulAndRecovers(t *testing.T) {
	var framer Framer
	wire := []byte("PRIVMSG #c :hel")
	wire = append(wire, 0)
	wire = append(wire, []byte("lo\r\nPING :ok\r\n")...)

	result := framer.Feed(wire)
	if len(result.Faults) != 1 {
		t.Fatalf("faults = %+v, want 1", result.Faults)
	}
	if result.Faults[0].Err != ErrorInvalidCharacter {
		t.Fatalf("fault err = %v, want invalid character", result.Faults[0].Err)
	}
	if result.Faults[0].ByteCount != 18 {
		t.Fatalf("fault bytes = %d, want 18", result.Faults[0].ByteCount)
	}
	if len(result.Frames) != 1 || result.Frames[0] != "PING :ok" {
		t.Fatalf("frames = %q, want [PING :ok]", result.Frames)
	}
}

func TestRejectsOverlongAndRecovers(t *testing.T) {
	var framer Framer
	overlongBytes := MaxInboundClassicFrameBytes - 1
	wire := bytes.Repeat([]byte("X"), overlongBytes)
	wire = append(wire, []byte("\r\nPING :complete\r\n")...)

	result := framer.Feed(wire)
	if len(result.Faults) != 1 {
		t.Fatalf("faults = %+v, want 1", result.Faults)
	}
	if result.Faults[0].Err != ErrorTooManyBytes {
		t.Fatalf("fault err = %v, want too many bytes", result.Faults[0].Err)
	}
	if result.Faults[0].ByteCount != overlongBytes {
		t.Fatalf("fault bytes = %d, want %d", result.Faults[0].ByteCount, overlongBytes)
	}
	if result.Faults[0].Preview != strings.Repeat("X", 160) {
		t.Fatalf("fault preview = %q, want 160 X", result.Faults[0].Preview)
	}
	if len(result.Frames) != 1 || result.Frames[0] != "PING :complete" {
		t.Fatalf("frames = %q, want [PING :complete]", result.Frames)
	}

	result = framer.Feed(bytes.Repeat([]byte("Y"), MaxInboundClassicFrameBytes))
	if len(result.Faults) != 1 {
		t.Fatalf("faults = %+v, want 1", result.Faults)
	}
	if len(result.Frames) != 0 {
		t.Fatalf("frames = %q, want none", result.Frames)
	}

	result = framer.Feed([]byte("tail\r\nPING :after\r\n"))
	if len(result.Frames) != 1 || result.Frames[0] != "PING :after" {
		t.Fatalf("frames = %q, want [PING :after]", result.Frames)
	}
}

func TestEnforcesClassicFrameBoundary(t *testing.T) {
	var framer Framer
	spec := append([]byte("PING :"), bytes.Repeat([]byte("z"), 504)...)
	spec = append(spec, []byte("\r\n")...)
	if len(spec) != MaxClassicFrameBytes {
		t.Fatalf("spec length = %d, want %d", len(spec), MaxClassicFrameBytes)
	}
	result := framer.Feed(spec)
	if len(result.Frames) != 1 || len(result.Faults) != 0 {
		t.Fatalf("frames = %q faults = %+v, want one clean frame", result.Frames, result.Faults)
	}

	overSpec := append([]byte("PING :"), bytes.Repeat([]byte("z"), 505)...)
	overSpec = append(overSpec, []byte("\r\n")...)
	result = framer.Feed(overSpec)
	if len(result.Frames) != 1 || len(result.Faults) != 0 {
		t.Fatalf("frames = %q faults = %+v, want one clean frame", result.Frames, result.Faults)
	}

	exact := append([]byte("PING :"), bytes.Repeat([]byte("z"), MaxInboundClassicFrameBytes-len("PING :")-2)...)
	exact = append(exact, []byte("\r\n")...)
	if len(exact) != MaxInboundClassicFrameBytes {
		t.Fatalf("exact length = %d, want %d", len(exact), MaxInboundClassicFrameBytes)
	}
	result = framer.Feed(exact)
	if len(result.Frames) != 1 || len(result.Faults) != 0 {
		t.Fatalf("frames = %q faults = %+v, want one clean frame", result.Frames, result.Faults)
	}

	tooLong := append([]byte("PING :"), bytes.Repeat([]byte("z"), MaxInboundClassicFrameBytes-len("PING :")-1)...)
	tooLong = append(tooLong, []byte("\r\nPING :short\r\n")...)
	result = framer.Feed(tooLong)
	if len(result.Faults) != 1 {
		t.Fatalf("faults = %+v, want 1", result.Faults)
	}
	if result.Faults[0].Err != ErrorTooManyBytes {
		t.Fatalf("fault err = %v, want too many bytes", result.Faults[0].Err)
	}
	if result.Faults[0].ByteCount != MaxInboundClassicFrameBytes-1 {
		t.Fatalf("fault bytes = %d, want %d", result.Faults[0].ByteCount, MaxInboundClassicFrameBytes-1)
	}
	if !strings.HasPrefix(result.Faults[0].Preview, "PING :") {
		t.Fatalf("fault preview = %q, want PING : prefix", result.Faults[0].Preview)
	}
	if len(result.Frames) != 1 || result.Frames[0] != "PING :short" {
		t.Fatalf("frames = %q, want [PING :short]", result.Frames)
	}
}

func TestAcceptsTaggedClassicFrame(t *testing.T) {
	var framer Framer
	tagged := append([]byte("@label=123 PING :"), bytes.Repeat([]byte("z"), 504)...)
	tagged = append(tagged, []byte("\r\n")...)

	result := framer.Feed(tagged)
	if len(result.Faults) != 0 {
		t.Fatalf("faults = %+v, want none", result.Faults)
	}
	if len(result.Frames) != 1 {
		t.Fatalf("frames = %q, want 1", result.Frames)
	}
	if _, err := Parse(result.Frames[0]); err != nil {
		t.Fatalf("tagged classic frame failed to parse: %v", err)
	}
}

func TestEnforcesTagSectionBoundary(t *testing.T) {
	var framer Framer
	exact := append([]byte("@"), bytes.Repeat([]byte("a"), MaxTagSectionBytes)...)
	exact = append(exact, []byte(" PING :ok\r\n")...)

	result := framer.Feed(exact)
	if len(result.Faults) != 0 {
		t.Fatalf("faults = %+v, want none", result.Faults)
	}
	if len(result.Frames) != 1 {
		t.Fatalf("frames = %q, want 1", result.Frames)
	}
	if _, err := Parse(result.Frames[0]); err != nil {
		t.Fatalf("tag-boundary frame failed to parse: %v", err)
	}

	tooLong := append([]byte("@"), bytes.Repeat([]byte("a"), MaxTagSectionBytes+1)...)
	tooLong = append(tooLong, []byte(" PING :no\r\nPING :recovered\r\n")...)
	result = framer.Feed(tooLong)
	if len(result.Faults) != 1 {
		t.Fatalf("faults = %+v, want 1", result.Faults)
	}
	if len(result.Frames) != 1 || result.Frames[0] != "PING :recovered" {
		t.Fatalf("frames = %q, want [PING :recovered]", result.Frames)
	}
}
