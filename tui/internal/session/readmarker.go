package session

import (
	"fmt"
	"strings"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

type readMarkerQueue struct {
	target    string
	timestamp time.Time
}

func (s *Session) foldReadMarkerTargetLocked(target string) string {
	if target == "" {
		return ""
	}
	if s.features.IsChannel(target) {
		return s.foldChannelLocked(target)
	}
	return s.features.CaseMapping().Normalize(target)
}

func (s *Session) readMarkerCommandLocked() string {
	return irc.ReadMarkerCommand(s.capabilities)
}

func (s *Session) deliverReadMarkerLocked(message irc.Message) bool {
	command := message.Command
	if !strings.EqualFold(command, "MARKREAD") && !strings.EqualFold(command, "READ") {
		return false
	}
	if len(message.Params) < 1 {
		return true
	}
	target := parameter(message, 0)
	if target == "" {
		return true
	}
	if len(message.Params) < 2 {
		networkID := s.config.NetworkID
		s.emit(func(handler Handler) { handler.ReadMarkerReceived(networkID, target, nil) })
		return true
	}
	marker, ok := irc.ParseReadMarkerParameter(parameter(message, 1))
	if !ok {
		return true
	}
	folded := s.foldReadMarkerTargetLocked(target)
	delete(s.readMarkerInFlight, folded)
	if s.readMarkerPending != nil {
		s.flushReadMarkerSetLocked(folded)
	}
	networkID := s.config.NetworkID
	s.emit(func(handler Handler) { handler.ReadMarkerReceived(networkID, target, marker) })
	return true
}

func (s *Session) handleReadMarkerFailLocked(message irc.Message) {
	command := parameter(message, 0)
	if !strings.EqualFold(command, "MARKREAD") && !strings.EqualFold(command, "READ") {
		return
	}
	for index := 1; index < len(message.Params); index++ {
		target := parameter(message, index)
		if target == "" {
			continue
		}
		folded := s.foldReadMarkerTargetLocked(target)
		if s.readMarkerInFlight[folded] {
			delete(s.readMarkerInFlight, folded)
			s.flushReadMarkerSetLocked(folded)
			return
		}
	}
}

// RequestReadMarkerGet asks the server for the stored marker on one target.
func (s *Session) RequestReadMarkerGet(target string) {
	if target == "" {
		return
	}
	s.locked(func() { s.requestReadMarkerGetLocked(target) })
}

func (s *Session) requestReadMarkerGetLocked(target string) {
	command := s.readMarkerCommandLocked()
	if command == "" {
		return
	}
	s.sendCommandLocked(fmt.Sprintf("%s %s", command, target), "")
}

// QueueReadMarkerSet records a newer read marker to publish when caught up.
func (s *Session) QueueReadMarkerSet(target string, when time.Time) {
	if target == "" || when.IsZero() {
		return
	}
	s.locked(func() { s.queueReadMarkerSetLocked(target, when) })
}

func (s *Session) queueReadMarkerSetLocked(target string, when time.Time) {
	command := s.readMarkerCommandLocked()
	if command == "" {
		return
	}
	folded := s.foldReadMarkerTargetLocked(target)
	if folded == "" {
		return
	}
	if s.readMarkerPending == nil {
		s.readMarkerPending = make(map[string]readMarkerQueue)
	}
	if s.readMarkerInFlight == nil {
		s.readMarkerInFlight = make(map[string]bool)
	}
	if pending, ok := s.readMarkerPending[folded]; ok && !when.After(pending.timestamp) {
		return
	}
	s.readMarkerPending[folded] = readMarkerQueue{target: target, timestamp: when}
	s.flushReadMarkerSetLocked(folded)
}

func (s *Session) flushReadMarkerSetLocked(folded string) {
	if s.readMarkerInFlight[folded] {
		return
	}
	pending, ok := s.readMarkerPending[folded]
	if !ok {
		return
	}
	command := s.readMarkerCommandLocked()
	if command == "" {
		delete(s.readMarkerPending, folded)
		return
	}
	line := fmt.Sprintf("%s %s timestamp=%s", command, pending.target, irc.FormatReadMarkerTime(pending.timestamp))
	s.sendCommandLocked(line, "")
	delete(s.readMarkerPending, folded)
	s.readMarkerInFlight[folded] = true
}
