package connection

import (
	"maps"
	"slices"
	"strconv"
	"strings"

	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/irc"
	"github.com/fredimachado/omairc/tui/internal/session"
	"github.com/fredimachado/omairc/tui/internal/storage"
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

// draftSecret is the in-memory secret state for one network's password or
// NickServ slot. It is the phase-11 simplification of C++ IrcDraftSecret: the
// held password, whether the user edited it this sheet session, whether the
// stored profile declares it saved, and the credential state. The
// revision/obsolete-key machinery is deliberately dropped because the Go
// credential store is synchronous: a rename just removes the old key and
// writes the new one inside Apply.
type draftSecret struct {
	password string
	edited   bool
	saved    bool
	state    storage.CredentialState
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
// non-persistent half of IrcConnection, plus the phase-11 profile store and
// credential-store wiring. It is not safe for concurrent use.
type Connection struct {
	// OnDraftChanged fires whenever the shown draft or selection changes.
	// Nil-able and invoked synchronously.
	OnDraftChanged func()
	// OnNetworksChanged fires whenever the roster rows change. Nil-able.
	OnNetworksChanged func()
	// OnSetupRequiredChanged fires when SetupRequired flips. Nil-able.
	OnSetupRequiredChanged func()
	// OnPersistenceChanged fires when PersistenceStatus changes. Nil-able.
	OnPersistenceChanged func()
	// OnCredentialChanged fires when the selected draft's credential state or
	// status changes. Nil-able.
	OnCredentialChanged func()

	ctrl    *controller.Controller
	factory TransportFactory

	profiles    *storage.ProfileStore
	credentials storage.CredentialStore
	ephemeral   bool

	stored            []NetworkProfile
	draft             NetworkProfile
	selectedNetworkID string
	addedFrom         string

	secrets         map[string]*draftSecret
	nickServSecrets map[string]*draftSecret
	applied         map[string]appliedSession

	persistenceMessages map[string]string

	// lastPersistence and lastCredential remember the last published value so
	// an OnPersistenceChanged / OnCredentialChanged wake-up only fires when the
	// shown sentence actually moved.
	lastPersistence string
	lastCredential  string

	// storeUnavailable latches the first backend Unavailable result so a later
	// held password overlays to SessionOnly without another backend call.
	storeUnavailable bool
}

// New builds a connection over ctrl. With no stored profiles the draft is the
// suggested Freenode profile and selectedNetworkId points at it. Seed persisted
// profiles with SetProfileStore or SetStoredProfiles.
func New(ctrl *controller.Controller, factory TransportFactory) *Connection {
	connection := &Connection{
		ctrl:                ctrl,
		factory:             factory,
		secrets:             make(map[string]*draftSecret),
		nickServSecrets:     make(map[string]*draftSecret),
		applied:             make(map[string]appliedSession),
		persistenceMessages: make(map[string]string),
	}
	connection.draft = SuggestedProfile()
	connection.selectedNetworkID = connection.draft.NetworkID
	connection.draft.EnsureIconColor(connection.usedIconColors(connection.draft.NetworkID))
	return connection
}

// SetProfileStore installs the persisted profile store and seeds the stored
// list from it. A nil store clears persistence (the profile list is left
// alone). It mirrors IrcConnection::loadStored.
func (c *Connection) SetProfileStore(store *storage.ProfileStore) {
	c.profiles = store
	if store == nil {
		return
	}
	persisted := store.Profiles()
	profiles := make([]NetworkProfile, 0, len(persisted))
	for _, profile := range persisted {
		profiles = append(profiles, profileFromStorage(profile))
	}
	c.SetStoredProfiles(profiles)
}

// SetCredentialStore installs the credential backend (nil means session-only)
// and reads the stored secrets for the seeded profiles. An unavailable backend
// latches so a held secret overlays to SessionOnly.
func (c *Connection) SetCredentialStore(store storage.CredentialStore) {
	c.credentials = store
	if store == nil {
		c.markHeldSecretsSessionOnly()
		c.draftChanged()
		return
	}
	for index := range c.stored {
		c.readStoredSecret(c.stored[index], false)
		c.readStoredSecret(c.stored[index], true)
	}
	c.draftChanged()
}

// SetEphemeral disables all persistence: it nils both stores, keeps secrets in
// memory only, and forces the credential state of a held secret to SessionOnly.
func (c *Connection) SetEphemeral(ephemeral bool) {
	c.ephemeral = ephemeral
	if !ephemeral {
		return
	}
	c.profiles = nil
	c.credentials = nil
	c.storeUnavailable = false
	c.markHeldSecretsSessionOnly()
	c.draftChanged()
}

// SetStoredProfiles seeds the stored list, mirroring
// IrcConnection::setStoredProfiles. It selects the first complete profile,
// else the first; with an empty list it falls back to the suggested profile.
// The declared secrets of every seeded profile are read synchronously when a
// credential store is installed.
func (c *Connection) SetStoredProfiles(profiles []NetworkProfile) {
	wasSetup := c.SetupRequired()
	c.stored = cloneProfiles(profiles)
	c.assignStoredIconColors()
	c.seedStoredSecrets()
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

// SetPassword replaces the selected network's in-memory password and marks the
// draft secret edited so Apply persists it. An empty value is refused unless a
// stored secret exists to clear, mirroring IrcConnection::setPassword.
func (c *Connection) SetPassword(password string) {
	secret := c.passwordSecret(c.selectedNetworkID)
	if password == "" {
		if !secret.saved {
			return
		}
		if secret.password == "" && secret.edited {
			return
		}
		secret.password = ""
		secret.edited = true
		c.markSessionOnlyIfStoreUnavailable(secret, c.nickServSecret(c.selectedNetworkID))
		if secret.state != storage.CredentialSessionOnly {
			secret.state = storage.CredentialMissing
		}
		c.draftChanged()
		return
	}
	if secret.password == password {
		return
	}
	secret.password = password
	secret.edited = true
	c.markSessionOnlyIfStoreUnavailable(secret, c.nickServSecret(c.selectedNetworkID))
	c.draftChanged()
}

// SetNickServPassword replaces the selected network's in-memory NickServ
// password and marks the draft secret edited so Apply persists it. An empty
// value is refused unless a stored secret exists to clear.
func (c *Connection) SetNickServPassword(password string) {
	secret := c.nickServSecret(c.selectedNetworkID)
	if password == "" {
		if !secret.saved {
			return
		}
		if secret.password == "" && secret.edited {
			return
		}
		secret.password = ""
		secret.edited = true
		c.markSessionOnlyIfStoreUnavailable(secret, c.passwordSecret(c.selectedNetworkID))
		if secret.state != storage.CredentialSessionOnly {
			secret.state = storage.CredentialMissing
		}
		c.draftChanged()
		return
	}
	if secret.password == password {
		return
	}
	secret.password = password
	secret.edited = true
	c.markSessionOnlyIfStoreUnavailable(secret, c.passwordSecret(c.selectedNetworkID))
	c.draftChanged()
}

// PasswordSet reports whether the selected network has an in-memory password.
func (c *Connection) PasswordSet() bool {
	return c.passwordSecret(c.selectedNetworkID).password != ""
}

// NickServSet reports whether the selected network has an in-memory NickServ
// password.
func (c *Connection) NickServSet() bool {
	return c.nickServSecret(c.selectedNetworkID).password != ""
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
		delete(c.secrets, dropped)
		delete(c.nickServSecrets, dropped)
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
// was removed. A read-only or malformed settings file leaves the network in
// place and records the persistence sentence, mirroring
// IrcConnection::removeSelected.
func (c *Connection) RemoveSelected() bool {
	if !c.CanRemove() {
		return false
	}
	id := c.selectedNetworkID
	removed, hadStored := c.storedProfile(id)

	if c.profiles != nil && !c.ephemeral {
		status := c.profiles.Remove(id)
		if status == storage.StatusAccessError || status == storage.StatusFormatError {
			c.assignPersistenceStatus(id,
				"This network could not be removed. "+settingsFileSentence(status))
			return false
		}
		if status == storage.StatusWritten {
			delete(c.persistenceMessages, id)
		}
	}

	wasSetup := c.SetupRequired()
	removedIndex := c.storedIndex(id)

	c.ctrl.DiscardSession(id)
	c.ctrl.ForgetNetworkState(id)
	if hadStored && c.credentials != nil && !c.ephemeral {
		c.credentials.Remove(credentialKeyFor(removed, false))
		c.credentials.Remove(credentialKeyFor(removed, true))
	}
	delete(c.secrets, id)
	delete(c.nickServSecrets, id)
	delete(c.applied, id)
	delete(c.persistenceMessages, id)

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
// move was in range. The controller owns the persisted sidebar order now, so
// this only reorders the in-memory roster.
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

// Apply normalizes the draft, persists the profile and its edited secrets,
// stores it, and reconciles its session. It reports whether a live session was
// reached. An incomplete draft is normalized in place and rejected.
func (c *Connection) Apply() bool {
	wasSetup := c.SetupRequired()
	c.draft.EnsureIconColor(c.usedIconColors(c.draft.NetworkID))
	profile := c.draft.Normalized()
	if !profile.IsComplete() {
		c.draft = profile
		c.draftChanged()
		return false
	}

	previousProfile, hadPrevious := c.storedProfile(profile.NetworkID)
	password := c.passwordSecret(profile.NetworkID)
	nickServ := c.nickServSecret(profile.NetworkID)

	c.applySecret(profile, previousProfile, hadPrevious, password, false)
	c.applySecret(profile, previousProfile, hadPrevious, nickServ, true)
	profile.SecretSaved = password.saved
	profile.NickServSaved = nickServ.saved

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

	var sessionAccepted bool
	if c.profiles != nil && !c.ephemeral {
		status := c.profiles.Save(storageProfile(profile))
		sessionAccepted = c.reconcile(profile)
		c.recordApplyPersistence(profile.NetworkID, status, sessionAccepted)
	} else {
		sessionAccepted = c.reconcile(profile)
	}

	password.edited = false
	nickServ.edited = false
	c.notifyPersistence()
	c.notifyCredential()
	return sessionAccepted
}

// --- Preferences ----------------------------------------------------------

// The Preferences tab and /pref share one source of truth: the controller's
// preference store. The connection only forwards the reads and writes and
// repaints its sheet, so the Connect sheet and /pref can never disagree.

// ReopenDirects reports whether direct messages reopen at startup.
func (c *Connection) ReopenDirects() bool { return c.ctrl.ReopenDirects() }

// SetReopenDirects replaces the reopen-directs preference.
func (c *Connection) SetReopenDirects(enabled bool) {
	c.ctrl.SetReopenDirects(enabled)
	c.draftChanged()
}

// ShowAvatars reports whether avatars are shown.
func (c *Connection) ShowAvatars() bool { return c.ctrl.ShowAvatars() }

// SetShowAvatars replaces the avatar preference.
func (c *Connection) SetShowAvatars(enabled bool) {
	c.ctrl.SetShowAvatars(enabled)
	c.draftChanged()
}

// OpenAtUnread reports whether the shell opens conversations at unread starts.
func (c *Connection) OpenAtUnread() bool { return c.ctrl.OpenAtUnread() }

// SetOpenAtUnread replaces the open-at-unread preference.
func (c *Connection) SetOpenAtUnread(enabled bool) {
	c.ctrl.SetOpenAtUnread(enabled)
	c.draftChanged()
}

// --- Persistence and credentials ------------------------------------------

// PersistenceStatus is the selected network's persistence sentence, or "".
// It mirrors IrcConnection::persistenceStatus.
func (c *Connection) PersistenceStatus() string {
	return c.persistenceMessages[c.selectedNetworkID]
}

// CredentialState is the selected draft's combined credential state: the
// password slot's state after the NickServ slot's availability is folded in.
func (c *Connection) CredentialState() storage.CredentialState {
	return c.passwordSecret(c.selectedNetworkID).state
}

// CredentialStatus is the selected draft's credential sentence, or "". It
// mirrors IrcConnection::credentialStatus.
func (c *Connection) CredentialStatus() string {
	password := c.passwordSecret(c.selectedNetworkID)
	nickServ := c.nickServSecret(c.selectedNetworkID)
	left := secretStatus(password, "password")
	right := ""
	if !secretIsIdle(nickServ) {
		right = secretStatus(nickServ, "NickServ")
	}
	if right == "" {
		return left
	}
	shownLeft := ""
	if !secretIsIdle(password) {
		shownLeft = left
	}
	if shownLeft == "" {
		return right
	}
	if shownLeft == right {
		return shownLeft
	}
	if password.password != "" && nickServ.password != "" &&
		(password.state == storage.CredentialUnavailable ||
			password.state == storage.CredentialSessionOnly) &&
		(nickServ.state == storage.CredentialUnavailable ||
			nickServ.state == storage.CredentialSessionOnly) {
		return "secure storage unavailable; password and NickServ are session-only"
	}
	return shownLeft + "; " + right
}

// CanForgetPassword reports whether there is a stored or held password to
// forget.
func (c *Connection) CanForgetPassword() bool {
	secret := c.passwordSecret(c.selectedNetworkID)
	return secret.saved || secret.password != ""
}

// CanForgetNickServ reports whether there is a stored or held NickServ secret
// to forget.
func (c *Connection) CanForgetNickServ() bool {
	secret := c.nickServSecret(c.selectedNetworkID)
	return secret.saved || secret.password != ""
}

// ForgetPassword removes the selected network's stored password (when a
// backend is installed), clears the held value and the saved flag, and
// persists the profile. It is a no-op when there is nothing to forget.
func (c *Connection) ForgetPassword() {
	if !c.CanForgetPassword() {
		return
	}
	c.forgetSecret(false)
}

// ForgetNickServ removes the selected network's stored NickServ secret (when a
// backend is installed), clears the held value and the saved flag, and
// persists the profile. It is a no-op when there is nothing to forget.
func (c *Connection) ForgetNickServ() {
	if !c.CanForgetNickServ() {
		return
	}
	c.forgetSecret(true)
}

// ActivateStartup reconciles every complete, connectOnStartup stored profile
// whose declared secrets are readable (or already held in memory). It returns
// how many sessions it started. It mirrors IrcConnection::activateStartup
// without the asynchronous read wait: the Go credential store is synchronous,
// so this is safe to call once stores are installed.
func (c *Connection) ActivateStartup() int {
	started := 0
	for index := range c.stored {
		profile := c.stored[index]
		if !profile.ConnectOnStartup || !profile.IsComplete() {
			continue
		}
		c.readStoredSecret(profile, false)
		c.readStoredSecret(profile, true)
		if c.secretUnreadable(c.passwordSecret(profile.NetworkID), profile.SecretSaved, true) {
			continue
		}
		if c.secretUnreadable(c.nickServSecret(profile.NetworkID), profile.NickServSaved, false) {
			continue
		}
		if c.reconcile(profile) {
			started++
		}
	}
	return started
}

// PersistAvatarURL saves the network's standing avatar URL into its stored
// profile. It mirrors IrcConnection::persistAvatarUrl.
func (c *Connection) PersistAvatarURL(networkID, url string) {
	if networkID == "" {
		return
	}
	index := c.storedIndex(networkID)
	if index < 0 || c.stored[index].AvatarURL == url {
		return
	}
	previous := c.PersistenceStatus()
	c.stored[index].AvatarURL = url
	if applied, ok := c.applied[networkID]; ok {
		applied.profile.AvatarURL = url
		c.applied[networkID] = applied
	}
	if c.draft.NetworkID == networkID {
		c.draft.AvatarURL = url
	}
	if c.profiles != nil && !c.ephemeral {
		c.recordBackgroundSave(networkID, c.profiles.Save(storageProfile(c.stored[index])))
	}
	c.publishPersistence(previous)
}

// StoredAvatarURL returns the stored profile's avatar URL, or "".
func (c *Connection) StoredAvatarURL(networkID string) string {
	if profile, ok := c.storedProfile(networkID); ok {
		return profile.AvatarURL
	}
	return ""
}

// PersistAutojoin saves the network's autojoin channels and keys into its
// stored profile. It mirrors IrcConnection::persistAutojoin.
func (c *Connection) PersistAutojoin(networkID string, channels []string, keys map[string]string) {
	index := c.storedIndex(networkID)
	if index < 0 {
		return
	}
	if slices.Equal(c.stored[index].AutojoinChannels, channels) &&
		maps.Equal(c.stored[index].AutojoinKeys, keys) {
		return
	}
	previous := c.PersistenceStatus()
	c.stored[index].AutojoinChannels = slices.Clone(channels)
	c.stored[index].AutojoinKeys = maps.Clone(keys)
	if applied, ok := c.applied[networkID]; ok {
		applied.profile.AutojoinChannels = slices.Clone(channels)
		applied.profile.AutojoinKeys = maps.Clone(keys)
		c.applied[networkID] = applied
	}
	if c.draft.NetworkID == networkID {
		c.draft.AutojoinChannels = slices.Clone(channels)
		c.draft.AutojoinKeys = maps.Clone(keys)
	}
	if c.profiles != nil && !c.ephemeral {
		c.recordBackgroundSave(networkID, c.profiles.Save(storageProfile(c.stored[index])))
	}
	c.publishPersistence(previous)
	c.draftChanged()
}

// applySecret performs the synchronous credential write/remove for one edited
// draft secret during Apply. A rename/rekey removes the previous key before
// writing the new one. When the draft was not edited the profile flag is left
// as it was.
func (c *Connection) applySecret(profile, previousProfile NetworkProfile, hadPrevious bool, secret *draftSecret, nickServ bool) {
	nextKey := credentialKeyFor(profile, nickServ)
	if hadPrevious {
		previousKey := credentialKeyFor(previousProfile, nickServ)
		if previousKey != nextKey && (secret.saved || secret.password != "") &&
			c.credentials != nil && !c.ephemeral {
			c.noteCredentialResult(c.credentials.Remove(previousKey))
		}
	}
	if !secret.edited {
		return
	}
	if c.credentials == nil || c.ephemeral {
		secret.saved = false
		if secret.password != "" {
			secret.state = storage.CredentialSessionOnly
		} else {
			secret.state = storage.CredentialMissing
		}
		return
	}
	if secret.password != "" {
		result := c.credentials.Write(nextKey, secret.password)
		c.noteCredentialResult(result)
		if result.State == storage.CredentialAvailable {
			secret.saved = true
			secret.state = storage.CredentialAvailable
			return
		}
		secret.state = overlayState(result.State, secret.password)
		return
	}
	result := c.credentials.Remove(nextKey)
	c.noteCredentialResult(result)
	secret.saved = false
	if result.State == storage.CredentialAvailable || result.State == storage.CredentialMissing {
		secret.state = storage.CredentialMissing
		return
	}
	secret.state = overlayState(result.State, "")
}

// forgetSecret clears one selected secret, removes it from the backend when one
// is installed, and persists the profile's saved flag.
func (c *Connection) forgetSecret(nickServ bool) {
	id := c.selectedNetworkID
	secret := c.secretFor(id, nickServ)
	profile := c.draft
	if stored, ok := c.storedProfile(id); ok {
		profile = stored
	}
	secret.password = ""
	secret.edited = false
	secret.saved = false
	secret.state = storage.CredentialMissing
	if c.credentials != nil && !c.ephemeral && profile.NetworkID != "" {
		result := c.credentials.Remove(credentialKeyFor(profile, nickServ))
		c.noteCredentialResult(result)
		if result.State != storage.CredentialAvailable && result.State != storage.CredentialMissing {
			secret.state = overlayState(result.State, "")
		}
	}
	c.persistSavedFlag(id, nickServ, false)
	c.draftChanged()
}

// seedStoredSecrets resets every stored profile's secret slots from the
// profile's declared flags and reads the declared ones when a backend is
// installed.
func (c *Connection) seedStoredSecrets() {
	c.storeUnavailable = false
	for index := range c.stored {
		profile := c.stored[index]
		secret := c.secretFor(profile.NetworkID, false)
		*secret = draftSecret{state: storage.CredentialMissing, saved: profile.SecretSaved}
		secret = c.secretFor(profile.NetworkID, true)
		*secret = draftSecret{state: storage.CredentialMissing, saved: profile.NickServSaved}
		if profile.SecretSaved {
			c.readStoredSecret(profile, false)
		}
		if profile.NickServSaved {
			c.readStoredSecret(profile, true)
		}
	}
}

// readStoredSecret synchronously reads one declared-but-unheld secret. It is a
// no-op when the profile does not declare the secret saved, when no backend is
// installed, or when the value is already held. An Unavailable result never
// clears the saved flag: the profile still says saved.
func (c *Connection) readStoredSecret(profile NetworkProfile, nickServ bool) {
	saved := profile.SecretSaved
	if nickServ {
		saved = profile.NickServSaved
	}
	secret := c.secretFor(profile.NetworkID, nickServ)
	secret.saved = saved
	if !saved || c.credentials == nil || c.ephemeral || secret.password != "" {
		return
	}
	result := c.credentials.Read(credentialKeyFor(profile, nickServ))
	c.noteCredentialResult(result)
	if result.State == storage.CredentialAvailable {
		secret.password = result.Password
		secret.state = storage.CredentialAvailable
		return
	}
	secret.state = result.State
}

// markHeldSecretsSessionOnly forces every held secret to SessionOnly.
func (c *Connection) markHeldSecretsSessionOnly() {
	for _, secret := range c.secrets {
		if secret.password != "" {
			secret.state = storage.CredentialSessionOnly
		}
	}
	for _, secret := range c.nickServSecrets {
		if secret.password != "" {
			secret.state = storage.CredentialSessionOnly
		}
	}
}

// markSessionOnlyIfStoreUnavailable overlays SessionOnly when the backend is
// unavailable, when the store is known down, or when the other slot is already
// session-only. It mirrors IrcConnection::markSessionOnlyIfStoreUnavailable.
func (c *Connection) markSessionOnlyIfStoreUnavailable(secret, other *draftSecret) {
	if c.credentials == nil || c.storeUnavailable ||
		secret.state == storage.CredentialUnavailable ||
		other.state == storage.CredentialUnavailable ||
		other.state == storage.CredentialSessionOnly {
		secret.state = storage.CredentialSessionOnly
	}
}

// noteCredentialResult latches an Unavailable backend so a later held secret
// overlays to SessionOnly without another call.
func (c *Connection) noteCredentialResult(result storage.CredentialResult) {
	if result.State == storage.CredentialUnavailable {
		c.storeUnavailable = true
	}
}

// persistSavedFlag writes one saved flag into c.stored (and c.draft when it is
// the selected network) and saves the profile when a store is installed. It
// mirrors IrcConnection::persistSavedFlag.
func (c *Connection) persistSavedFlag(networkID string, nickServ bool, saved bool) {
	index := c.storedIndex(networkID)
	if index < 0 {
		return
	}
	if nickServ {
		c.stored[index].NickServSaved = saved
	} else {
		c.stored[index].SecretSaved = saved
	}
	if c.draft.NetworkID == networkID {
		if nickServ {
			c.draft.NickServSaved = saved
		} else {
			c.draft.SecretSaved = saved
		}
	}
	if c.profiles != nil && !c.ephemeral {
		c.recordBackgroundSave(networkID, c.profiles.Save(storageProfile(c.stored[index])))
	}
}

// recordBackgroundSave records a background (non-Apply) persistence outcome.
// It mirrors IrcConnection::recordBackgroundSave.
func (c *Connection) recordBackgroundSave(networkID string, status storage.Status) {
	if networkID == "" || status == storage.StatusAbsent {
		return
	}
	previous := c.PersistenceStatus()
	if status == storage.StatusWritten {
		delete(c.persistenceMessages, networkID)
		c.publishPersistence(previous)
		return
	}
	if sentence := settingsFileSentence(status); sentence != "" {
		c.persistenceMessages[networkID] = sentence
		c.publishPersistence(previous)
	}
}

// recordApplyPersistence records the persistence sentence after Apply. It
// mirrors IrcConnection::recordApplyPersistence.
func (c *Connection) recordApplyPersistence(networkID string, status storage.Status, sessionAccepted bool) {
	if networkID == "" || status == storage.StatusAbsent {
		return
	}
	if status == storage.StatusWritten && sessionAccepted {
		delete(c.persistenceMessages, networkID)
		return
	}
	if status == storage.StatusWritten {
		c.persistenceMessages[networkID] = "These settings were saved successfully."
		return
	}
	sentence := settingsFileSentence(status)
	if sentence == "" {
		return
	}
	if sessionAccepted {
		c.persistenceMessages[networkID] = "Connected using these settings. " + sentence
		return
	}
	c.persistenceMessages[networkID] = sentence
}

// assignPersistenceStatus stores one sentence and publishes the change.
func (c *Connection) assignPersistenceStatus(networkID, message string) {
	previous := c.PersistenceStatus()
	c.persistenceMessages[networkID] = message
	c.publishPersistence(previous)
}

// publishPersistence fires OnPersistenceChanged when the shown sentence moved.
func (c *Connection) publishPersistence(previous string) {
	if c.PersistenceStatus() == previous {
		return
	}
	c.lastPersistence = c.PersistenceStatus()
	if c.OnPersistenceChanged != nil {
		c.OnPersistenceChanged()
	}
}

// publishCredential fires OnCredentialChanged when the shown state or sentence
// moved.
func (c *Connection) publishCredential() {
	snapshot := strconv.Itoa(int(c.CredentialState())) + "\x00" + c.CredentialStatus()
	if snapshot == c.lastCredential {
		return
	}
	c.lastCredential = snapshot
	if c.OnCredentialChanged != nil {
		c.OnCredentialChanged()
	}
}

// notifyCredential compares the current selected-draft credential snapshot to
// the last published one and fires when it moved.
func (c *Connection) notifyCredential() {
	c.publishCredential()
}

// settingsFileSentence renders the failure sentence for a store status. It
// mirrors the anonymous settingsFileSentence in ircconnection.cpp.
func settingsFileSentence(status storage.Status) string {
	switch status {
	case storage.StatusAccessError:
		return "The settings file could not be written."
	case storage.StatusFormatError:
		return "The settings file could not be read."
	}
	return ""
}

// secretIsIdle reports whether a secret slot carries no state at all. It
// mirrors IrcConnection::secretIsIdle without the obsolete-key fields.
func secretIsIdle(secret *draftSecret) bool {
	return secret.password == "" && !secret.edited && !secret.saved
}

// secretStatus renders one secret slot's sentence. It mirrors
// IrcConnection::secretStatus.
func secretStatus(secret *draftSecret, noun string) string {
	if secret.edited && secret.password != "" &&
		(secret.state == storage.CredentialAvailable || secret.state == storage.CredentialMissing) {
		return noun + " changed; apply to save securely"
	}
	switch secret.state {
	case storage.CredentialLoading:
		return "checking secure storage"
	case storage.CredentialAvailable:
		return noun + " saved securely"
	case storage.CredentialMissing:
		if secret.password != "" {
			return noun + " is session-only until applied"
		}
		return ""
	case storage.CredentialUnavailable:
		if secret.password != "" {
			return "secure storage unavailable; " + noun + " is session-only"
		}
		return "secure storage unavailable"
	case storage.CredentialError:
		if secret.password != "" {
			return "secure storage error; " + noun + " is session-only"
		}
		return "secure storage error; " + noun + " is not saved"
	case storage.CredentialSessionOnly:
		return "secure storage unavailable; " + noun + " is session-only"
	}
	return ""
}

// overlayState folds a held password into an Unavailable backend state. It
// mirrors IrcConnection::overlayState.
func overlayState(backend storage.CredentialState, password string) storage.CredentialState {
	if backend == storage.CredentialUnavailable && password != "" {
		return storage.CredentialSessionOnly
	}
	return backend
}

// secretSlotTracked reports whether a secret slot has state worth waiting on.
// It mirrors IrcConnection::secretSlotTracked.
func (c *Connection) secretSlotTracked(secret *draftSecret, saved bool) bool {
	return saved || secret.saved || secret.edited || secret.password != ""
}

// secretUnreadable ports IrcConnection::secretUnreadable: a tracked slot that
// is still loading or errored blocks, and a declared-but-unreadable saved slot
// blocks.
func (c *Connection) secretUnreadable(secret *draftSecret, saved, required bool) bool {
	tracked := required || c.secretSlotTracked(secret, saved)
	if secret.state == storage.CredentialLoading || secret.state == storage.CredentialError {
		return tracked
	}
	return saved &&
		(secret.state == storage.CredentialMissing ||
			secret.state == storage.CredentialUnavailable ||
			secret.state == storage.CredentialSessionOnly)
}

// credentialKeyFor builds the credential key for one profile slot:
// {NetworkID, Username or Nick, Host}, with purpose "nickserv" for the
// NickServ slot. It mirrors IrcConnection::credentialKey.
func credentialKeyFor(profile NetworkProfile, nickServ bool) storage.CredentialKey {
	username := profile.Username
	if username == "" {
		username = profile.Nick
	}
	key := storage.CredentialKey{
		NetworkID: profile.NetworkID,
		Username:  username,
		Host:      profile.Host,
	}
	if nickServ {
		key.Purpose = "nickserv"
	}
	return key
}

// profileFromStorage converts a persisted profile into the connection model.
func profileFromStorage(profile storage.Profile) NetworkProfile {
	return NetworkProfile{
		NetworkID:        profile.NetworkID,
		Name:             profile.Name,
		Host:             profile.Host,
		Port:             profile.Port,
		TLSEnabled:       profile.TLSEnabled,
		ConnectOnStartup: profile.ConnectOnStartup,
		SecretSaved:      profile.SecretSaved,
		NickServSaved:    profile.NickServSaved,
		Nick:             profile.Nick,
		Username:         profile.Username,
		Realname:         profile.Realname,
		Account:          profile.Account,
		BouncerNetwork:   profile.BouncerNetwork,
		AutojoinChannels: slices.Clone(profile.AutojoinChannels),
		AutojoinKeys:     maps.Clone(profile.AutojoinKeys),
		IconColor:        profile.IconColor,
		AvatarURL:        profile.AvatarURL,
	}
}

// storageProfile converts a connection profile into the persisted shape.
func storageProfile(profile NetworkProfile) storage.Profile {
	return storage.Profile{
		NetworkID:        profile.NetworkID,
		Name:             profile.Name,
		Host:             profile.Host,
		Port:             profile.Port,
		TLSEnabled:       profile.TLSEnabled,
		ConnectOnStartup: profile.ConnectOnStartup,
		SecretSaved:      profile.SecretSaved,
		NickServSaved:    profile.NickServSaved,
		Nick:             profile.Nick,
		Username:         profile.Username,
		Realname:         profile.Realname,
		Account:          profile.Account,
		BouncerNetwork:   profile.BouncerNetwork,
		AutojoinChannels: slices.Clone(profile.AutojoinChannels),
		AutojoinKeys:     maps.Clone(profile.AutojoinKeys),
		IconColor:        profile.IconColor,
		AvatarURL:        profile.AvatarURL,
	}
}

// --- Internals ------------------------------------------------------------

// reconcile builds the session config for profile and brings the matching
// session live. An unchanged signature restarts the existing session; any
// change discards and rebuilds it. It mirrors IrcConnection::reconcile minus
// the asynchronous credential revision bookkeeping.
func (c *Connection) reconcile(profile NetworkProfile) bool {
	password := c.passwordSecret(profile.NetworkID).password
	nickServPassword := c.nickServSecret(profile.NetworkID).password
	candidate := appliedSession{
		profile:          profile,
		password:         password,
		nickServPassword: nickServPassword,
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

func (c *Connection) passwordSecret(networkID string) *draftSecret {
	return c.secretFor(networkID, false)
}

func (c *Connection) nickServSecret(networkID string) *draftSecret {
	return c.secretFor(networkID, true)
}

// secretFor returns the password or NickServ secret for networkID, creating a
// Missing-state slot on first use.
func (c *Connection) secretFor(networkID string, nickServ bool) *draftSecret {
	secrets := c.secrets
	if nickServ {
		secrets = c.nickServSecrets
	}
	if secret, ok := secrets[networkID]; ok {
		return secret
	}
	secret := &draftSecret{state: storage.CredentialMissing}
	secrets[networkID] = secret
	return secret
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
	c.notifyPersistence()
	c.notifyCredential()
	if c.OnDraftChanged != nil {
		c.OnDraftChanged()
	}
}

// notifyPersistence fires OnPersistenceChanged when the shown sentence moved.
func (c *Connection) notifyPersistence() {
	current := c.PersistenceStatus()
	if current == c.lastPersistence {
		return
	}
	c.lastPersistence = current
	if c.OnPersistenceChanged != nil {
		c.OnPersistenceChanged()
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
