package irc

import (
	"strings"
	"testing"
)

// parseOK parses a line that must succeed.
func parseOK(t *testing.T, line string) Message {
	t.Helper()
	message, err := Parse(line)
	if err != nil {
		t.Fatalf("Parse(%q) failed: %v", line, err)
	}
	return message
}

// errorValue normalizes a Parse error into an Error value, mirroring how the
// C++ suite compares IrcResult::error.
func errorValue(err error) Error {
	if err == nil {
		return ErrorNone
	}
	if value, ok := err.(Error); ok {
		return value
	}
	return Error(-1)
}

func TestParsesTrailingParameters(t *testing.T) {
	message := parseOK(t, ":n!u@h PRIVMSG #c :hello world")
	if len(message.Params) != 2 {
		t.Fatalf("params = %q, want 2", message.Params)
	}
	if message.Params[0] != "#c" {
		t.Fatalf("params[0] = %q, want %q", message.Params[0], "#c")
	}
	if message.Params[1] != "hello world" {
		t.Fatalf("params[1] = %q, want %q", message.Params[1], "hello world")
	}

	message = parseOK(t, ":n!u@h PRIVMSG #c :")
	if len(message.Params) != 2 {
		t.Fatalf("params = %q, want 2", message.Params)
	}
	if message.Params[1] != "" {
		t.Fatalf("trailing param = %q, want empty", message.Params[1])
	}
}

func TestPreservesMiddleParameters(t *testing.T) {
	message := parseOK(t, "PRIVMSG #channel hello")
	if len(message.Params) != 2 {
		t.Fatalf("params = %q, want 2", message.Params)
	}
	if message.Params[0] != "#channel" {
		t.Fatalf("params[0] = %q, want %q", message.Params[0], "#channel")
	}
	if message.Params[1] != "hello" {
		t.Fatalf("params[1] = %q, want %q", message.Params[1], "hello")
	}
}

func TestLimitsParameters(t *testing.T) {
	message := parseOK(t, "TEST 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17")
	if len(message.Params) != 15 {
		t.Fatalf("params = %q, want 15", message.Params)
	}
	if message.Params[13] != "14" {
		t.Fatalf("params[13] = %q, want %q", message.Params[13], "14")
	}
	if message.Params[14] != "15 16 17" {
		t.Fatalf("params[14] = %q, want %q", message.Params[14], "15 16 17")
	}
}

func TestRejectsMalformedMessages(t *testing.T) {
	malformed := []string{
		":nick! PRIVMSG #c :hi",
		":nick@ PRIVMSG #c :hi",
		":@host PRIVMSG #c :hi",
		":nospacePRIVMSG",
		": ",
		"!!!",
		"22",
		"2222",
		"@tag=x",
		"@=value PRIVMSG #c :hi",
		"PR1VMSG #c :hi",
	}
	for _, line := range malformed {
		if _, err := Parse(line); err == nil {
			t.Fatalf("Parse(%q) unexpectedly succeeded", line)
		}
	}
}

func TestClassifiesPrefixes(t *testing.T) {
	message := parseOK(t, ":alice PRIVMSG #room :hello")
	if message.Prefix == nil {
		t.Fatal("prefix is nil, want a server prefix")
	}
	if message.Prefix.Raw != "alice" {
		t.Fatalf("raw = %q, want %q", message.Prefix.Raw, "alice")
	}
	if message.Prefix.Nick != "" {
		t.Fatalf("nick = %q, want empty", message.Prefix.Nick)
	}

	message = parseOK(t, ":irc.example.net PRIVMSG #room :hello")
	if message.Prefix == nil || message.Prefix.Raw != "irc.example.net" {
		t.Fatalf("prefix = %+v, want raw irc.example.net", message.Prefix)
	}
	if message.Prefix.Nick != "" {
		t.Fatalf("nick = %q, want empty", message.Prefix.Nick)
	}

	message = parseOK(t, ":n!u@h PRIVMSG #room :hello")
	if message.Prefix == nil {
		t.Fatal("prefix is nil, want n!u@h")
	}
	if message.Prefix.Nick != "n" || message.Prefix.User != "u" || message.Prefix.Host != "h" {
		t.Fatalf("prefix = %+v, want nick n user u host h", message.Prefix)
	}
}

func TestRecoversPrefixNick(t *testing.T) {
	message := parseOK(t, ":alice PRIVMSG #room :hello")
	if got := PrefixNick(message); got != "alice" {
		t.Fatalf("PrefixNick = %q, want %q", got, "alice")
	}

	message = parseOK(t, ":irc.example.net PRIVMSG #room :hello")
	if got := PrefixNick(message); got != "" {
		t.Fatalf("PrefixNick = %q, want empty for a server name", got)
	}

	message = parseOK(t, ":n!u@h PRIVMSG #room :hello")
	if got := PrefixNick(message); got != "n" {
		t.Fatalf("PrefixNick = %q, want %q", got, "n")
	}
}

func TestParsesTags(t *testing.T) {
	message := parseOK(t,
		"@aaa=hello\\sworld;semi=one\\:two;slash=one\\\\two;"+
			"lines=one\\rtwo\\nthree;flag :n!u@h PRIVMSG #c :tagged")
	if len(message.Tags) != 5 {
		t.Fatalf("tags = %+v, want 5", message.Tags)
	}
	if message.Tags[0].Name != "aaa" {
		t.Fatalf("tags[0].name = %q, want aaa", message.Tags[0].Name)
	}
	assertTagValue(t, message.Tags[0], "hello world")
	assertTagValue(t, message.Tags[1], "one;two")
	assertTagValue(t, message.Tags[2], "one\\two")
	assertTagValue(t, message.Tags[3], "one\rtwo\nthree")
	if message.Tags[4].Value != nil {
		t.Fatalf("tags[4].value = %q, want nil", *message.Tags[4].Value)
	}
	if message.Command != "PRIVMSG" {
		t.Fatalf("command = %q, want PRIVMSG", message.Command)
	}
}

func TestParsesClientOnlyTypingTag(t *testing.T) {
	message := parseOK(t, "@+typing=active :n!u@h TAGMSG #c")
	if len(message.Tags) != 1 {
		t.Fatalf("tags = %+v, want 1", message.Tags)
	}
	if message.Tags[0].Name != "+typing" {
		t.Fatalf("tags[0].name = %q, want +typing", message.Tags[0].Name)
	}
	assertTagValue(t, message.Tags[0], "active")
	if message.Command != "TAGMSG" {
		t.Fatalf("command = %q, want TAGMSG", message.Command)
	}
}

func TestPreservesUtf8(t *testing.T) {
	payload := "PRIVMSG #c :h\xc3\xa9llo \xf0\x9f\xa5\x94"
	message := parseOK(t, payload)
	if len(message.Params) != 2 {
		t.Fatalf("params = %q, want 2", message.Params)
	}
	want := payload[strings.IndexByte(payload, ':')+1:]
	if message.Params[1] != want {
		t.Fatalf("params[1] = %q, want %q", message.Params[1], want)
	}
}

func TestDecodesValidUtf8WireText(t *testing.T) {
	if got := WireText(nil); got != "" {
		t.Fatalf("WireText(nil) = %q, want empty", got)
	}
	if got := WireText([]byte("hello")); got != "hello" {
		t.Fatalf("WireText = %q, want hello", got)
	}
	if got := WireText([]byte("\xc3\xa9")); got != string(rune(0x00E9)) {
		t.Fatalf("WireText = %q, want U+00E9", got)
	}
	if got := WireText([]byte("h\xc3\xa9llo \xf0\x9f\xa5\x94")); got != "h\u00e9llo \U0001F954" {
		t.Fatalf("WireText = %q, want h\\u00e9llo \\U0001F954", got)
	}
}

func TestDecodesInvalidUtf8AsLatin1(t *testing.T) {
	latin1 := WireText([]byte{0xe9})
	if latin1 != string(rune(0x00E9)) {
		t.Fatalf("WireText = %q, want U+00E9", latin1)
	}
	if strings.ContainsRune(latin1, 0xFFFD) {
		t.Fatalf("WireText injected U+FFFD: %q", latin1)
	}

	loneLead := WireText([]byte{0xc3})
	if loneLead != string(rune(0x00C3)) {
		t.Fatalf("WireText = %q, want U+00C3", loneLead)
	}
	if strings.ContainsRune(loneLead, 0xFFFD) {
		t.Fatalf("WireText injected U+FFFD: %q", loneLead)
	}

	cafe := WireText([]byte{'c', 'a', 'f', 0xe9})
	if cafe != "caf"+string(rune(0x00E9)) {
		t.Fatalf("WireText = %q, want caf U+00E9", cafe)
	}
}

func assertTagValue(t *testing.T, tag Tag, want string) {
	t.Helper()
	if tag.Value == nil {
		t.Fatalf("tag %q value is nil, want %q", tag.Name, want)
	}
	if *tag.Value != want {
		t.Fatalf("tag %q value = %q, want %q", tag.Name, *tag.Value, want)
	}
}
