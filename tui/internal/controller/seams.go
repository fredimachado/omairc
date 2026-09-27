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
// autoawayruntime.go, channellist.go, and stores.go). Phase 9 implements the
// inbox store in Controller (see inbox.go and controller.go). Phase 11
// implements the bouncer playback coordinator (playback.go) and wires the
// on-disk stores, so what remains here is only the transcript log alias.
// Desktop integration and the suppressDesktopNotification test latch live on
// the shell (internal/ui), mirroring OmaircWindow.qml calling
// backend.notifyDesktop; the controller only emits the arrival signals.

// ConversationLog is the transcript persistence seam. It is the reducer's own
// irc.ConversationLog, already consumed by irc.EventReducer.SetConversationLog;
// Phase 10 owns the on-disk implementation and Phase 11 wires it.
type ConversationLog = irc.ConversationLog
