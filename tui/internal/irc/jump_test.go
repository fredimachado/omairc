package irc

import "testing"

func TestJumpScoreRanksNameAboveDetail(t *testing.T) {
	if got := JumpScore("alice", "#desktop", "Alice wrote this"); got != 1 {
		t.Fatalf("topic-only score = %d, want 1", got)
	}
	if got := JumpScore("desk", "#desktop", "Alice wrote this"); got != 2 {
		t.Fatalf("name-only score = %d, want 2", got)
	}
	if got := JumpScore("desk", "#desktop", "desktop notes"); got != 3 {
		t.Fatalf("name and topic score = %d, want 3", got)
	}
	if got := JumpScore("", "#desktop", "topic"); got != 0 {
		t.Fatalf("empty query score = %d, want 0", got)
	}
	if got := JumpScore("zzz", "#desktop", "topic"); got != 0 {
		t.Fatalf("miss score = %d, want 0", got)
	}
	if JumpResultLimit != 20 {
		t.Fatalf("limit = %d, want 20", JumpResultLimit)
	}
}

func TestMeaningfulRealnameSkipsPlaceholders(t *testing.T) {
	if MeaningfulRealname("Alice Example", "Alice") != true {
		t.Fatal("a distinct real name must match")
	}
	for _, realname := range []string{"", "Alice", "unknown", "Realname", "FULLNAME"} {
		if MeaningfulRealname(realname, "Alice") {
			t.Fatalf("%q must not match as a real name", realname)
		}
	}
}

func TestWhoReplyRealname(t *testing.T) {
	cases := []struct {
		in, want string
	}{
		{"0 Alice Example", "Alice Example"},
		{"2", ""},
		{"Alice Example", "Alice Example"},
		{"", ""},
	}
	for _, testCase := range cases {
		if got := whoReplyRealname(testCase.in); got != testCase.want {
			t.Fatalf("whoReplyRealname(%q) = %q, want %q", testCase.in, got, testCase.want)
		}
	}
}
