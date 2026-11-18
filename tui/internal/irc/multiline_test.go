package irc

import (
	"strings"
	"testing"
)

// client/src/irc/multiline.rs carries the Rust twins of these tests. Lines are
// parsed from wire text, as the session sees them.

func mlLine(t *testing.T, text string) Message {
	t.Helper()
	message, err := Parse(text)
	if err != nil {
		t.Fatalf("Parse(%q): %v", text, err)
	}
	return message
}

func mlOpen(t *testing.T) Message {
	return mlLine(t, "@msgid=open1;time=2026-01-02T03:04:05.000Z;account=mira :mira!u@h BATCH +ml draft/multiline #omarchy")
}

func mlTag(message Message, name string) (string, bool) {
	for _, tag := range message.Tags {
		if tag.Name == name && tag.Value != nil {
			return *tag.Value, true
		}
	}
	return "", false
}

func TestMultilineLimitsReadBothKeys(t *testing.T) {
	got := ParseMultilineLimits("max-bytes=4096,max-lines=100")
	want := MultilineLimits{MaxBytes: 4096, HasMaxBytes: true, MaxLines: 100, HasMaxLines: true}
	if got != want {
		t.Fatalf("limits = %+v, want %+v", got, want)
	}
}

func TestMultilineLimitsReadOneKey(t *testing.T) {
	got := ParseMultilineLimits("max-lines=7")
	want := MultilineLimits{MaxLines: 7, HasMaxLines: true}
	if got != want {
		t.Fatalf("limits = %+v, want %+v", got, want)
	}
}

func TestMultilineLimitsIgnoreEmptyJunkUnknownAndNonNumeric(t *testing.T) {
	for _, value := range []string{"", "junk", ",,", "max-bytes", "max-bytes=", "max-bytes=abc", "max-bytes=-1", "max-bytes=+1", "MAX-BYTES=5", "other=5"} {
		if got := ParseMultilineLimits(value); got != (MultilineLimits{}) {
			t.Fatalf("ParseMultilineLimits(%q) = %+v, want zero", value, got)
		}
	}
	got := ParseMultilineLimits("other=1,max-bytes=9")
	if got != (MultilineLimits{MaxBytes: 9, HasMaxBytes: true}) {
		t.Fatalf("limits = %+v", got)
	}
}

func TestAssembleMultilineThreeLinesJoinWithTwoLineBreaks(t *testing.T) {
	lines := []Message{
		mlLine(t, "@batch=ml :mira!u@h PRIVMSG #omarchy :one"),
		mlLine(t, "@batch=ml :mira!u@h PRIVMSG #omarchy :two"),
		mlLine(t, "@batch=ml :mira!u@h PRIVMSG #omarchy :three"),
	}
	message, ok := AssembleMultiline(mlOpen(t), lines)
	if !ok {
		t.Fatal("not assembled")
	}
	if message.Command != "PRIVMSG" || message.Params[0] != "#omarchy" || message.Params[1] != "one\ntwo\nthree" {
		t.Fatalf("message = %+v", message)
	}
}

func TestAssembleMultilineConcatJoinsWithNoSeparator(t *testing.T) {
	lines := []Message{
		mlLine(t, "@batch=ml PRIVMSG #omarchy :a"),
		mlLine(t, "@batch=ml;draft/multiline-concat PRIVMSG #omarchy :b"),
		mlLine(t, "@batch=ml PRIVMSG #omarchy :c"),
	}
	message, _ := AssembleMultiline(mlOpen(t), lines)
	if message.Params[1] != "ab\nc" {
		t.Fatalf("body = %q", message.Params[1])
	}
}

func TestAssembleMultilineConcatTagOnFirstLineIsIgnored(t *testing.T) {
	lines := []Message{
		mlLine(t, "@batch=ml;draft/multiline-concat PRIVMSG #omarchy :a"),
		mlLine(t, "@batch=ml PRIVMSG #omarchy :b"),
	}
	message, _ := AssembleMultiline(mlOpen(t), lines)
	if message.Params[1] != "a\nb" {
		t.Fatalf("body = %q", message.Params[1])
	}
}

func TestAssembleMultilineBatchLineSuppliesTagsAndLinesDoNot(t *testing.T) {
	lines := []Message{
		mlLine(t, "@batch=ml;msgid=line1;time=1999-01-01T00:00:00.000Z :other!u@h PRIVMSG #omarchy :one"),
		mlLine(t, "@batch=ml;msgid=line2 PRIVMSG #omarchy :two"),
	}
	message, _ := AssembleMultiline(mlOpen(t), lines)
	if got, _ := mlTag(message, "msgid"); got != "open1" {
		t.Fatalf("msgid = %q", got)
	}
	if got, _ := mlTag(message, "time"); got != "2026-01-02T03:04:05.000Z" {
		t.Fatalf("time = %q", got)
	}
	if got, _ := mlTag(message, "account"); got != "mira" {
		t.Fatalf("account = %q", got)
	}
	if _, found := mlTag(message, "batch"); found {
		t.Fatal("batch tag kept")
	}
	if message.Prefix == nil || message.Prefix.Raw != "mira!u@h" {
		t.Fatalf("prefix = %+v", message.Prefix)
	}
}

func TestAssembleMultilinePrefixFallsBackToFirstLine(t *testing.T) {
	open := mlLine(t, "@msgid=open1 BATCH +ml draft/multiline #omarchy")
	lines := []Message{mlLine(t, "@batch=ml :bob!b@h PRIVMSG #omarchy :hi")}
	message, _ := AssembleMultiline(open, lines)
	if message.Prefix == nil || message.Prefix.Raw != "bob!b@h" {
		t.Fatalf("prefix = %+v", message.Prefix)
	}
}

func TestAssembleMultilineTargetFallsBackToFirstLine(t *testing.T) {
	open := mlLine(t, "BATCH +ml draft/multiline")
	lines := []Message{mlLine(t, "@batch=ml PRIVMSG #omarchy :hi")}
	message, _ := AssembleMultiline(open, lines)
	if message.Params[0] != "#omarchy" {
		t.Fatalf("target = %q", message.Params[0])
	}
}

func TestAssembleMultilineNoticeStaysANotice(t *testing.T) {
	lines := []Message{
		mlLine(t, "@batch=ml NOTICE #omarchy :one"),
		mlLine(t, "@batch=ml NOTICE #omarchy :two"),
	}
	message, _ := AssembleMultiline(mlOpen(t), lines)
	if message.Command != "NOTICE" || message.Params[1] != "one\ntwo" {
		t.Fatalf("message = %+v", message)
	}
}

func TestAssembleMultilineKeepsABlankLineBetweenText(t *testing.T) {
	lines := []Message{
		mlLine(t, "@batch=ml PRIVMSG #omarchy :one"),
		mlLine(t, "@batch=ml PRIVMSG #omarchy :"),
		mlLine(t, "@batch=ml PRIVMSG #omarchy :three"),
	}
	message, _ := AssembleMultiline(mlOpen(t), lines)
	if message.Params[1] != "one\n\nthree" {
		t.Fatalf("body = %q", message.Params[1])
	}
}

func TestAssembleMultilineOnlyBlankLinesAssembleToNothing(t *testing.T) {
	lines := []Message{
		mlLine(t, "@batch=ml PRIVMSG #omarchy :"),
		mlLine(t, "@batch=ml PRIVMSG #omarchy :  "),
	}
	if _, ok := AssembleMultiline(mlOpen(t), lines); ok {
		t.Fatal("blank lines assembled")
	}
	if _, ok := AssembleMultiline(mlOpen(t), nil); ok {
		t.Fatal("no lines assembled")
	}
}

func TestAssembleMultilineCtcpLineRefusesTheWholeBatch(t *testing.T) {
	lines := []Message{
		mlLine(t, "@batch=ml PRIVMSG #omarchy :one"),
		mlLine(t, "@batch=ml PRIVMSG #omarchy :\x01ACTION waves\x01"),
	}
	if _, ok := AssembleMultiline(mlOpen(t), lines); ok {
		t.Fatal("CTCP batch assembled")
	}
}

func TestAssembleMultilineDecodesLineByLine(t *testing.T) {
	latin := mlLine(t, "@batch=ml PRIVMSG #omarchy :x")
	latin.Params[1] = "caf\xe9"
	lines := []Message{mlLine(t, "@batch=ml PRIVMSG #omarchy :naïve \U0001F954"), latin}
	message, _ := AssembleMultiline(mlOpen(t), lines)
	if want := "naïve \U0001F954\ncafé"; message.Params[1] != want {
		t.Fatalf("body = %q, want %q", message.Params[1], want)
	}
}

func TestAssembleMultilineSkipsAnotherCommandOrTarget(t *testing.T) {
	lines := []Message{
		mlLine(t, "@batch=ml PRIVMSG #omarchy :one"),
		mlLine(t, "@batch=ml NOTICE #omarchy :notice"),
		mlLine(t, "@batch=ml PRIVMSG #other :elsewhere"),
		mlLine(t, "@batch=ml PRIVMSG #OMARCHY :two"),
		mlLine(t, "@batch=ml TOPIC #omarchy :nope"),
	}
	message, _ := AssembleMultiline(mlOpen(t), lines)
	if message.Params[1] != "one\ntwo" {
		t.Fatalf("body = %q", message.Params[1])
	}
	if strings.Contains(message.Params[1], "\r") {
		t.Fatal("carriage return in body")
	}
}

func TestMultilineNegotiationNeedsBatchAndRidesPresenceLine(t *testing.T) {
	withoutBatch := NewCapabilityNegotiation(false)
	withoutBatch.Advertise(tokens("draft/multiline=max-bytes=4096,max-lines=100 multi-prefix"))
	assertCapLines(t, withoutBatch.TakeRequest().Lines, "multi-prefix")

	negotiation := NewCapabilityNegotiation(false)
	negotiation.Advertise(tokens("draft/multiline=max-bytes=4096,max-lines=100 batch"))
	assertCapLines(t, negotiation.TakeRequest().Lines, "batch draft/multiline")
	granted := negotiation.Acknowledge(tokens("batch draft/multiline"))
	if !granted.Contains(CapabilityMultiline) || !negotiation.Enabled().Contains(CapabilityMultiline) {
		t.Fatal("multiline not enabled")
	}
}

func TestMultilineNegotiationLimitsStoredThenClearedByCapDelAndReset(t *testing.T) {
	negotiation := NewCapabilityNegotiation(false)
	if negotiation.MultilineLimits() != (MultilineLimits{}) {
		t.Fatal("limits not empty at start")
	}
	negotiation.Advertise(tokens("batch draft/multiline=max-bytes=4096,max-lines=100"))
	want := MultilineLimits{MaxBytes: 4096, HasMaxBytes: true, MaxLines: 100, HasMaxLines: true}
	if negotiation.MultilineLimits() != want {
		t.Fatalf("limits = %+v", negotiation.MultilineLimits())
	}

	negotiation.Withdraw(tokens("draft/multiline"))
	if negotiation.MultilineLimits() != (MultilineLimits{}) {
		t.Fatal("limits kept after CAP DEL")
	}

	negotiation.Advertise(tokens("draft/multiline=max-lines=5"))
	if got := negotiation.MultilineLimits(); !got.HasMaxLines || got.MaxLines != 5 {
		t.Fatalf("limits = %+v", got)
	}
	negotiation.Reset(false)
	if negotiation.MultilineLimits() != (MultilineLimits{}) {
		t.Fatal("limits kept after reset")
	}
}

func TestMultilineBareTokenIsRequestedWithNoLimits(t *testing.T) {
	negotiation := NewCapabilityNegotiation(false)
	negotiation.Advertise(tokens("batch draft/multiline"))
	assertCapLines(t, negotiation.TakeRequest().Lines, "batch draft/multiline")
	if negotiation.MultilineLimits() != (MultilineLimits{}) {
		t.Fatal("limits set for a bare token")
	}
}
