package controller

import (
	"github.com/fredimachado/omairc/tui/internal/irc"
	"github.com/fredimachado/omairc/tui/internal/storage"
)

// Each conversation's unsent composer text is kept across a restart in
// drafts.json (storage.DraftStore). The shell hands a draft over when it
// switches away, when the terminal loses focus, and on quit, never per
// keystroke, and asks for it back when a conversation is shown. The key is the
// network id plus the conversation's name under the network's case mapping.
// irc.DraftMayBePersisted decides what may be written; a draft it rejects
// removes the stored one instead. The Status pane has no draft here. Rust:
// client/src/controller/drafts.rs.

// SetDraftStore replaces the draft store and loads it as of now (tests and
// the shell's disk wiring).
func (c *Controller) SetDraftStore(store *storage.DraftStore) {
	if store == nil {
		store = storage.NewDraftStore()
	}
	c.drafts = store
	c.drafts.Load(c.now())
}

// RememberDraft keeps a conversation's unsent text across a restart. Blank
// text removes it, and so does anything the secret gate rejects.
func (c *Controller) RememberDraft(networkID, target, text string) {
	if c == nil || c.drafts == nil {
		return
	}
	features := c.reducer.ServerFeatures(networkID)
	c.drafts.Remember(networkID, target, text, features.CaseMapping(), features.ChannelTypes(), c.now())
}

// DraftFor returns the stored draft for a conversation, if any.
func (c *Controller) DraftFor(networkID, target string) (string, bool) {
	if c == nil || c.drafts == nil {
		return "", false
	}
	features := c.reducer.ServerFeatures(networkID)
	return c.drafts.Get(networkID, target, features.CaseMapping())
}

// RememberDraftByID is RememberDraft for a conversation id (the shell's draft
// key). An id that does not parse keeps nothing.
func (c *Controller) RememberDraftByID(conversationID, text string) {
	if key, ok := irc.ParseConversationID(conversationID); ok {
		c.RememberDraft(key.NetworkID, key.NormalizedTarget, text)
	}
}

// DraftForID is DraftFor for a conversation id.
func (c *Controller) DraftForID(conversationID string) (string, bool) {
	key, ok := irc.ParseConversationID(conversationID)
	if !ok {
		return "", false
	}
	return c.DraftFor(key.NetworkID, key.NormalizedTarget)
}
