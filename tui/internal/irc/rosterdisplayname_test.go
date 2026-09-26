package irc

import "testing"

func TestRosterDisplayName(t *testing.T) {
	tests := []struct {
		name        string
		label       string
		nick        string
		otherLabels []string
		want        string
	}{
		{
			name:  "no clash keeps the label",
			label: "irc.example",
			nick:  "fred",
			want:  "irc.example",
		},
		{
			name:        "clash appends the nick",
			label:       "irc.example",
			nick:        "fred",
			otherLabels: []string{"irc.example"},
			want:        "irc.example · fred",
		},
		{
			name:        "clash with empty nick keeps the label",
			label:       "irc.example",
			nick:        "",
			otherLabels: []string{"irc.example"},
			want:        "irc.example",
		},
		{
			name:        "clash with whitespace nick keeps the label",
			label:       "irc.example",
			nick:        "   ",
			otherLabels: []string{"irc.example"},
			want:        "irc.example",
		},
		{
			name:  "empty label is a new network",
			label: "",
			nick:  "fred",
			want:  "New network",
		},
		{
			name:        "clash is case-insensitive",
			label:       "irc.example",
			nick:        "fred",
			otherLabels: []string{"Irc.Example"},
			want:        "irc.example · fred",
		},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			got := RosterDisplayName(test.label, test.nick, test.otherLabels)
			if got != test.want {
				t.Fatalf("RosterDisplayName(%q, %q, %v) = %q, want %q",
					test.label, test.nick, test.otherLabels, got, test.want)
			}
		})
	}
}
