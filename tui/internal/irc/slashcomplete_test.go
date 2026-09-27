package irc

import (
	"slices"
	"testing"
)

func TestOpenableNeedle(t *testing.T) {
	cases := []struct {
		text string
		want string
	}{
		{"/j", "j"},
		{"/JOIN", "join"},
		{"/", ""},
		{"//", ""},
		{"///", ""},
		{"/join #x", ""},
		{"/a b", ""},
		{"  /j", "j"},
		{"", ""},
		{"hello", ""},
		{" /", ""},
	}
	for _, test := range cases {
		if got := openableNeedle(test.text); got != test.want {
			t.Errorf("openableNeedle(%q) = %q, want %q", test.text, got, test.want)
		}
	}
}

func TestRankSlashAliasAndOrder(t *testing.T) {
	hits := rankSlash("j", SurfaceConversation)
	if len(hits) != 1 || hits[0].Label != "/join" {
		t.Fatalf("rankSlash(j) = %+v, want [/join]", hits)
	}

	topic := rankSlash("t", SurfaceConversation)
	if len(topic) < 2 || topic[0].Label != "/topic" {
		t.Fatalf("rankSlash(t, conversation) = %+v, want /topic first", topic)
	}
	time := rankSlash("t", SurfaceStatus)
	if len(time) < 1 || time[0].Label != "/time" {
		t.Fatalf("rankSlash(t, status) = %+v, want /time first", time)
	}
}

func TestRankSlashCapsAtSix(t *testing.T) {
	hits := rankSlash("m", SurfaceConversation)
	if len(hits) != 6 {
		t.Fatalf("rankSlash(m) returned %d hits, want 6", len(hits))
	}
	want := []string{"/me", "/msg", "/mode", "/monitor", "/monitored", "/mute"}
	got := make([]string, len(hits))
	for index, hit := range hits {
		got[index] = hit.Label
	}
	if !slices.Equal(got, want) {
		t.Fatalf("rankSlash(m) = %v, want %v", got, want)
	}
	for _, hit := range hits {
		if hit.Label == "/muted" {
			t.Fatal("/muted should be dropped by the six-hit cap")
		}
	}
}

func TestProjectSlashVerb(t *testing.T) {
	probe := ProjectSlash("/jo", SurfaceConversation)
	if !probe.Open || probe.Needle != "jo" {
		t.Fatalf("ProjectSlash(/jo) = %+v", probe)
	}
	if !probe.HasLabel("/join") {
		t.Fatalf("ProjectSlash(/jo) missing /join: %+v", probe.Hits)
	}
}

func TestProjectSlashStatusExcludesMe(t *testing.T) {
	probe := ProjectSlash("/me", SurfaceStatus)
	if !probe.Open {
		t.Fatalf("ProjectSlash(/me, status) closed: %+v", probe)
	}
	if probe.HasLabel("/me") {
		t.Fatal("Status completion should exclude /me")
	}
	conversation := ProjectSlash("/me", SurfaceConversation)
	if !conversation.Open || !conversation.HasLabel("/me") {
		t.Fatalf("conversation completion should include /me: %+v", conversation)
	}
	if conversation.Hits[0].Label != "/me" {
		t.Fatalf("RankSlash(me) first = %q, want /me", conversation.Hits[0].Label)
	}
}

func TestProjectSlashClosedShapes(t *testing.T) {
	for _, text := range []string{"/", "//", "/join #x", "hello", ""} {
		if probe := ProjectSlash(text, SurfaceConversation); probe.Open {
			t.Errorf("ProjectSlash(%q) should be closed: %+v", text, probe)
		}
	}
}

func TestProjectSlashPrefNames(t *testing.T) {
	bare := ProjectSlash("/pref", SurfaceConversation)
	if !bare.Open || len(bare.Hits) != 1 || bare.Hits[0].Label != "/pref" {
		t.Fatalf("bare /pref should complete the verb: %+v", bare)
	}

	probe := ProjectSlash("/pref ", SurfaceConversation)
	if !probe.Open {
		t.Fatalf("/pref should list toggles: %+v", probe)
	}
	want := []string{"/pref directs", "/pref avatars", "/pref unread"}
	got := make([]string, len(probe.Hits))
	for index, hit := range probe.Hits {
		got[index] = hit.Label
	}
	if !slices.Equal(got, want) {
		t.Fatalf("pref name hits = %v, want %v", got, want)
	}
	if probe.Hits[0].Usage != "Reopen direct messages on startup" {
		t.Fatalf("pref usage = %q", probe.Hits[0].Usage)
	}

	one := ProjectSlash("/pref a", SurfaceConversation)
	if !one.Open || len(one.Hits) != 1 || one.Hits[0].Label != "/pref avatars" {
		t.Fatalf("/pref a = %+v, want only /pref avatars", one)
	}

	upper := ProjectSlash("/pref DIRECTS", SurfaceConversation)
	if !upper.Open || len(upper.Hits) != 1 || upper.Hits[0].Label != "/pref directs" {
		t.Fatalf("/pref DIRECTS = %+v", upper)
	}
}

func TestProjectSlashPrefValues(t *testing.T) {
	probe := ProjectSlash("/pref avatars ", SurfaceConversation)
	if !probe.Open {
		t.Fatalf("/pref avatars should list on/off: %+v", probe)
	}
	want := []string{"/pref avatars on", "/pref avatars off"}
	got := make([]string, len(probe.Hits))
	for index, hit := range probe.Hits {
		got[index] = hit.Label
	}
	if !slices.Equal(got, want) {
		t.Fatalf("pref value hits = %v, want %v", got, want)
	}
	if probe.Hits[0].Usage != "/pref avatars on" {
		t.Fatalf("pref value usage = %q", probe.Hits[0].Usage)
	}

	partial := ProjectSlash("/pref avatars o", SurfaceConversation)
	if !partial.Open || len(partial.Hits) != 2 || partial.Hits[0].Label != "/pref avatars on" {
		t.Fatalf("/pref avatars o = %+v", partial)
	}
}

func TestProjectSlashPrefClosedShapes(t *testing.T) {
	for _, text := range []string{
		"/pref avatars x",
		"/pref bogus ",
		"/pref  directs",
		"/pref avatars on x",
	} {
		if probe := ProjectSlash(text, SurfaceConversation); probe.Open {
			t.Errorf("ProjectSlash(%q) should be closed: %+v", text, probe)
		}
	}
}

func TestSlashProbeHasLabel(t *testing.T) {
	probe := SlashProbe{
		Open:   true,
		Needle: "j",
		Hits:   []SlashHit{{Label: "/join", Usage: "/join [channel] [key][, ...]"}},
	}
	if !probe.HasLabel("/join") {
		t.Fatal("HasLabel(/join) = false, want true")
	}
	if probe.HasLabel("/j") {
		t.Fatal("HasLabel(/j) = true, want false")
	}
	if (SlashProbe{}).HasLabel("/join") {
		t.Fatal("closed probe HasLabel = true, want false")
	}
}

func TestOpenSlashProbeRequiresNeedleAndHits(t *testing.T) {
	if openSlashProbe("", []SlashHit{{Label: "/join"}}).Open {
		t.Fatal("open probe with an empty needle should be closed")
	}
	if openSlashProbe("j", nil).Open {
		t.Fatal("open probe with no hits should be closed")
	}
}
