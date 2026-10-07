package controller

import (
	"maps"
	"os"
	"slices"
	"testing"

	"github.com/fredimachado/omairc/tui/internal/irc"
	"github.com/fredimachado/omairc/tui/internal/storage"
)

// This file ports the Phase 11 preference and sidebar-order persistence checks
// from tests/session/tst_controller.cpp. Every test points XDG_CONFIG_HOME at a
// temp dir so the real user config is never touched.

func TestLoadStoredPreferencesDefaultsOn(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())

	c := New()
	c.SetEphemeral(false)
	c.LoadStoredPreferences()

	if !c.ReopenDirects() || !c.ShowAvatars() || !c.OpenAtUnread() {
		t.Fatalf("defaults = directs:%v avatars:%v unread:%v, want all true",
			c.ReopenDirects(), c.ShowAvatars(), c.OpenAtUnread())
	}
}

func TestSetReopenDirectsPersistsAndReloads(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())

	c := New()
	c.SetEphemeral(false)
	c.LoadStoredPreferences()
	c.SetReopenDirects(false)

	settings := storage.OpenSettings("")
	if settings.Bool(preferencesGroup, reopenDirectMessagesKey, true) {
		t.Fatalf("%s not persisted false", reopenDirectMessagesKey)
	}

	fresh := New()
	fresh.SetEphemeral(false)
	fresh.LoadStoredPreferences()
	if fresh.ReopenDirects() {
		t.Fatalf("fresh ReopenDirects = true, want false")
	}
	if !fresh.ShowAvatars() || !fresh.OpenAtUnread() {
		t.Fatalf("fresh siblings = avatars:%v unread:%v, want true",
			fresh.ShowAvatars(), fresh.OpenAtUnread())
	}
}

func TestPrefEnabledAndApplyMatchGetters(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())

	c := New()
	c.SetEphemeral(false)

	c.PrefApply(irc.PrefAvatars, false)
	if c.PrefEnabled(irc.PrefAvatars) || c.ShowAvatars() {
		t.Fatalf("PrefApply(avatars, false) did not reach the getter")
	}
	c.PrefApply(irc.PrefUnread, false)
	if c.PrefEnabled(irc.PrefUnread) || c.OpenAtUnread() {
		t.Fatalf("PrefApply(unread, false) did not reach the getter")
	}
	c.PrefApply(irc.PrefDirects, false)
	if c.PrefEnabled(irc.PrefDirects) || c.ReopenDirects() {
		t.Fatalf("PrefApply(directs, false) did not reach the getter")
	}
	c.PrefApply(irc.PrefDirects, true)
	if !c.PrefEnabled(irc.PrefDirects) || !c.ReopenDirects() {
		t.Fatalf("PrefApply(directs, true) did not reach the getter")
	}

	// The /pref dispatcher routes through the host adapter; assert it agrees.
	host := ctrlHost{c: c}
	if host.PrefEnabled(irc.PrefAvatars) != c.ShowAvatars() {
		t.Fatalf("ctrlHost.PrefEnabled disagrees with the controller")
	}
	host.PrefApply(irc.PrefAvatars, true)
	if !c.ShowAvatars() {
		t.Fatalf("ctrlHost.PrefApply did not reach the controller")
	}
}

func TestEphemeralControllerWritesNothing(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())

	c := New() // ephemeral by default
	c.SetReopenDirects(false)
	c.SetShowAvatars(false)
	c.SetOpenAtUnread(false)
	c.SetNetworkOrder([]string{"alpha", "zulu"})
	c.SetNetworkCollapsed("alpha", true)

	if path := storage.ConfigPath(); path != "" {
		if _, err := os.Stat(path); err == nil {
			t.Fatalf("ephemeral controller wrote %s", path)
		}
	}
}

func TestNetworkOrderAndCollapseRoundTrip(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())

	c := New()
	c.SetEphemeral(false)
	addNetwork(t, c, "alpha")
	addNetwork(t, c, "omarchy")
	addNetwork(t, c, "zulu")
	c.SetNetworkOrder([]string{"zulu", "alpha", "omarchy"})
	c.SetNetworkCollapsed("alpha", true)

	fresh := New()
	fresh.SetEphemeral(false)
	addNetwork(t, fresh, "alpha")
	addNetwork(t, fresh, "omarchy")
	addNetwork(t, fresh, "zulu")
	fresh.LoadStoredPreferences()

	if got, want := fresh.NetworkOrder(), []string{"zulu", "alpha", "omarchy"}; !slices.Equal(got, want) {
		t.Fatalf("restored NetworkOrder = %v, want %v", got, want)
	}
	if !fresh.IsNetworkCollapsed("alpha") {
		t.Fatalf("restored collapse lost")
	}
	if fresh.IsNetworkCollapsed("zulu") {
		t.Fatalf("restored collapse invented zulu")
	}
}

func TestLoadStoredSidebarDropsMissingAndSortsLeftovers(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())

	c := New()
	c.SetEphemeral(false)
	// No sessions yet: the collapsed set has nothing to keep.
	c.LoadStoredPreferences()
	if got := c.NetworkOrder(); len(got) != 0 {
		t.Fatalf("NetworkOrder with no sessions = %v, want empty", got)
	}

	// A saved order with a missing id keeps only the registered ids, and the
	// leftovers come back in the profileLess fallback order (nick, here).
	settings := storage.OpenSettings("")
	settings.SetStringList(preferencesGroup, networkOrderKey, []string{"zulu", "gone", "alpha"})
	settings.SetStringList(preferencesGroup, collapsedNetworksKey, []string{"gone", "zulu"})
	if status := settings.Sync(); status != storage.StatusWritten {
		t.Fatalf("Sync = %v", status)
	}

	fresh := New()
	fresh.SetEphemeral(false)
	addNetwork(t, fresh, "zulu")
	addNetwork(t, fresh, "alpha")
	addNetwork(t, fresh, "omarchy")
	fresh.LoadStoredPreferences()

	if got, want := fresh.NetworkOrder(), []string{"zulu", "alpha", "omarchy"}; !slices.Equal(got, want) {
		t.Fatalf("NetworkOrder = %v, want %v", got, want)
	}
	if !fresh.IsNetworkCollapsed("zulu") {
		t.Fatalf("collapsed zulu dropped")
	}
	if fresh.IsNetworkCollapsed("gone") {
		t.Fatalf("collapsed gone survived")
	}
}

func TestPersistenceCallbacksForwardAvatarAndAutojoin(t *testing.T) {
	c := New()

	// Nil callbacks stay a no-op, and the in-memory avatar store still updates.
	ctrlHost{c: c}.PersistAvatarURL("freenode", "https://example.test/a.png")
	if got := c.avatars.URLForNetwork("freenode"); got != "https://example.test/a.png" {
		t.Fatalf("avatars.URLForNetwork = %q, want the persisted url", got)
	}
	c.AutojoinChannelsChanged("freenode", []string{"#go"}, map[string]string{"#go": "secret"})

	var avatarNetwork, avatarURL string
	c.OnAvatarURLChanged = func(networkID, url string) {
		avatarNetwork, avatarURL = networkID, url
	}
	ctrlHost{c: c}.PersistAvatarURL("freenode", "https://example.test/b.png")
	if avatarNetwork != "freenode" || avatarURL != "https://example.test/b.png" {
		t.Fatalf("OnAvatarURLChanged = (%q, %q), want (freenode, ...b.png)", avatarNetwork, avatarURL)
	}

	var autojoinNetwork string
	var autojoinChannels []string
	var autojoinKeys map[string]string
	c.OnAutojoinChanged = func(networkID string, channels []string, keys map[string]string) {
		autojoinNetwork, autojoinChannels, autojoinKeys = networkID, channels, keys
	}
	c.AutojoinChannelsChanged("freenode", []string{"#go"}, map[string]string{"#go": "secret"})
	if autojoinNetwork != "freenode" {
		t.Fatalf("OnAutojoinChanged network = %q, want freenode", autojoinNetwork)
	}
	if !slices.Equal(autojoinChannels, []string{"#go"}) {
		t.Fatalf("OnAutojoinChanged channels = %v, want [#go]", autojoinChannels)
	}
	if !maps.Equal(autojoinKeys, map[string]string{"#go": "secret"}) {
		t.Fatalf("OnAutojoinChanged keys = %v, want map[#go:secret]", autojoinKeys)
	}
}
