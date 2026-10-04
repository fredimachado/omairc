package session

import (
	"fmt"
	"strings"
	"time"
	"unicode"

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
	folded := s.foldReadMarkerTargetLocked(target)
	networkID := s.config.NetworkID
	if len(message.Params) < 2 {
		s.emit(func(handler Handler) { handler.ReadMarkerReceived(networkID, target, nil) })
		return true
	}
	marker, ok := irc.ParseReadMarkerParameter(parameter(message, 1))
	if !ok {
		delete(s.readMarkerInFlight, folded)
		s.flushReadMarkerSetLocked(folded)
		return true
	}
	if marker == nil {
		s.emit(func(handler Handler) { handler.ReadMarkerReceived(networkID, target, nil) })
		return true
	}
	echoed := irc.ReadMarkerTimeMillis(*marker)
	s.emit(func(handler Handler) { handler.ReadMarkerReceived(networkID, target, &echoed) })
	if inFlight, hasInFlight := s.readMarkerInFlight[folded]; hasInFlight {
		if !irc.ReadMarkerTimeAfter(inFlight, echoed) {
			delete(s.readMarkerInFlight, folded)
			if pending, hasPending := s.readMarkerPending[folded]; hasPending {
				if !irc.ReadMarkerTimeAfter(pending.timestamp, echoed) {
					delete(s.readMarkerPending, folded)
				}
			}
			s.flushReadMarkerSetLocked(folded)
		}
	}
	return true
}

func readMarkerFailTargets(message irc.Message) []string {
	if len(message.Params) < 3 {
		return nil
	}
	if len(message.Params) == 3 {
		candidate := parameter(message, 2)
		if readMarkerFailContextTarget(candidate) {
			return []string{candidate}
		}
		return nil
	}
	end := len(message.Params) - 1
	var targets []string
	for index := 2; index < end; index++ {
		candidate := parameter(message, index)
		if readMarkerFailContextTarget(candidate) {
			targets = append(targets, candidate)
		}
	}
	return targets
}

func readMarkerFailContextTarget(candidate string) bool {
	if candidate == "" {
		return false
	}
	if strings.Contains(candidate, " ") {
		return false
	}
	if strings.Contains(candidate, "_") && strings.ToUpper(candidate) == candidate {
		return false
	}
	for _, r := range candidate {
		if !unicode.IsLetter(r) && !unicode.IsDigit(r) && r != '#' && r != '&' && r != '+' && r != '-' && r != '.' && r != '@' {
			return false
		}
	}
	return true
}

func (s *Session) handleReadMarkerFailLocked(message irc.Message) {
	command := parameter(message, 0)
	if !strings.EqualFold(command, "MARKREAD") && !strings.EqualFold(command, "READ") {
		return
	}
	targets := readMarkerFailTargets(message)
	if len(targets) == 0 {
		for folded := range s.readMarkerInFlight {
			delete(s.readMarkerInFlight, folded)
			s.flushReadMarkerSetLocked(folded)
		}
		return
	}
	for _, target := range targets {
		folded := s.foldReadMarkerTargetLocked(target)
		if folded == "" {
			continue
		}
		delete(s.readMarkerInFlight, folded)
		s.flushReadMarkerSetLocked(folded)
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
	when = irc.ReadMarkerTimeMillis(when)
	if s.readMarkerPending == nil {
		s.readMarkerPending = make(map[string]readMarkerQueue)
	}
	if s.readMarkerInFlight == nil {
		s.readMarkerInFlight = make(map[string]time.Time)
	}
	if inFlight, ok := s.readMarkerInFlight[folded]; ok {
		if !irc.ReadMarkerTimeAfter(when, inFlight) {
			return
		}
	}
	if pending, ok := s.readMarkerPending[folded]; ok && !irc.ReadMarkerTimeAfter(when, pending.timestamp) {
		return
	}
	s.readMarkerPending[folded] = readMarkerQueue{target: target, timestamp: when}
	s.flushReadMarkerSetLocked(folded)
}

func (s *Session) flushReadMarkerSetLocked(folded string) {
	if _, ok := s.readMarkerInFlight[folded]; ok {
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
	s.readMarkerInFlight[folded] = pending.timestamp
}
