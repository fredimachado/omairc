package controller

import (
	"testing"

	"github.com/fredimachado/omairc/tui/internal/irc"
	"github.com/fredimachado/omairc/tui/internal/storage"
)

// This file ports the Phase 11 open-direct store checks: remembering a direct,
// forgetting it, and restoring persisted directs at MOTD end.

func hasConversationKey(c *Controller, networkID, target string) bool {
	return c.reducer.Find(c.reducer.ConversationKey(networkID, target)) != nil
}

func TestRestoreOpenDirectsAfterMotd(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())

	c := New()
	c.SetEphemeral(false)
	c.LoadStoredPreferences()

	store := storage.NewOpenDirectStore()
	c.SetOpenDirectStore(store)
	features := c.reducer.ServerFeatures("freenode")
	mapping := features.CaseMapping()
	store.Add("freenode", "alice", mapping)
	store.Add("freenode", "#chan", mapping)
	store.Add("freenode", "NickServ", mapping)

	c.MessageReceived("freenode", irc.Message{Command: "376", Params: []string{"omairc", "End of MOTD"}})

	if !hasConversationKey(c, "freenode", "alice") {
		t.Fatalf("alice was not restored; conversations = %v", conversationTargets(c))
	}
	if hasConversationKey(c, "freenode", "#chan") {
		t.Fatalf("channel restored as a direct")
	}
	if hasConversationKey(c, "freenode", "NickServ") {
		t.Fatalf("service restored as a direct")
	}
	// The prune drops the non-persistable entries from the store too.
	for _, target := range store.Targets("freenode") {
		if target != "alice" {
			t.Fatalf("store kept %q after prune", target)
		}
	}
}

func TestRestoreOpenDirectsRespectsReopenToggle(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())

	c := New()
	c.SetEphemeral(false)
	c.LoadStoredPreferences()
	c.SetReopenDirects(false)

	store := storage.NewOpenDirectStore()
	c.SetOpenDirectStore(store)
	features := c.reducer.ServerFeatures("freenode")
	mapping := features.CaseMapping()
	store.Add("freenode", "alice", mapping)

	c.MessageReceived("freenode", irc.Message{Command: "376", Params: []string{"omairc", "End of MOTD"}})

	if hasConversationKey(c, "freenode", "alice") {
		t.Fatalf("restore ran while reopen directs is off")
	}
	if len(store.Targets("freenode")) != 1 {
		t.Fatalf("store changed while reopen directs is off: %v", store.Targets("freenode"))
	}
}

func TestRestoreOpenDirectsSkipsExistingConversation(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())

	c := New()
	c.SetEphemeral(false)
	c.LoadStoredPreferences()

	store := storage.NewOpenDirectStore()
	c.SetOpenDirectStore(store)
	features := c.reducer.ServerFeatures("freenode")
	mapping := features.CaseMapping()
	store.Add("freenode", "alice", mapping)

	// alice already exists, so the restore must not create a duplicate or
	// change the existing transcript.
	key := c.reducer.ConversationKey("freenode", "alice")
	c.reducer.EnsureConversation(key, "alice", irc.CauseUserOpen)
	before := len(c.reducer.Conversations())

	c.MessageReceived("freenode", irc.Message{Command: "376", Params: []string{"omairc", "End of MOTD"}})

	if got := len(c.reducer.Conversations()); got != before {
		t.Fatalf("conversation count = %d, want %d", got, before)
	}
}

func TestRememberAndForgetOpenDirect(t *testing.T) {
	c := New()
	store := storage.NewOpenDirectStore()
	store.SetEphemeral(true)
	c.SetOpenDirectStore(store)

	c.rememberOpenDirect("freenode", "alice")
	c.rememberOpenDirect("freenode", "#chan")
	c.rememberOpenDirect("freenode", "NickServ")

	if got := store.Targets("freenode"); len(got) != 1 || got[0] != "alice" {
		t.Fatalf("Targets = %v, want [alice]", got)
	}

	c.forgetOpenDirect("freenode", "alice")
	if got := store.Targets("freenode"); len(got) != 0 {
		t.Fatalf("Targets after forget = %v, want empty", got)
	}
}
