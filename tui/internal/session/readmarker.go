package session

import (
	"fmt"
	"strings"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

// setSlot is the one SET slot per folded target: idle, one in-flight stamp, or
// one in-flight stamp plus one newer queued stamp. Two in-flight stamps for a
// target cannot be stored. It mirrors client/src/session/readmarker.rs SetSlot.
type setSlot interface{ setSlot() }

type slotIdle struct{}

type slotInFlight struct{ Sent time.Time }

// slotQueued holds the newest stamp waiting behind Sent. Target is the wire
// spelling of the newest queued call.
type slotQueued struct {
	Sent, Next time.Time
	Target     string
}

func (slotIdle) setSlot()     {}
func (slotInFlight) setSlot() {}
func (slotQueued) setSlot()   {}

// sendSet is a SET line a transition wants on the wire now.
type sendSet struct {
	Target    string
	Timestamp time.Time
}

// offerSlot applies a newer-than-everything offer. lastKnown is the
// forward-only high-water mark; an equal or older stamp is ignored.
func offerSlot(slot setSlot, when time.Time, target string, lastKnown *time.Time) (setSlot, *sendSet) {
	if lastKnown != nil && !irc.ReadMarkerTimeAfter(when, *lastKnown) {
		return slot, nil
	}
	switch current := slot.(type) {
	case slotInFlight:
		if !irc.ReadMarkerTimeAfter(when, current.Sent) {
			return slot, nil
		}
		return slotQueued{Sent: current.Sent, Next: when, Target: target}, nil
	case slotQueued:
		if !irc.ReadMarkerTimeAfter(when, current.Next) {
			return slot, nil
		}
		return slotQueued{Sent: current.Sent, Next: when, Target: target}, nil
	default:
		return slotInFlight{Sent: when}, &sendSet{Target: target, Timestamp: when}
	}
}

// echoedSlot applies the echo of marker echoed: it releases the in-flight stamp
// when that stamp is not newer than the echo, drops a queued stamp the echo
// covers, and returns a still-newer queued stamp to send now.
func echoedSlot(slot setSlot, echoed time.Time) (setSlot, *sendSet) {
	switch current := slot.(type) {
	case slotInFlight:
		if irc.ReadMarkerTimeAfter(current.Sent, echoed) {
			return slot, nil
		}
		return slotIdle{}, nil
	case slotQueued:
		if irc.ReadMarkerTimeAfter(current.Sent, echoed) {
			return slot, nil
		}
		if !irc.ReadMarkerTimeAfter(current.Next, echoed) {
			return slotIdle{}, nil
		}
		return slotInFlight{Sent: current.Next}, &sendSet{Target: current.Target, Timestamp: current.Next}
	default:
		return slot, nil
	}
}

// releasedSlot applies a FAIL or an unparseable echo: the in-flight stamp is
// released (the same stamp may retry) and a queued stamp is sent now.
func releasedSlot(slot setSlot) (setSlot, *sendSet) {
	if current, ok := slot.(slotQueued); ok {
		return slotInFlight{Sent: current.Next}, &sendSet{Target: current.Target, Timestamp: current.Next}
	}
	return slotIdle{}, nil
}

func (s *Session) readMarkerSlotLocked(folded string) setSlot {
	if slot, ok := s.readMarkerSlots[folded]; ok {
		return slot
	}
	return slotIdle{}
}

// settleReadMarkerSlotLocked stores slot for folded and puts send on the wire.
// A refused send leaves the slot idle: nothing is in flight.
func (s *Session) settleReadMarkerSlotLocked(folded string, slot setSlot, send *sendSet) {
	if send != nil && !s.sendReadMarkerSetLocked(*send) {
		slot = slotIdle{}
	}
	if _, idle := slot.(slotIdle); idle {
		delete(s.readMarkerSlots, folded)
		return
	}
	if s.readMarkerSlots == nil {
		s.readMarkerSlots = make(map[string]setSlot)
	}
	s.readMarkerSlots[folded] = slot
}

func (s *Session) sendReadMarkerSetLocked(set sendSet) bool {
	command := s.readMarkerCommandLocked()
	if command == "" {
		return false
	}
	line := fmt.Sprintf("%s %s timestamp=%s", command, set.Target, irc.FormatReadMarkerTime(set.Timestamp))
	return s.sendCommandLocked(line, "")
}

// releaseReadMarkerSlotLocked is a FAIL or unparseable echo for folded.
func (s *Session) releaseReadMarkerSlotLocked(folded string) {
	slot, send := releasedSlot(s.readMarkerSlotLocked(folded))
	s.settleReadMarkerSlotLocked(folded, slot, send)
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
		s.releaseReadMarkerSlotLocked(folded)
		// Unparseable replies still release a controller get-hold.
		s.emit(func(handler Handler) { handler.ReadMarkerReceived(networkID, target, nil) })
		return true
	}
	if marker == nil {
		s.emit(func(handler Handler) { handler.ReadMarkerReceived(networkID, target, nil) })
		return true
	}
	echoed := irc.ReadMarkerTimeMillis(*marker)
	s.advanceReadMarkerLastKnownLocked(folded, echoed)
	s.emit(func(handler Handler) { handler.ReadMarkerReceived(networkID, target, &echoed) })
	slot, send := echoedSlot(s.readMarkerSlotLocked(folded), echoed)
	s.settleReadMarkerSlotLocked(folded, slot, send)
	return true
}

func (s *Session) handleReadMarkerFailLocked(message irc.Message) {
	command := parameter(message, 0)
	if !strings.EqualFold(command, "MARKREAD") && !strings.EqualFold(command, "READ") {
		return
	}
	networkID := s.config.NetworkID
	if len(message.Params) < 4 {
		folded := make([]string, 0, len(s.readMarkerSlots))
		for key := range s.readMarkerSlots {
			folded = append(folded, key)
		}
		for _, key := range folded {
			s.releaseReadMarkerSlotLocked(key)
		}
		// Empty target clears every awaiting get on this network.
		s.emit(func(handler Handler) { handler.ReadMarkerReceived(networkID, "", nil) })
		return
	}
	target := parameter(message, 2)
	folded := s.foldReadMarkerTargetLocked(target)
	if folded == "" {
		return
	}
	s.releaseReadMarkerSlotLocked(folded)
	s.emit(func(handler Handler) { handler.ReadMarkerReceived(networkID, target, nil) })
}

// ReadMarkerCommand returns the wire verb for an acknowledged read-marker cap, or "".
func (s *Session) ReadMarkerCommand() string {
	var command string
	s.locked(func() { command = s.readMarkerCommandLocked() })
	return command
}

// RequestReadMarkerGet asks the server for the stored marker on one target. It
// reports whether the command was sent (session registered and cap enabled).
func (s *Session) RequestReadMarkerGet(target string) bool {
	if target == "" {
		return false
	}
	var sent bool
	s.locked(func() { sent = s.requestReadMarkerGetLocked(target) })
	return sent
}

func (s *Session) requestReadMarkerGetLocked(target string) bool {
	command := s.readMarkerCommandLocked()
	if command == "" {
		return false
	}
	return s.sendCommandLocked(fmt.Sprintf("%s %s", command, target), "")
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
	var lastKnown *time.Time
	if known, ok := s.readMarkerLastKnown[folded]; ok {
		lastKnown = &known
	}
	slot, send := offerSlot(s.readMarkerSlotLocked(folded), when, target, lastKnown)
	s.settleReadMarkerSlotLocked(folded, slot, send)
}

func (s *Session) advanceReadMarkerLastKnownLocked(folded string, when time.Time) {
	if s.readMarkerLastKnown == nil {
		s.readMarkerLastKnown = make(map[string]time.Time)
	}
	if known, ok := s.readMarkerLastKnown[folded]; ok && !irc.ReadMarkerTimeAfter(when, known) {
		return
	}
	s.readMarkerLastKnown[folded] = when
}
