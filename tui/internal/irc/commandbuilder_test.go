package irc

import (
	"strings"
	"testing"
)

func TestBuildsRegistration(t *testing.T) {
	result, err := Registration("alice", "aliceuser", "Omairc User", "")
	if err != nil {
		t.Fatalf("Registration failed: %v", err)
	}
	want := "NICK alice\r\nUSER aliceuser 8 * :Omairc User\r\n"
	if result != want {
		t.Fatalf("registration = %q, want %q", result, want)
	}
	if strings.Contains(result, "HELLO") {
		t.Fatalf("registration leaked a default realname: %q", result)
	}

	result, err = Registration("alice", "aliceuser", "Omairc User", "s3cret")
	if err != nil {
		t.Fatalf("Registration with password failed: %v", err)
	}
	want = "PASS s3cret\r\nNICK alice\r\nUSER aliceuser 8 * :Omairc User\r\n"
	if result != want {
		t.Fatalf("registration = %q, want %q", result, want)
	}
}

func TestRejectsInvalidRegistration(t *testing.T) {
	cases := []struct {
		nickname string
		username string
		realname string
		password string
	}{
		{"", "user", "real", ""},
		{"nick", "", "real", ""},
		{"ni ck", "user", "real", ""},
		{"nick", "us er", "real", ""},
		{"nick", "user", "real", "pass word"},
		{"nick\nnick", "user", "real", ""},
		{"nick", "user", "real", "p\x00ass"},
	}
	for _, testCase := range cases {
		if _, err := Registration(testCase.nickname, testCase.username, testCase.realname, testCase.password); err == nil {
			t.Fatalf("Registration(%q, %q, %q, %q) unexpectedly succeeded",
				testCase.nickname, testCase.username, testCase.realname, testCase.password)
		}
	}
}

func TestRejectsOutboundInjection(t *testing.T) {
	for _, command := range []string{
		"PRIVMSG #c :hi\r\nQUIT",
		"PRIVMSG #c :hi\nQUIT",
		"PRIVMSG #c :hi\rQUIT",
		"PRIVMSG #c :hi\x00there",
	} {
		if _, err := Line(command); err == nil {
			t.Fatalf("Line(%q) unexpectedly succeeded", command)
		}
	}

	result, err := Line("JOIN #chan")
	if err != nil {
		t.Fatalf("Line(JOIN #chan) failed: %v", err)
	}
	if result != "JOIN #chan\r\n" {
		t.Fatalf("line = %q, want %q", result, "JOIN #chan\r\n")
	}
}

func TestEnforcesOutboundBoundary(t *testing.T) {
	result, err := Line(strings.Repeat("A", 510))
	if err != nil {
		t.Fatalf("Line(510) failed: %v", err)
	}
	if len(result) != 512 {
		t.Fatalf("line length = %d, want 512", len(result))
	}

	if _, err := Line(strings.Repeat("A", 511)); err == nil {
		t.Fatal("Line(511) unexpectedly succeeded")
	}
}

func TestSplitsOutboundChatOnWordBoundary(t *testing.T) {
	prefix := "PRIVMSG #omarchy :"
	first := strings.Repeat("a", 400)
	second := strings.Repeat("b", 200)

	chunks := SplitTrailingParam(prefix, first+" "+second, "")
	if len(chunks) != 2 {
		t.Fatalf("chunks = %d, want 2", len(chunks))
	}
	if chunks[0] != first || chunks[1] != second {
		t.Fatalf("chunks = %q, want [first second]", chunks)
	}

	firstLine := prefix + chunks[0]
	secondLine := prefix + chunks[1]
	if firstLine != "PRIVMSG #omarchy :"+strings.Repeat("a", 400) {
		t.Fatalf("first line = %q", firstLine)
	}
	if secondLine != "PRIVMSG #omarchy :"+strings.Repeat("b", 200) {
		t.Fatalf("second line = %q", secondLine)
	}
	if len(firstLine)+2 > MaxClassicFrameBytes {
		t.Fatalf("first line too long: %d", len(firstLine)+2)
	}
	if len(secondLine)+2 > MaxClassicFrameBytes {
		t.Fatalf("second line too long: %d", len(secondLine)+2)
	}
	if _, err := Line(firstLine); err != nil {
		t.Fatalf("first line failed to build: %v", err)
	}
	if _, err := Line(secondLine); err != nil {
		t.Fatalf("second line failed to build: %v", err)
	}
}

func TestSplitsOutboundChatHardWhenTokenExceedsFrame(t *testing.T) {
	prefix := "PRIVMSG #omarchy :"
	maxBody := MaxClassicFrameBytes - len(prefix) - 2
	token := strings.Repeat("x", maxBody*2+16)

	chunks := SplitTrailingParam(prefix, token, "")
	if len(chunks) != 3 {
		t.Fatalf("chunks = %d, want 3", len(chunks))
	}
	if chunks[0] != token[:maxBody] {
		t.Fatalf("chunks[0] = %q, want first maxBody", chunks[0])
	}
	if chunks[1] != token[maxBody:maxBody*2] {
		t.Fatalf("chunks[1] = %q, want second maxBody", chunks[1])
	}
	if chunks[2] != token[maxBody*2:] {
		t.Fatalf("chunks[2] = %q, want remainder", chunks[2])
	}

	for _, chunk := range chunks {
		line := prefix + chunk
		if len(line)+2 > MaxClassicFrameBytes {
			t.Fatalf("chunk line too long: %d", len(line)+2)
		}
		built, err := Line(line)
		if err != nil {
			t.Fatalf("chunk line failed to build: %v", err)
		}
		if len(built) != len(line)+2 {
			t.Fatalf("built length = %d, want %d", len(built), len(line)+2)
		}
	}

	eAcute := "\xC3\xA9"
	torn := strings.Repeat("x", maxBody-1) + eAcute + strings.Repeat("y", 10)
	tornChunks := SplitTrailingParam(prefix, torn, "")
	if len(tornChunks) != 2 {
		t.Fatalf("torn chunks = %d, want 2", len(tornChunks))
	}
	if tornChunks[0] != strings.Repeat("x", maxBody-1) {
		t.Fatalf("tornChunks[0] = %q, want the lead bytes", tornChunks[0])
	}
	if tornChunks[1] != eAcute+strings.Repeat("y", 10) {
		t.Fatalf("tornChunks[1] = %q, want the continuation byte kept with its lead", tornChunks[1])
	}
}

func TestBuildsJoin(t *testing.T) {
	result, err := Join("#a", "")
	if err != nil {
		t.Fatalf("Join(#a) failed: %v", err)
	}
	if result != "JOIN #a\r\n" {
		t.Fatalf("join = %q, want %q", result, "JOIN #a\r\n")
	}

	result, err = Join("#a", "pword")
	if err != nil {
		t.Fatalf("Join(#a, pword) failed: %v", err)
	}
	if result != "JOIN #a pword\r\n" {
		t.Fatalf("join = %q, want %q", result, "JOIN #a pword\r\n")
	}

	result, err = Join("&local", "secret")
	if err != nil {
		t.Fatalf("Join(&local, secret) failed: %v", err)
	}
	if result != "JOIN &local secret\r\n" {
		t.Fatalf("join = %q, want %q", result, "JOIN &local secret\r\n")
	}
}

func TestRejectsInvalidJoin(t *testing.T) {
	for _, channel := range []string{"", "#a b", "#a,b"} {
		if _, err := Join(channel, ""); err == nil {
			t.Fatalf("Join(%q) unexpectedly succeeded", channel)
		}
	}
	for _, key := range []string{"p word", "x,y"} {
		if _, err := Join("#a", key); err == nil {
			t.Fatalf("Join(#a, %q) unexpectedly succeeded", key)
		}
	}

	unkeyedEmpty, err := Join("#a", "")
	if err != nil {
		t.Fatalf("Join(#a, \"\") failed: %v", err)
	}
	if unkeyedEmpty != "JOIN #a\r\n" {
		t.Fatalf("join = %q, want %q", unkeyedEmpty, "JOIN #a\r\n")
	}

	if _, err := Join("#a\r", ""); err == nil {
		t.Fatal("Join with CR unexpectedly succeeded")
	}
	if _, err := Join("#a\n", ""); err == nil {
		t.Fatal("Join with LF unexpectedly succeeded")
	}
	if _, err := Join("#a\x00", ""); err == nil {
		t.Fatal("Join with NUL unexpectedly succeeded")
	}
	if _, err := Join("#a", "k\x00ey"); err == nil {
		t.Fatal("Join with a NUL key unexpectedly succeeded")
	}
	if _, err := Join(":chan", ""); err == nil {
		t.Fatal("Join(:chan) unexpectedly succeeded")
	}
	if _, err := Join("#ok", ":key"); err == nil {
		t.Fatal("Join(#ok, :key) unexpectedly succeeded")
	}

	if _, err := Join(strings.Repeat("A", 508), ""); err == nil {
		t.Fatal("overlong Join unexpectedly succeeded")
	}
}
