package controller

import "github.com/fredimachado/omairc/tui/internal/irc"

// DefaultStatusConsoleCapacity is the per-network Status ring-buffer cap. It
// matches IrcStatusConsole's default in src/irc/ircstatusconsole.h.
const DefaultStatusConsoleCapacity = 2000

// StatusConsole is the minimal Phase 2 Status console: a per-network ring
// buffer of classified irc.StatusEntry lines. It mirrors the storage half of
// IrcStatusConsole; the open/network projection and the slash dispatch that
// the Qt console also owns land in later phases.
//
// A StatusConsole is not safe for concurrent use; see the package doc.
type StatusConsole struct {
	capacity int
	lines    map[string][]irc.StatusEntry
}

// NewStatusConsole returns an empty console. A non-positive capacity falls
// back to DefaultStatusConsoleCapacity.
func NewStatusConsole(capacity int) *StatusConsole {
	if capacity <= 0 {
		capacity = DefaultStatusConsoleCapacity
	}
	return &StatusConsole{
		capacity: capacity,
		lines:    make(map[string][]irc.StatusEntry),
	}
}

// Append records one entry on its network's buffer, dropping the oldest entry
// once the buffer is over capacity. An entry with no network id is ignored.
func (s *StatusConsole) Append(entry irc.StatusEntry) {
	networkID := entry.NetworkID()
	if networkID == "" {
		return
	}
	buffer := append(s.lines[networkID], entry)
	if len(buffer) > s.capacity {
		trimmed := make([]irc.StatusEntry, s.capacity)
		copy(trimmed, buffer[len(buffer)-s.capacity:])
		buffer = trimmed
	}
	s.lines[networkID] = buffer
}

// Lines returns a copy of one network's entries, oldest first. A network with
// no recorded lines returns nil.
func (s *StatusConsole) Lines(networkID string) []irc.StatusEntry {
	if networkID == "" {
		return nil
	}
	buffer := s.lines[networkID]
	if len(buffer) == 0 {
		return nil
	}
	out := make([]irc.StatusEntry, len(buffer))
	copy(out, buffer)
	return out
}

// Clear drops every recorded line for one network.
func (s *StatusConsole) Clear(networkID string) {
	if networkID == "" {
		return
	}
	delete(s.lines, networkID)
}
