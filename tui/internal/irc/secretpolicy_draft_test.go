package irc

import "testing"

// keep the cases identical to allows_draft_persistence in
// client/src/irc/secretpolicy.rs
func TestDraftMayBePersisted(t *testing.T) {
	for _, kept := range []string{
		"half typed",
		"  indented prose",
		"see you at five",
		"/me waves",
		"/ME waves",
		"  /me waves",
		"> not a command",
	} {
		if !DraftMayBePersisted(kept, "#&") {
			t.Errorf("DraftMayBePersisted(%q) = false, want true", kept)
		}
	}
	for _, dropped := range []string{
		"/msg NickServ IDENTIFY pw",
		"/ns identify pw",
		"/quote PASS hunter2",
		"/oper a b",
		"/join #a",
		"/join #a key",
		"/me",
		"/mew hi",
		"/msg alice hi",
		"/me PASS hunter2",
		"/me identify hunter2",
		"PASS hunter2",
		"IDENTIFY hunter2",
		"identify pw",
		"garbage PASS hunter2",
		"PRIVMSG NickServ :IDENTIFY pw",
		"oper a b",
	} {
		if DraftMayBePersisted(dropped, "#&") {
			t.Errorf("DraftMayBePersisted(%q) = true, want false", dropped)
		}
	}
}
