package controller

import "github.com/fredimachado/omairc/tui/internal/irc"

// This file names the seams the later port phases plug into. Phase 2 ports only
// the state-model subset of IrcController, so none of these are called by the
// controller yet: each type documents the role its phase owns and the smallest
// entry point the controller would use. They exist so later waves can add the
// implementation without re-deriving the boundary, and so a reviewer can see
// exactly which behaviour is deliberately absent.
//
// The types are nil-able seam handles. Phase 2 never stores or dereferences
// them; a later phase adds the controller field and the call sites.

// ConversationLog is the transcript persistence seam. It is the reducer's own
// irc.ConversationLog, already consumed by irc.EventReducer.SetConversationLog;
// Phase 10 owns the on-disk implementation.
type ConversationLog = irc.ConversationLog

// CommandDispatcher is the Phase 7 slash-command seam. It parses a composer
// submission into a command and executes it. Phase 2's SendMessage is a plain
// PRIVMSG only and never calls this.
type CommandDispatcher interface {
	// Dispatch runs one composer submission and reports whether it was sent.
	Dispatch(text string) bool
}

// PlaybackCoordinator is the Phase 11 bouncer (znc.in/playback) seam. It owns
// the per-network playback clock, held query batches, and the PLAY request.
type PlaybackCoordinator interface {
	// OnRegistered begins playback for a freshly registered network.
	OnRegistered(networkID string, autojoinChannels []string)
}

// MonitorCoordinator is the Phase 8 MONITOR seam. It owns the monitor list and
// the 730/731/734 presence numerics.
type MonitorCoordinator interface {
	// ForgetPresence drops the coordinator's cached presence for a network.
	ForgetPresence(networkID string)
}

// ReplyRouter is the Phase 6 WHOIS / CTCP / status-reply seam. It routes
// correlated replies into the conversation or Status surface that asked.
type ReplyRouter interface {
	// Forget drops the router's per-network state.
	Forget(networkID string)
}

// MuteStore is the Phase 8 mute persistence seam. Phase 2 keeps the in-memory
// reducer flag only; nothing is written to disk.
type MuteStore interface {
	// Targets lists the muted targets for one network.
	Targets(networkID string) []string
}

// IgnoreStore is the Phase 8 ignore persistence seam. It backs the session's
// IgnoreFilter, which Phase 2 leaves unset.
type IgnoreStore interface {
	// Nicks lists the ignored nicks for one network.
	Nicks(networkID string) []string
}

// HighlightStore is the Phase 8 highlight-word seam. It is the source
// irc.EventReducer.SetHighlightWords reads from.
type HighlightStore interface {
	// Words lists the highlight words for one network.
	Words(networkID string) []string
}

// AutoawayRuntime is the Phase 8 idle/autoaway seam. Phase 2 does not send
// AWAY after chat.
type AutoawayRuntime interface {
	// NoteLocalActivity records local user activity for the idle clock.
	NoteLocalActivity()
}

// Preferences is the Phase 9 stored-preferences seam (reopen directs, avatars,
// open-at-unread). Phase 2 uses no preference.
type Preferences interface {
	// Bool reads a stored boolean preference, falling back when unset.
	Bool(name string, fallback bool) bool
}

// AvatarStore is the Phase 9 profile-avatar seam.
type AvatarStore interface {
	// URLForNetwork returns the stored avatar URL for a network.
	URLForNetwork(networkID string) string
}

// InboxStore is the Phase 9 inbox seam. Phase 2 surfaces arrivals through
// Controller.OnInboxArrived and stores nothing.
type InboxStore interface {
	// Append stores one inbox item.
	Append(item irc.InboxItem)
}

// ChannelList is the Phase 10 /list seam.
type ChannelList interface {
	// Forget drops the cached channel list for a network.
	Forget(networkID string)
}
