package controller

import "github.com/fredimachado/omairc/tui/internal/irc"

// This file names the seams the later port phases plug into. Earlier phases
// ported only the state-model subset of IrcController, so each type documents
// the role its phase owns and the smallest entry point the controller uses.
//
// Phase 7 implements the command dispatcher, the reply router, the monitor
// coordinator, the autoaway runtime, the in-memory ignore/mute/highlight/
// avatar/preference stores, and the channel-list request/model inside this
// package (see commanddispatcher.go, replyrouter.go, monitorcoordinator.go,
// autoawayruntime.go, channellist.go, and stores.go). What remains here is
// still deferred.

// ConversationLog is the transcript persistence seam. It is the reducer's own
// irc.ConversationLog, already consumed by irc.EventReducer.SetConversationLog;
// Phase 10 owns the on-disk implementation.
type ConversationLog = irc.ConversationLog

// PlaybackCoordinator is the Phase 11 bouncer (znc.in/playback) seam. It owns
// the per-network playback clock, held query batches, and the PLAY request.
type PlaybackCoordinator interface {
	// OnRegistered begins playback for a freshly registered network.
	OnRegistered(networkID string, autojoinChannels []string)
}

// InboxStore is the Phase 9 inbox seam. The controller surfaces arrivals
// through Controller.OnInboxArrived and stores nothing yet.
type InboxStore interface {
	// Append stores one inbox item.
	Append(item irc.InboxItem)
}
