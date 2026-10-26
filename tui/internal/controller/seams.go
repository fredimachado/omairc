package controller

import "github.com/fredimachado/omairc/tui/internal/irc"

// ConversationLog is the transcript persistence seam. It is the reducer's own
// irc.ConversationLog, already consumed by irc.EventReducer.SetConversationLog.
// Desktop integration and the suppressDesktopNotification test latch live on
// the shell (internal/ui), mirroring `client/src/ui/shell` calling
// backend.notifyDesktop; the controller only emits the arrival signals.
type ConversationLog = irc.ConversationLog
