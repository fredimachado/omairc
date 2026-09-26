package irc

import "testing"

func TestPrefCatalog(t *testing.T) {
	catalog := PrefCatalog()
	if len(catalog) != 3 {
		t.Fatalf("pref catalog has %d rows, want 3", len(catalog))
	}
	want := []PrefSpec{
		{PrefDirects, "directs", "Reopen direct messages on startup"},
		{PrefAvatars, "avatars", "Show peer avatars"},
		{PrefUnread, "unread", "Open conversations at unread"},
	}
	for index, spec := range catalog {
		if spec != want[index] {
			t.Errorf("catalog[%d] = %+v, want %+v", index, spec, want[index])
		}
	}
}

func TestPrefFind(t *testing.T) {
	if spec := PrefFind("DIRECTS"); spec == nil || spec.Name != PrefDirects {
		t.Fatalf("PrefFind(DIRECTS) = %+v, want directs", spec)
	}
	if spec := PrefFind("avatars"); spec == nil || spec.Name != PrefAvatars {
		t.Fatalf("PrefFind(avatars) = %+v, want avatars", spec)
	}
	if spec := PrefFind("nope"); spec != nil {
		t.Fatalf("PrefFind(nope) = %+v, want nil", spec)
	}
}

func TestPrefStrings(t *testing.T) {
	if got := PrefUsage(); got != "/pref [directs|avatars|unread] [on|off]" {
		t.Errorf("PrefUsage() = %q", got)
	}
	if got := PrefAvatarNote(); got != "Turn off to keep avatar hosts from seeing your IP on busy channels." {
		t.Errorf("PrefAvatarNote() = %q", got)
	}
}

func TestFormatPrefState(t *testing.T) {
	if got := FormatPrefState(PrefDirects, true); got != "Reopen direct messages on startup: on" {
		t.Errorf("FormatPrefState(directs, true) = %q", got)
	}
	if got := FormatPrefState(PrefAvatars, false); got != "Show peer avatars: off" {
		t.Errorf("FormatPrefState(avatars, false) = %q", got)
	}
}

func TestFormatPrefQuery(t *testing.T) {
	if got := FormatPrefQuery(PrefDirects, true); got != "Reopen direct messages on startup: on" {
		t.Errorf("FormatPrefQuery(directs, true) = %q", got)
	}
	want := "Show peer avatars: on\n" + PrefAvatarNote()
	if got := FormatPrefQuery(PrefAvatars, true); got != want {
		t.Errorf("FormatPrefQuery(avatars, true) = %q, want %q", got, want)
	}
}

func TestFormatPrefList(t *testing.T) {
	want := "Reopen direct messages on startup: on\n" +
		"Show peer avatars: off\n" +
		"Open conversations at unread: on"
	if got := FormatPrefList(true, false, true); got != want {
		t.Errorf("FormatPrefList = %q, want %q", got, want)
	}
}

func TestParsePrefArgument(t *testing.T) {
	cases := []struct {
		argument string
		want     PrefRequest
	}{
		{"", PrefRequest{Kind: PrefQueryAll}},
		{"   ", PrefRequest{Kind: PrefQueryAll}},
		{"directs", PrefRequest{Kind: PrefQueryOne, Name: PrefDirects}},
		{"AVATARS", PrefRequest{Kind: PrefQueryOne, Name: PrefAvatars}},
		{"avatars on", PrefRequest{Kind: PrefSet, Name: PrefAvatars, Enabled: true}},
		{"unread OFF", PrefRequest{Kind: PrefSet, Name: PrefUnread, Enabled: false}},
		{"directs maybe", PrefRequest{Kind: PrefUsageKind, Name: PrefDirects}},
		{"directs on extra", PrefRequest{Kind: PrefUsageKind}},
		{"bogus", PrefRequest{Kind: PrefUsageKind}},
		{"bogus on", PrefRequest{Kind: PrefUsageKind}},
	}
	for _, test := range cases {
		if got := ParsePrefArgument(test.argument); got != test.want {
			t.Errorf("ParsePrefArgument(%q) = %+v, want %+v", test.argument, got, test.want)
		}
	}
}
