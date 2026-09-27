package connection

import (
	"slices"
	"testing"
)

// validProfile is a complete profile the validation matrix mutates one field
// at a time.
func validProfile() NetworkProfile {
	profile := CreateProfile()
	profile.Name = "libera"
	profile.Host = "irc.libera.chat"
	profile.Port = 6697
	profile.TLSEnabled = true
	profile.Nick = "omairc"
	return profile
}

func TestValidateMatrix(t *testing.T) {
	tests := []struct {
		name   string
		mutate func(*NetworkProfile)
		want   Problem
	}{
		{"complete", func(*NetworkProfile) {}, ProblemNone},
		{"missing host", func(p *NetworkProfile) { p.Host = "" }, ProblemMissingHost},
		{"missing host after trim", func(p *NetworkProfile) { p.Host = "   " }, ProblemMissingHost},
		{"invalid port", func(p *NetworkProfile) { p.Port = 0 }, ProblemInvalidPort},
		{"missing nick", func(p *NetworkProfile) { p.Nick = "" }, ProblemMissingNick},
		{"unsendable nick", func(p *NetworkProfile) { p.Nick = "bad nick" }, ProblemUnsendableIdentity},
		{"unsendable username", func(p *NetworkProfile) { p.Username = "bad user" }, ProblemUnsendableIdentity},
		{"unsendable realname", func(p *NetworkProfile) { p.Realname = "bad\nname" }, ProblemUnsendableIdentity},
		{"unsendable account", func(p *NetworkProfile) { p.Account = "bad account" }, ProblemUnsendableAccount},
		{"account slash", func(p *NetworkProfile) { p.Account = "bad/account" }, ProblemUnsendableAccount},
		{"unsendable bouncer", func(p *NetworkProfile) { p.BouncerNetwork = "bad network" }, ProblemUnsendableBouncerNetwork},
		{"bouncer slash", func(p *NetworkProfile) { p.BouncerNetwork = "bad/network" }, ProblemUnsendableBouncerNetwork},
		{"unsendable channel", func(p *NetworkProfile) { p.AutojoinChannels = []string{"#bad\x00chan"} }, ProblemUnsendableChannel},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			profile := validProfile()
			test.mutate(&profile)
			if got := profile.Validate(); got != test.want {
				t.Fatalf("Validate() = %v, want %v", got, test.want)
			}
			if got := profile.IsComplete(); got != (test.want == ProblemNone) {
				t.Fatalf("IsComplete() = %v, want %v", got, test.want == ProblemNone)
			}
		})
	}
}

func TestProblemText(t *testing.T) {
	want := map[Problem]string{
		ProblemNone:                     "",
		ProblemMissingHost:              "Host is required",
		ProblemMissingNick:              "Nick is required",
		ProblemInvalidPort:              "Port is required",
		ProblemUnsendableIdentity:       "Nick, username, or real name cannot be sent",
		ProblemUnsendableAccount:        "Account cannot contain a space or a slash",
		ProblemUnsendableBouncerNetwork: "Bouncer network cannot contain a space or a slash",
		ProblemUnsendableChannel:        "An autojoin channel cannot be sent",
	}
	for problem, text := range want {
		if got := ProblemText(problem); got != text {
			t.Fatalf("ProblemText(%v) = %q, want %q", problem, got, text)
		}
	}
}

func TestValidateOrderPrefersHostThenPortThenNick(t *testing.T) {
	profile := NetworkProfile{Host: "", Port: 0, Nick: ""}
	if got := profile.Validate(); got != ProblemMissingHost {
		t.Fatalf("Validate() = %v, want ProblemMissingHost", got)
	}
	profile.Host = "irc.example"
	if got := profile.Validate(); got != ProblemInvalidPort {
		t.Fatalf("Validate() = %v, want ProblemInvalidPort", got)
	}
	profile.Port = 6697
	if got := profile.Validate(); got != ProblemMissingNick {
		t.Fatalf("Validate() = %v, want ProblemMissingNick", got)
	}
}

func TestParseAutojoin(t *testing.T) {
	tests := []struct {
		name  string
		input string
		want  []string
	}{
		{"empty", "", nil},
		{"commas and whitespace", " #one , #two ", []string{"#one", "#two"}},
		{"bare tokens kept verbatim", "omarchy general", []string{"omarchy", "general"}},
		{"key after channel dropped", "#one secret", []string{"#one"}},
		{"key then channel", "secret #one", []string{"secret", "#one"}},
		{"several keys dropped", "#one k1 k2 #two k3", []string{"#one", "#two"}},
		{"whitespace runs", "  #a\t\n#b  ", []string{"#a", "#b"}},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			if got := ParseAutojoin(test.input); !slices.Equal(got, test.want) {
				t.Fatalf("ParseAutojoin(%q) = %v, want %v", test.input, got, test.want)
			}
		})
	}
}

func TestNormalizedTrimsPrefixesAndRetainsKeys(t *testing.T) {
	profile := NetworkProfile{
		Name:             "  libera  ",
		Host:             " irc.libera.chat ",
		Nick:             " omairc ",
		Username:         " user ",
		Realname:         " Real Name ",
		Account:          " acct ",
		BouncerNetwork:   " net ",
		AutojoinChannels: []string{"omarchy", "#two"},
		AutojoinKeys:     map[string]string{"#OMARCHY": "one-key", "#three": "gone"},
	}
	got := profile.Normalized()
	if got.Name != "libera" || got.Host != "irc.libera.chat" || got.Nick != "omairc" {
		t.Fatalf("trimmed fields = %+v", got)
	}
	if got.Username != "user" || got.Realname != "Real Name" || got.Account != "acct" || got.BouncerNetwork != "net" {
		t.Fatalf("trimmed fields = %+v", got)
	}
	if want := []string{"#omarchy", "#two"}; !slices.Equal(got.AutojoinChannels, want) {
		t.Fatalf("AutojoinChannels = %v, want %v", got.AutojoinChannels, want)
	}
	// The key is re-keyed to the canonical channel name and the stale key
	// (#three) is dropped.
	if len(got.AutojoinKeys) != 1 || got.AutojoinKeys["#omarchy"] != "one-key" {
		t.Fatalf("AutojoinKeys = %v, want {#omarchy: one-key}", got.AutojoinKeys)
	}
}

func TestNormalizedDropsKeyAfterChannel(t *testing.T) {
	profile := NetworkProfile{AutojoinChannels: []string{"#one", "secret"}}
	got := profile.Normalized()
	if want := []string{"#one"}; !slices.Equal(got.AutojoinChannels, want) {
		t.Fatalf("AutojoinChannels = %v, want %v", got.AutojoinChannels, want)
	}
	if len(got.AutojoinKeys) != 0 {
		t.Fatalf("AutojoinKeys = %v, want empty", got.AutojoinKeys)
	}
}

func TestSuggestedProfileDefaults(t *testing.T) {
	profile := SuggestedProfile()
	if profile.NetworkID == "" {
		t.Fatal("NetworkID is empty")
	}
	if profile.Host != "irc.libera.chat" {
		t.Fatalf("Host = %q", profile.Host)
	}
	if profile.Name != profile.Host {
		t.Fatalf("Name = %q, want %q", profile.Name, profile.Host)
	}
	if profile.Port != 6697 {
		t.Fatalf("Port = %d, want 6697", profile.Port)
	}
	if !profile.TLSEnabled {
		t.Fatal("TLSEnabled = false, want true")
	}
	if want := []string{"#omarchy"}; !slices.Equal(profile.AutojoinChannels, want) {
		t.Fatalf("AutojoinChannels = %v, want %v", profile.AutojoinChannels, want)
	}
}

func TestCreateProfileNetworkIDIsUnique(t *testing.T) {
	first := CreateProfile()
	second := CreateProfile()
	if first.NetworkID == "" || second.NetworkID == "" {
		t.Fatal("empty network id")
	}
	if first.NetworkID == second.NetworkID {
		t.Fatalf("duplicate network id %q", first.NetworkID)
	}
	if len(first.NetworkID) != 11 {
		t.Fatalf("network id %q length = %d, want 11 (unpadded base64 of 8 bytes)", first.NetworkID, len(first.NetworkID))
	}
	if first.IconColor != NoIconColor {
		t.Fatalf("IconColor = %d, want %d", first.IconColor, NoIconColor)
	}
}

func TestSASLAccount(t *testing.T) {
	tests := []struct {
		name    string
		profile NetworkProfile
		want    string
	}{
		{"falls back to nick", NetworkProfile{Nick: "omairc"}, "omairc"},
		{"prefers account", NetworkProfile{Nick: "omairc", Account: "acct"}, "acct"},
		{"bouncer with account", NetworkProfile{Nick: "omairc", Account: "acct", BouncerNetwork: "libera"}, "acct/libera"},
		{"bouncer with nick", NetworkProfile{Nick: "omairc", BouncerNetwork: "libera"}, "omairc/libera"},
		{"trims", NetworkProfile{Nick: " omairc ", Account: " acct ", BouncerNetwork: " net "}, "acct/net"},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			if got := test.profile.SASLAccount(); got != test.want {
				t.Fatalf("SASLAccount() = %q, want %q", got, test.want)
			}
		})
	}
}

func TestResolvedName(t *testing.T) {
	if got := ResolvedName("  libera ", "irc.example"); got != "libera" {
		t.Fatalf("ResolvedName = %q, want libera", got)
	}
	if got := ResolvedName("   ", " irc.example "); got != "irc.example" {
		t.Fatalf("ResolvedName = %q, want irc.example", got)
	}
	profile := NetworkProfile{Name: " ", Host: "irc.example"}
	if got := profile.ResolvedName(); got != "irc.example" {
		t.Fatalf("ResolvedName() = %q, want irc.example", got)
	}
}

func TestPickAndEnsureIconColor(t *testing.T) {
	if got := PickIconColor([]int{0, 2, 4, 1}); got != 3 {
		t.Fatalf("PickIconColor = %d, want the only free slot 3", got)
	}
	if got := PickIconColor([]int{0, 1, 2, 3, 4}); got < 0 || got >= IconColorCount {
		t.Fatalf("PickIconColor = %d, want a slot in [0,%d)", got, IconColorCount)
	}

	profile := NetworkProfile{IconColor: NoIconColor}
	if !profile.EnsureIconColor([]int{0, 1, 2, 3}) {
		t.Fatal("EnsureIconColor = false, want true when unassigned")
	}
	if profile.IconColor != 4 {
		t.Fatalf("IconColor = %d, want 4", profile.IconColor)
	}
	if profile.EnsureIconColor(nil) {
		t.Fatal("EnsureIconColor = true, want false when already assigned")
	}
}
