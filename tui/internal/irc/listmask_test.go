package irc

import "testing"

func TestExpandListMask(t *testing.T) {
	cases := []struct {
		in, want string
	}{
		{"bob", "bob!*@*"},
		{"bob!*@*", "bob!*@*"},
		{"bob!u@h", "bob!u@h"},
		{"*@evil.example", "*@evil.example"},
	}
	for _, test := range cases {
		if got := ExpandListMask(test.in); got != test.want {
			t.Errorf("ExpandListMask(%q) = %q, want %q", test.in, got, test.want)
		}
	}
}

func TestKickNickFromMask(t *testing.T) {
	cases := []struct {
		in, want string
	}{
		{"bob", "bob"},
		{"bob!*@*", "bob"},
		{"bob!u@h", "bob"},
		{"*!*@evil.example", "*"},
	}
	for _, test := range cases {
		if got := KickNickFromMask(test.in); got != test.want {
			t.Errorf("KickNickFromMask(%q) = %q, want %q", test.in, got, test.want)
		}
	}
}

func TestListMaskKindLetters(t *testing.T) {
	if ListMaskBan.ModeLetter() != 'b' {
		t.Fatalf("ban letter = %q, want b", ListMaskBan.ModeLetter())
	}
	if ListMaskQuiet.ModeLetter() != 'q' {
		t.Fatalf("quiet letter = %q, want q", ListMaskQuiet.ModeLetter())
	}
}
