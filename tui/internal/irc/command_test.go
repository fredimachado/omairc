package irc

import (
	"slices"
	"testing"
)

// This file mirrors the C++ tst_command matrix in tests/session for the
// portable slash-command core.

func TestVerbTableShape(t *testing.T) {
	table := VerbTable()
	if len(table) != 47 {
		t.Fatalf("verb table has %d rows, want 47", len(table))
	}

	first := table[0]
	if first.Verb != VerbAction || first.Name != "me" || first.Usage != "/me <text>" ||
		first.Scope != ScopeConversation || first.WrongScopeText != "" {
		t.Fatalf("unexpected first row: %+v", first)
	}
	if len(first.Aliases) != 0 {
		t.Fatalf("Action aliases = %v, want none", first.Aliases)
	}

	join := table[1]
	if join.Verb != VerbJoin || join.Name != "join" ||
		join.Usage != "/join [channel] [key][, ...]" || join.Scope != ScopeEither {
		t.Fatalf("unexpected join row: %+v", join)
	}
	if !slices.Equal(join.Aliases, []string{"j"}) {
		t.Fatalf("join aliases = %v, want [j]", join.Aliases)
	}

	part := table[2]
	if part.Scope != ScopeEither || part.WrongScopeText != "Part applies to channels" {
		t.Fatalf("unexpected part row: %+v", part)
	}

	topic := table[9]
	if topic.Verb != VerbTopic || topic.Scope != ScopeConversation ||
		topic.WrongScopeText != "Topic applies to channels" {
		t.Fatalf("unexpected topic row: %+v", topic)
	}

	last := table[len(table)-1]
	if last.Verb != VerbList || last.Name != "list" || last.Usage != "/list [mask]" {
		t.Fatalf("unexpected last row: %+v", last)
	}
}

func TestVerbTableReturnsCopy(t *testing.T) {
	table := VerbTable()
	table[0].Name = "mutated"
	if again := VerbTable(); again[0].Name != "me" {
		t.Fatalf("VerbTable leaked a mutation: %q", again[0].Name)
	}
}

func TestLookupVerbCaseAndAliases(t *testing.T) {
	if spec := LookupVerb("J"); spec == nil || spec.Verb != VerbJoin {
		t.Fatalf("LookupVerb(J) = %+v, want join", spec)
	}
	if spec := LookupVerb("leave"); spec == nil || spec.Verb != VerbPart {
		t.Fatalf("LookupVerb(leave) = %+v, want part", spec)
	}
	if spec := LookupVerb("QUIT"); spec == nil || spec.Verb != VerbQuit {
		t.Fatalf("LookupVerb(QUIT) = %+v, want disconnect", spec)
	}
	if spec := LookupVerb("quote"); spec == nil || spec.Verb != VerbRaw {
		t.Fatalf("LookupVerb(quote) = %+v, want raw", spec)
	}
	if spec := LookupVerb("nope"); spec != nil {
		t.Fatalf("LookupVerb(nope) = %+v, want nil", spec)
	}
}

func TestFindVerb(t *testing.T) {
	for _, verb := range []Verb{VerbEmpty, VerbSay, VerbUnknown} {
		if spec := FindVerb(verb); spec != nil {
			t.Fatalf("FindVerb(%v) = %+v, want nil", verb, spec)
		}
	}
	if spec := FindVerb(VerbJoin); spec == nil || spec.Name != "join" {
		t.Fatalf("FindVerb(join) = %+v, want join", spec)
	}
}

func TestParseCommandTable(t *testing.T) {
	cases := []struct {
		input string
		want  Command
	}{
		{"", Command{Verb: VerbEmpty}},
		{"hello world", Command{Verb: VerbSay, Argument: "hello world"}},
		{"  spaced  ", Command{Verb: VerbSay, Argument: "spaced"}},
		{"//away", Command{Verb: VerbSay, Argument: "/away"}},
		{"///x", Command{Verb: VerbSay, Argument: "//x"}},
		{"/j #help", Command{Verb: VerbJoin, Name: "/j", Argument: "#help"}},
		{"/join", Command{Verb: VerbJoin, Name: "/join"}},
		{"/JOIN  #help  ", Command{Verb: VerbJoin, Name: "/JOIN", Argument: "#help"}},
		{"/back extra", Command{Verb: VerbBack, Name: "/back"}},
		{"/bogus", Command{Verb: VerbUnknown, Name: "/bogus"}},
		{"//", Command{Verb: VerbSay, Argument: "/"}},
	}
	for _, test := range cases {
		got := ParseCommand(test.input)
		if got.Verb != test.want.Verb || got.Name != test.want.Name ||
			got.Argument != test.want.Argument {
			t.Errorf("ParseCommand(%q) = %+v, want %+v", test.input, got, test.want)
		}
	}
}

func TestCommandIsLiveMessage(t *testing.T) {
	if !(Command{Verb: VerbSay}).IsLiveMessage() {
		t.Error("Say should be a live message")
	}
	if !(Command{Verb: VerbAction}).IsLiveMessage() {
		t.Error("Action should be a live message")
	}
	if (Command{Verb: VerbJoin}).IsLiveMessage() {
		t.Error("Join should not be a live message")
	}
}

func TestCommandAllowedOn(t *testing.T) {
	if !(Command{Verb: VerbSay}).AllowedOn(SurfaceConversation) {
		t.Error("Say should be allowed on the conversation surface")
	}
	if (Command{Verb: VerbSay}).AllowedOn(SurfaceStatus) {
		t.Error("Say should not be allowed on the Status surface")
	}
	// Join is Either.
	if !(Command{Verb: VerbJoin}).AllowedOn(SurfaceStatus) {
		t.Error("Join should be allowed on the Status surface")
	}
	// /me (Action) is conversation-only.
	me := ParseCommand("/me waves")
	if me.AllowedOn(SurfaceStatus) {
		t.Error("/me should not be allowed on the Status surface")
	}
	if !me.AllowedOn(SurfaceConversation) {
		t.Error("/me should be allowed on the conversation surface")
	}
}

func TestVerbSpecAllowedOn(t *testing.T) {
	action := LookupVerb("me")
	if action == nil || !action.AllowedOn(SurfaceConversation) || action.AllowedOn(SurfaceStatus) {
		t.Fatalf("Action scope wrong: %+v", action)
	}
	join := LookupVerb("join")
	if join == nil || !join.AllowedOn(SurfaceConversation) || !join.AllowedOn(SurfaceStatus) {
		t.Fatalf("Join scope wrong: %+v", join)
	}
}

func TestVerbSpecWrongScopeMessage(t *testing.T) {
	if got := (VerbSpec{Scope: ScopeEither}).WrongScopeMessage(); got != "Select a connected conversation first" {
		t.Fatalf("empty wrong-scope text = %q", got)
	}
	if got := (VerbSpec{WrongScopeText: "Topic applies to channels"}).WrongScopeMessage(); got != "Topic applies to channels" {
		t.Fatalf("wrong-scope text = %q", got)
	}
}

func TestVisibleVerbsOnSurface(t *testing.T) {
	conversation := VisibleVerbsOn(SurfaceConversation)
	status := VisibleVerbsOn(SurfaceStatus)
	if len(conversation) != 47 {
		t.Fatalf("conversation visible verbs = %d, want 47", len(conversation))
	}
	// Status drops the conversation-only verbs: me, close, topic, op, deop,
	// voice, devoice, ban.
	if len(status) != 39 {
		t.Fatalf("status visible verbs = %d, want 39", len(status))
	}
	for _, spec := range status {
		if spec.Scope == ScopeConversation {
			t.Fatalf("status visible verbs include conversation-only %q", spec.Name)
		}
	}
}

func TestCommandOutcomeText(t *testing.T) {
	cases := []struct {
		outcome CommandOutcome
		command Command
		want    string
	}{
		{OutcomeSent, Command{}, ""},
		{OutcomeNotConnected, Command{}, "Not connected"},
		{OutcomeRefused, Command{}, "Command was refused"},
		{OutcomeUnsupported, Command{}, "That command is not supported"},
		{OutcomeUnsupported, Command{Name: "/bogus"}, "Unknown command: /bogus"},
		{OutcomeWrongScope, Command{Verb: VerbBack}, "Select a connected conversation first"},
		{OutcomeWrongScope, Command{Verb: VerbTopic}, "Topic applies to channels"},
		{OutcomeWrongScope, Command{Verb: VerbUnknown}, "Select a connected conversation first"},
	}
	for _, test := range cases {
		if got := CommandOutcomeText(test.outcome, test.command); got != test.want {
			t.Errorf("CommandOutcomeText(%v, %+v) = %q, want %q",
				test.outcome, test.command, got, test.want)
		}
	}
}
