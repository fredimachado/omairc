package session

import (
	"errors"
	"sort"
	"sync"
)

// Manager owns the live sessions, one per network id. It is the Go shape of
// IrcSessionManager in src/irc/ircsessionmanager.h: create an add-only
// registry, look sessions up by network id, and activate/stop/discard them.
//
// The C++ manager keeps its sessions as QObject children and relies on
// deleteLater for teardown. Go has no parent/child ownership, so Manager holds
// the only reference: Discard stops the session and drops it, and the caller's
// reference becomes the last one.
type Manager struct {
	mu       sync.Mutex
	sessions map[string]*Session
}

// NewManager returns an empty manager.
func NewManager() *Manager {
	return &Manager{sessions: make(map[string]*Session)}
}

// Create builds a session, installs it in the registry, and returns it. It
// rejects an empty network id, a duplicate network id, and a nil transport,
// mirroring IrcSessionManager::createSession (ircsessionmanager.cpp:8-26).
func (m *Manager) Create(config SessionConfig, transport Transport, clock Clock) (*Session, error) {
	if transport == nil {
		return nil, errors.New("session: transport is required")
	}
	if config.NetworkID == "" {
		return nil, errors.New("session: network id is required")
	}

	m.mu.Lock()
	defer m.mu.Unlock()
	if m.sessions == nil {
		m.sessions = make(map[string]*Session)
	}
	if _, exists := m.sessions[config.NetworkID]; exists {
		return nil, errors.New("session: network id is already registered")
	}

	session := NewSession(config, transport, clock)
	m.sessions[config.NetworkID] = session
	return session, nil
}

// Find returns the session for networkID, or nil.
func (m *Manager) Find(networkID string) *Session {
	m.mu.Lock()
	defer m.mu.Unlock()
	return m.sessions[networkID]
}

// NetworkIDs returns every registered network id, sorted.
func (m *Manager) NetworkIDs() []string {
	m.mu.Lock()
	ids := make([]string, 0, len(m.sessions))
	for id := range m.sessions {
		ids = append(ids, id)
	}
	m.mu.Unlock()
	sort.Strings(ids)
	return ids
}

// Activate starts the session for networkID and reports whether it ended up
// live (state neither Idle nor Failed). It mirrors
// IrcSessionManager::activateSession (ircsessionmanager.cpp:41-48).
func (m *Manager) Activate(networkID string) bool {
	session := m.Find(networkID)
	if session == nil {
		return false
	}
	session.Start()
	return session.isLive()
}

// Stop shuts the session for networkID down. A missing network is a no-op.
func (m *Manager) Stop(networkID string) {
	if session := m.Find(networkID); session != nil {
		session.Stop()
	}
}

// Discard removes the session for networkID from the registry and stops it.
// The removal happens first so a concurrent Create for the same id sees a free
// slot; the observable contract (Find returns nil once Discard returns) matches
// IrcSessionManager::discardSession (ircsessionmanager.cpp:58-68). A missing
// network is a no-op.
func (m *Manager) Discard(networkID string) {
	m.mu.Lock()
	session := m.sessions[networkID]
	delete(m.sessions, networkID)
	m.mu.Unlock()
	if session != nil {
		session.Stop()
	}
}

// IsLive reports whether networkID is registered and its session is live.
func (m *Manager) IsLive(networkID string) bool {
	session := m.Find(networkID)
	return session != nil && session.isLive()
}

// isLive reports whether the session is neither Idle nor Failed. It mirrors
// IrcSessionManager::isLive (ircsessionmanager.cpp:70-74).
func (s *Session) isLive() bool {
	state := s.State()
	return state != StateIdle && state != StateFailed
}
