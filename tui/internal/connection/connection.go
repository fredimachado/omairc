package connection

import (
	"maps"
	"slices"
	"strings"

	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/irc"
	"github.com/fredimachado/omairc/tui/internal/session"
)

// TransportFactory builds one transport per applied network. The shell injects
// the real transport; tests inject session.NewLoopbackTransport. A nil factory
// or a nil transport makes reconcile fail without touching the controller.
type TransportFactory func() session.Transport

// NetworkRow is one entry in the roster the Connect sheet renders. It mirrors
// IrcConnection::RosterRow, minus the icon and collapse fields phase 5 does
// not need.
type NetworkRow struct {
	NetworkID   string
	DisplayName string
	Stored      bool
	Selected    bool
}

// appliedSession caches the normalized profile and secrets last handed to the
// controller, so an unchanged Apply can restart the existing session instead of
// rebuilding it. It mirrors IrcConnection::IrcAppliedSession.
type appliedSession struct {
	profile          NetworkProfile
	password         string
	nickServPassword string
}

// Connection owns the Connect sheet's draft profile, the stored profile list,
// the selection, and the in-memory secrets. It is the Go port of the
// non-persistent half of IrcConnection. It is not safe for concurrent use.
type Connection struct {
	// OnDraftChanged fires whenever the shown draft or selection changes.
	// Nil-able and invoked synchronously.
	OnDraftChanged func()
	// OnNetworksChanged fires whenever the roster rows change. Nil-able.
	OnNetworksChanged func()
	// OnSetupRequiredChanged fires when SetupRequired flips. Nil-able.
	OnSetupRequiredChanged func()

	ctrl    *controller.Controller
	factory TransportFactory

	stored            []NetworkProfile
	draft             NetworkProfile
	selectedNetworkID string
	addedFrom         string

	passwords         map[string]string
	nickServPasswords map[string]string
	applied           map[string]appliedSession

	reopenDirects bool
	showAvatars   bool
	openAtUnread  bool
}

// New builds a connection over ctrl. With no stored profiles the draft is the
// suggested Libera profile and selectedNetworkId points at it. Seed persisted
// profiles with SetStoredProfiles; storage is a phase-11 seam.
func New(ctrl *controller.Controller, factory TransportFactory) *Connection {
	connection := &Connection{
		ctrl:              ctrl,
		factory:           factory,
		passwords:         make(map[string]string),
		nickServPasswords: make(map[string]string),
		applied:           make(map[string]appliedSession),
		reopenDirects:     true,
		showAvatars:       true,
		openAtUnread:      true,
	}
	connection.draft = SuggestedProfile()
	connection.selectedNetworkID = connection.draft.NetworkID
	connection.draft.EnsureIconColor(connection.usedIconColors(connection.draft.NetworkID))
	return connection
}

// SetStoredProfiles seeds the stored list, mirroring
// IrcConnection::setStoredProfiles minus persistence. It selects the first
// complete profile, else the first; with an empty list it falls back to the
// suggested profile.
func (c *Connection) SetStoredProfiles(profiles []NetworkProfile) {
	wasSetup := c.SetupRequired()
	c.stored = cloneProfiles(profiles)
	c.assignStoredIconColors()
	if len(c.stored) > 0 {
		chosen := c.stored[0]
		for _, profile := range c.stored {
			if profile.IsComplete() {
				chosen = profile
				break
			}
		}
		c.selectedNetworkID = chosen.NetworkID
		c.draft = chosen
	} else {
		c.draft = SuggestedProfile()
		c.selectedNetworkID = c.draft.NetworkID
		c.draft.EnsureIconColor(c.usedIconColors(c.draft.NetworkID))
	}
	c.draftChanged()
	c.networksChanged()
	if wasSetup != c.SetupRequired() {
		c.setupChanged()
	}
}

// --- Draft accessors ------------------------------------------------------

// Name returns the draft display name.
func (c *Connection) Name() string { return c.draft.Name }

// SetName replaces the draft display name.
func (c *Connection) SetName(name string) {
	if c.draft.Name == name {
		return
	}
	c.draft.Name = name
	c.draftChanged()
	c.networksChanged()
}

// Host returns the draft host.
func (c *Connection) Host() string { return c.draft.Host }

// SetHost replaces the draft host.
func (c *Connection) SetHost(host string) {
	if c.draft.Host == host {
		return
	}
	c.draft.Host = host
	c.draftChanged()
	c.networksChanged()
}

// Port returns the draft port.
func (c *Connection) Port() int { return int(c.draft.Port) }

// SetPort replaces the draft port, bounded to a uint16 like the C++ setter.
func (c *Connection) SetPort(port int) {
	bounded := port
	if bounded < 0 {
		bounded = 0
	}
	if bounded > 65535 {
		bounded = 65535
	}
	next := uint16(bounded)
	if c.draft.Port == next {
		return
	}
	c.draft.Port = next
	c.draftChanged()
}

// TLSEnabled reports the draft TLS flag.
func (c *Connection) TLSEnabled() bool { return c.draft.TLSEnabled }

// SetTLSEnabled replaces the draft TLS flag.
func (c *Connection) SetTLSEnabled(enabled bool) {
	if c.draft.TLSEnabled == enabled {
		return
	}
	c.draft.TLSEnabled = enabled
	c.draftChanged()
}

// ConnectOnStartup reports whether the draft network connects at launch.
func (c *Connection) ConnectOnStartup() bool { return c.draft.ConnectOnStartup }

// SetConnectOnStartup replaces the draft startup flag.
func (c *Connection) SetConnectOnStartup(enabled bool) {
	if c.draft.ConnectOnStartup == enabled {
		return
	}
	c.draft.ConnectOnStartup = enabled
	c.draftChanged()
}

// Nick returns the draft nick.
func (c *Connection) Nick() string { return c.draft.Nick }

// SetNick replaces the draft nick.
func (c *Connection) SetNick(nick string) {
	if c.draft.Nick == nick {
		return
	}
	c.draft.Nick = nick
	c.draftChanged()
	c.networksChanged()
}

// Username returns the draft username.
func (c *Connection) Username() string { return c.draft.Username }

// SetUsername replaces the draft username.
func (c *Connection) SetUsername(username string) {
	if c.draft.Username == username {
		return
	}
	c.draft.Username = username
	c.draftChanged()
}

// Realname returns the draft real name.
func (c *Connection) Realname() string { return c.draft.Realname }

// SetRealname replaces the draft real name.
func (c *Connection) SetRealname(realname string) {
	if c.draft.Realname == realname {
		return
	}
	c.draft.Realname = realname
	c.draftChanged()
}

// Account returns the draft SASL account.
func (c *Connection) Account() string { return c.draft.Account }

// SetAccount replaces the draft SASL account.
func (c *Connection) SetAccount(account string) {
	if c.draft.Account == account {
		return
	}
	c.draft.Account = account
	c.draftChanged()
}

// BouncerNetwork returns the draft bouncer upstream network.
func (c *Connection) BouncerNetwork() string { return c.draft.BouncerNetwork }

// SetBouncerNetwork replaces the draft bouncer upstream network.
func (c *Connection) SetBouncerNetwork(network string) {
	if c.draft.BouncerNetwork == network {
		return
	}
	c.draft.BouncerNetwork = network
	c.draftChanged()
}

// Autojoin returns the draft channels joined with single spaces, mirroring
// IrcConnection::autojoin.
func (c *Connection) Autojoin() string {
	return strings.Join(c.draft.AutojoinChannels, " ")
}

// SetAutojoin parses the typed channels and recomputes the retained keys,
// mirroring IrcConnection::setAutojoin.
func (c *Connection) SetAutojoin(channels string) {
	next := ParseAutojoin(channels)
	probe := c.draft
	probe.AutojoinChannels = next
	nextKeys := probe.Normalized().AutojoinKeys
	if slices.Equal(c.draft.AutojoinChannels, next) && maps.Equal(c.draft.AutojoinKeys, nextKeys) {
		return
	}
	c.draft.AutojoinChannels = next
	c.draft.AutojoinKeys = nextKeys
	c.draftChanged()
}

// SetPassword replaces the selected network's in-memory password. An empty
// value cannot clear an existing password, mirroring IrcConnection::setPassword
// without its credential-store branch.
func (c *Connection) SetPassword(password string) {
	current := c.passwords[c.selectedNetworkID]
	if password == "" && current != "" {
		return
	}
	if current == password {
		return
	}
	c.passwords[c.selectedNetworkID] = password
	c.draftChanged()
}

// SetNickServPassword replaces the selected network's in-memory NickServ
// password. An empty value cannot clear an existing password.
func (c *Connection) SetNickServPassword(password string) {
	current := c.nickServPasswords[c.selectedNetworkID]
	if password == "" && current != "" {
		return
	}
	if current == password {
		return
	}
	c.nickServPasswords[c.selectedNetworkID] = password
	c.draftChanged()
}

// PasswordSet reports whether the selected network has an in-memory password.
func (c *Connection) PasswordSet() bool {
	return c.passwords[c.selectedNetworkID] != ""
}

// NickServSet reports whether the selected network has an in-memory NickServ
// password.
func (c *Connection) NickServSet() bool {
	return c.nickServPasswords[c.selectedNetworkID] != ""
}

// --- Roster and status ----------------------------------------------------

// Networks returns the roster rows: every stored profile in stored order, with
// the draft shown for the selected row, then a non-stored row for the selected
// draft when it is not stored. It mirrors IrcConnection::rosterRows.
func (c *Connection) Networks() []NetworkRow {
	rows := make([]NetworkRow, 0, len(c.stored)+1)
	for _, profile := range c.stored {
		shown := profile
		if profile.NetworkID == c.selectedNetworkID {
			shown = c.draft
		}
		rows = append(rows, NetworkRow{
			NetworkID:   profile.NetworkID,
			DisplayName: c.rosterDisplayName(shown),
			Stored:      true,
			Selected:    profile.NetworkID == c.selectedNetworkID,
		})
	}
	if !c.isStored(c.selectedNetworkID) && c.selectedNetworkID != "" {
		rows = append(rows, NetworkRow{
			NetworkID:   c.selectedNetworkID,
			DisplayName: c.rosterDisplayName(c.draft),
			Stored:      false,
			Selected:    true,
		})
	}
	return rows
}

// SelectedNetworkID returns the selected network id.
func (c *Connection) SelectedNetworkID() string { return c.selectedNetworkID }

// DisplayName returns the draft's roster label.
func (c *Connection) DisplayName() string { return c.rosterDisplayName(c.draft) }

// Problem returns the draft's validation message, or "" when it is complete.
func (c *Connection) Problem() string { return ProblemText(c.draft.Validate()) }

// Dirty reports whether the draft differs from the stored profile it edits, or
// whether the selection is not stored at all.
func (c *Connection) Dirty() bool {
	stored, ok := c.storedProfile(c.selectedNetworkID)
	if !ok {
		return true
	}
	return !profilesEqual(c.draft.Normalized(), stored.Normalized())
}

// SetupRequired reports whether no stored profile is complete.
func (c *Connection) SetupRequired() bool {
	for _, profile := range c.stored {
		if profile.IsComplete() {
			return false
		}
	}
	return true
}

// CanAdd reports whether a new draft can be started.
func (c *Connection) CanAdd() bool {
	return !c.SetupRequired() && c.isStored(c.selectedNetworkID) && !c.Dirty()
}

// CanRemove reports whether the selected network is stored.
func (c *Connection) CanRemove() bool {
	return c.isStored(c.selectedNetworkID)
}

// CanDisconnect reports whether the selected network's session is live.
func (c *Connection) CanDisconnect() bool {
	return c.ctrl.SessionIsLive(c.selectedNetworkID)
}

// --- Draft and selection surface ------------------------------------------

// Select activates a stored network, or the current non-stored draft. It is a
// no-op when empty, already selected, dirty, or neither stored nor the draft.
func (c *Connection) Select(networkID string) {
	if networkID == "" || networkID == c.selectedNetworkID {
		return
	}
	if c.Dirty() {
		return
	}
	if !c.isStored(networkID) && networkID != c.draft.NetworkID {
		return
	}
	c.selectStored(networkID)
}

// Add starts a new draft network when CanAdd allows it.
func (c *Connection) Add() bool {
	if !c.CanAdd() {
		return false
	}
	c.addedFrom = c.selectedNetworkID
	c.draft = CreateProfile()
	c.draft.Port = 6697
	c.draft.TLSEnabled = true
	c.draft.EnsureIconColor(c.usedIconColors(c.draft.NetworkID))
	c.selectedNetworkID = c.draft.NetworkID
	c.draftChanged()
	c.networksChanged()
	return true
}

// Discard abandons the non-stored draft, returning to the network it was added
// from or the first stored one. When the selection is stored it restores the
// draft from that stored profile.
func (c *Connection) Discard() {
	if !c.isStored(c.selectedNetworkID) {
		dropped := c.selectedNetworkID
		delete(c.passwords, dropped)
		delete(c.nickServPasswords, dropped)
		if len(c.stored) > 0 {
			returnID := c.addedFrom
			if !c.isStored(returnID) {
				returnID = c.stored[0].NetworkID
			}
			c.addedFrom = ""
			c.selectStored(returnID)
			return
		}
		c.draft = SuggestedProfile()
		c.draft.NetworkID = dropped
		c.draft.EnsureIconColor(c.usedIconColors(dropped))
		c.draftChanged()
		c.networksChanged()
		return
	}
	if stored, ok := c.storedProfile(c.selectedNetworkID); ok {
		c.draft = stored
	}
	c.draftChanged()
	c.networksChanged()
}

// RemoveSelected drops the selected stored profile, its session, its cached
// state, and its secrets, then selects a neighbor. It reports whether anything
// was removed.
func (c *Connection) RemoveSelected() bool {
	if !c.CanRemove() {
		return false
	}
	wasSetup := c.SetupRequired()
	id := c.selectedNetworkID
	removedIndex := c.storedIndex(id)

	c.ctrl.DiscardSession(id)
	c.ctrl.ForgetNetworkState(id)
	delete(c.passwords, id)
	delete(c.nickServPasswords, id)
	delete(c.applied, id)

	c.stored = slices.DeleteFunc(c.stored, func(profile NetworkProfile) bool {
		return profile.NetworkID == id
	})
	if len(c.stored) > 0 {
		nextIndex := min(removedIndex, len(c.stored)-1)
		if nextIndex < 0 {
			nextIndex = 0
		}
		c.draft = c.stored[nextIndex]
		c.selectedNetworkID = c.stored[nextIndex].NetworkID
	} else {
		c.draft = SuggestedProfile()
		c.selectedNetworkID = c.draft.NetworkID
		c.draft.EnsureIconColor(c.usedIconColors(c.draft.NetworkID))
	}
	c.draftChanged()
	c.networksChanged()
	if wasSetup != c.SetupRequired() {
		c.setupChanged()
	}
	return true
}

// DisconnectSelected asks the controller to stop the selected network's
// session. It reports whether the controller accepted the request.
func (c *Connection) DisconnectSelected() bool {
	return c.ctrl.Disconnect(c.selectedNetworkID)
}

// MoveNetwork moves a stored network by delta places. It reports whether the
// move was in range.
func (c *Connection) MoveNetwork(networkID string, delta int) bool {
	if delta == 0 {
		return false
	}
	index := c.storedIndex(networkID)
	if index < 0 {
		return false
	}
	next := index + delta
	if next < 0 || next >= len(c.stored) {
		return false
	}
	profile := c.stored[index]
	c.stored = slices.Delete(c.stored, index, index+1)
	c.stored = slices.Insert(c.stored, next, profile)
	c.networksChanged()
	return true
}

// Apply normalizes the draft, stores it, and reconciles its session. It reports
// whether a live session was reached. An incomplete draft is normalized in
// place and rejected.
func (c *Connection) Apply() bool {
	wasSetup := c.SetupRequired()
	c.draft.EnsureIconColor(c.usedIconColors(c.draft.NetworkID))
	profile := c.draft.Normalized()
	if !profile.IsComplete() {
		c.draft = profile
		c.draftChanged()
		return false
	}

	found := false
	for index := range c.stored {
		if c.stored[index].NetworkID == profile.NetworkID {
			c.stored[index] = profile
			found = true
			break
		}
	}
	if !found {
		c.stored = append(c.stored, profile)
	}
	c.draft = profile
	c.selectedNetworkID = profile.NetworkID
	c.draftChanged()
	c.networksChanged()
	if wasSetup != c.SetupRequired() {
		c.setupChanged()
	}
	return c.reconcile(profile)
}

// --- Preferences ----------------------------------------------------------

// ReopenDirects reports whether direct messages reopen at startup.
func (c *Connection) ReopenDirects() bool { return c.reopenDirects }

// SetReopenDirects replaces the reopen-directs preference.
func (c *Connection) SetReopenDirects(enabled bool) {
	c.reopenDirects = enabled
}

// ShowAvatars reports whether avatars are shown.
func (c *Connection) ShowAvatars() bool { return c.showAvatars }

// SetShowAvatars replaces the avatar preference.
func (c *Connection) SetShowAvatars(enabled bool) {
	c.showAvatars = enabled
}

// OpenAtUnread reports whether the shell opens conversations at unread starts.
func (c *Connection) OpenAtUnread() bool { return c.openAtUnread }

// SetOpenAtUnread replaces the open-at-unread preference.
func (c *Connection) SetOpenAtUnread(enabled bool) {
	c.openAtUnread = enabled
}

// --- Internals ------------------------------------------------------------

// reconcile builds the session config for profile and brings the matching
// session live. An unchanged signature restarts the existing session; any
// change discards and rebuilds it. It mirrors IrcConnection::reconcile minus
// persistence and the credential round trip.
func (c *Connection) reconcile(profile NetworkProfile) bool {
	candidate := appliedSession{
		profile:          profile,
		password:         c.passwords[profile.NetworkID],
		nickServPassword: c.nickServPasswords[profile.NetworkID],
	}
	if applied, ok := c.applied[profile.NetworkID]; ok &&
		profilesEqual(applied.profile, candidate.profile) &&
		applied.password == candidate.password &&
		applied.nickServPassword == candidate.nickServPassword &&
		c.ctrl.Session(profile.NetworkID) != nil {
		return c.ctrl.Start(profile.NetworkID)
	}

	c.ctrl.DiscardSession(profile.NetworkID)
	config := session.DefaultSessionConfig(profile.NetworkID, profile.Name, profile.Host, profile.Nick)
	config.Port = profile.Port
	config.TLSEnabled = profile.TLSEnabled
	if profile.Username == "" {
		config.Username = profile.Nick
	} else {
		config.Username = profile.Username
	}
	if profile.Realname == "" {
		config.Realname = profile.Nick
	} else {
		config.Realname = profile.Realname
	}
	config.Password = candidate.password
	config.NickServPassword = candidate.nickServPassword
	config.SASLAccount = profile.SASLAccount()
	config.AutojoinChannels = profile.AutojoinChannels
	config.AutojoinKeys = profile.AutojoinKeys

	if c.factory == nil {
		return false
	}
	transport := c.factory()
	if transport == nil {
		return false
	}
	if _, err := c.ctrl.AddSession(config, transport, nil); err != nil {
		return false
	}
	c.applied[profile.NetworkID] = candidate
	return c.ctrl.Start(profile.NetworkID)
}

// rosterDisplayName composes the roster label for profile, disambiguating a
// duplicate resolved label with the nick. It mirrors
// IrcConnection::rosterDisplayName.
func (c *Connection) rosterDisplayName(profile NetworkProfile) string {
	others := make([]string, 0, len(c.stored))
	for _, stored := range c.stored {
		if stored.NetworkID == profile.NetworkID {
			continue
		}
		others = append(others, stored.ResolvedName())
	}
	return irc.RosterDisplayName(profile.ResolvedName(), profile.Nick, others)
}

func (c *Connection) selectStored(networkID string) {
	if stored, ok := c.storedProfile(networkID); ok {
		c.draft = stored
	}
	c.selectedNetworkID = networkID
	c.draftChanged()
	c.networksChanged()
}

func (c *Connection) isStored(networkID string) bool {
	return c.storedIndex(networkID) >= 0
}

func (c *Connection) storedIndex(networkID string) int {
	if networkID == "" {
		return -1
	}
	for index, profile := range c.stored {
		if profile.NetworkID == networkID {
			return index
		}
	}
	return -1
}

func (c *Connection) storedProfile(networkID string) (NetworkProfile, bool) {
	index := c.storedIndex(networkID)
	if index < 0 {
		return NetworkProfile{}, false
	}
	return c.stored[index], true
}

func (c *Connection) usedIconColors(exceptID string) []int {
	used := make([]int, 0, len(c.stored))
	for _, profile := range c.stored {
		if profile.NetworkID == exceptID {
			continue
		}
		if profile.IconColor >= 0 && profile.IconColor < IconColorCount {
			used = append(used, profile.IconColor)
		}
	}
	return used
}

func (c *Connection) assignStoredIconColors() {
	for index := range c.stored {
		c.stored[index].EnsureIconColor(c.usedIconColors(c.stored[index].NetworkID))
	}
}

func (c *Connection) draftChanged() {
	if c.OnDraftChanged != nil {
		c.OnDraftChanged()
	}
}

func (c *Connection) networksChanged() {
	if c.OnNetworksChanged != nil {
		c.OnNetworksChanged()
	}
}

func (c *Connection) setupChanged() {
	if c.OnSetupRequiredChanged != nil {
		c.OnSetupRequiredChanged()
	}
}

// cloneProfiles copies the list and its nested slices and maps so a caller
// cannot alias the connection's stored state.
func cloneProfiles(profiles []NetworkProfile) []NetworkProfile {
	cloned := make([]NetworkProfile, len(profiles))
	for index, profile := range profiles {
		profile.AutojoinChannels = slices.Clone(profile.AutojoinChannels)
		profile.AutojoinKeys = maps.Clone(profile.AutojoinKeys)
		cloned[index] = profile
	}
	return cloned
}
