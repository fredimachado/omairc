package storage

import (
	"os"
	"sort"
	"strings"
)

// IconColorCount is the number of palette slots a network may use. It mirrors
// IrcNetworkProfile::iconColorCount.
const IconColorCount = 5

// NoIconColor marks a profile with no assigned palette slot. It mirrors
// IrcNetworkProfile::noIconColor.
const NoIconColor = -1

// Profile is the persisted shape of one network, mirroring IrcNetworkProfile.
type Profile struct {
	NetworkID        string
	Name             string
	Host             string
	Port             uint16
	TLSEnabled       bool
	ConnectOnStartup bool
	SecretSaved      bool
	NickServSaved    bool
	Nick             string
	Username         string
	Realname         string
	Account          string
	BouncerNetwork   string
	AutojoinChannels []string
	AutojoinKeys     map[string]string
	IconColor        int
	AvatarURL        string
}

// ProfileStore reads and writes the "networks" group of the shared QSettings
// ini, mirroring IrcProfileStore. It holds no state: every call opens the
// config file fresh so external edits are never masked by a cache, exactly as
// IrcProfileStore constructs a QSettings per call.
type ProfileStore struct{}

// networksGroup is the QSettings group that owns every persisted network.
const networksGroup = "networks"

// NewProfileStore returns a stateless store over the shared config path.
func NewProfileStore() *ProfileStore {
	return &ProfileStore{}
}

// Profiles returns every persisted network in the store, mirroring
// IrcProfileStore::profiles. The "networks" group is walked through
// ChildGroups, and a profile with a blank trimmed name falls back to its host.
func (s *ProfileStore) Profiles() []Profile {
	settings := OpenSettings("")
	ids := settings.ChildGroups(networksGroup)
	result := make([]Profile, 0, len(ids))
	for _, id := range ids {
		group := networksGroup + "/" + id
		profile := Profile{NetworkID: id}
		profile.Host = settings.Value(group, "host")
		profile.Name = settings.Value(group, "name")
		if strings.TrimSpace(profile.Name) == "" {
			profile.Name = profile.Host
		}
		profile.Port = uint16(settings.Uint(group, "port", 6697))
		profile.TLSEnabled = settings.Bool(group, "tls", true)
		profile.ConnectOnStartup = settings.Bool(group, "connectOnStartup", false)
		profile.SecretSaved = settings.Bool(group, "secretSaved", false)
		profile.NickServSaved = settings.Bool(group, "nickServSaved", false)
		profile.Nick = settings.Value(group, "nick")
		profile.Username = settings.Value(group, "username")
		profile.Realname = settings.Value(group, "realname")
		profile.Account = settings.Value(group, "account")
		profile.BouncerNetwork = settings.Value(group, "bouncerNetwork")
		profile.AutojoinChannels = settings.StringList(group, "autojoin")
		profile.AutojoinKeys = profileStoreLoadAutojoinKeys(
			profile.AutojoinChannels,
			settings.StringList(group, "autojoinKeyChannels"),
			settings.StringList(group, "autojoinKeyValues"))
		// An absent or unparsable key yields NoIconColor through Int's
		// fallback, and a parsed value outside the palette fails the range
		// check, so both collapse to NoIconColor exactly like the C++
		// toInt(&ok) + range test.
		iconColor := settings.Int(group, "iconColor", NoIconColor)
		if iconColor >= 0 && iconColor < IconColorCount {
			profile.IconColor = iconColor
		} else {
			profile.IconColor = NoIconColor
		}
		profile.AvatarURL = settings.Value(group, "avatarUrl")
		result = append(result, profile)
	}
	return result
}

// Save persists profile under "networks/<networkId>", mirroring
// IrcProfileStore::save. An empty id is Absent. The write fails closed before
// any mutation when the file is malformed or cannot accept a write, so a bad
// line is preserved rather than silently rewritten from a partial cache.
func (s *ProfileStore) Save(profile Profile) Status {
	if profile.NetworkID == "" {
		return StatusAbsent
	}

	settings := OpenSettings("")
	if blocked := settings.WriteBlocked(); blocked != StatusWritten {
		return blocked
	}

	group := networksGroup + "/" + profile.NetworkID
	settings.RemoveGroup(group)
	if strings.TrimSpace(profile.Name) != "" {
		settings.SetValue(group, "name", profile.Name)
	}
	settings.SetValue(group, "host", profile.Host)
	settings.SetUint(group, "port", uint64(profile.Port))
	settings.SetBool(group, "tls", profile.TLSEnabled)
	settings.SetBool(group, "connectOnStartup", profile.ConnectOnStartup)
	settings.SetBool(group, "secretSaved", profile.SecretSaved)
	settings.SetBool(group, "nickServSaved", profile.NickServSaved)
	settings.SetValue(group, "nick", profile.Nick)
	settings.SetValue(group, "username", profile.Username)
	settings.SetValue(group, "realname", profile.Realname)
	settings.SetValue(group, "account", profile.Account)
	settings.SetValue(group, "bouncerNetwork", profile.BouncerNetwork)
	settings.SetStringList(group, "autojoin", profile.AutojoinChannels)
	if len(profile.AutojoinKeys) > 0 {
		// QMap::keys()/values() are sorted by key; reproduce that order so
		// the channels and values stay positionally aligned on disk.
		keys := make([]string, 0, len(profile.AutojoinKeys))
		for key := range profile.AutojoinKeys {
			keys = append(keys, key)
		}
		sort.Strings(keys)
		values := make([]string, 0, len(keys))
		for _, key := range keys {
			values = append(values, profile.AutojoinKeys[key])
		}
		settings.SetStringList(group, "autojoinKeyChannels", keys)
		settings.SetStringList(group, "autojoinKeyValues", values)
	}
	if profile.IconColor >= 0 && profile.IconColor < IconColorCount {
		settings.SetInt(group, "iconColor", profile.IconColor)
	}
	if profile.AvatarURL != "" {
		settings.SetValue(group, "avatarUrl", profile.AvatarURL)
	}
	return settings.Sync()
}

// Remove deletes "networks/<networkId>", mirroring IrcProfileStore::remove. An
// empty id or a config file that is missing on disk is Absent. The bytes are
// probed before any mutation, so a malformed or unreadable file is left
// untouched instead of being rebuilt from a partial parse.
func (s *ProfileStore) Remove(networkID string) Status {
	if networkID == "" {
		return StatusAbsent
	}

	path := ConfigPath()
	if path == "" {
		return StatusAbsent
	}
	// Check before opening: a missing file (or missing parent) is Absent, and
	// we must not create an empty ini just to remove a key from it.
	if _, err := os.Stat(path); err != nil {
		return StatusAbsent
	}

	// Fail closed on the raw bytes before touching the group, so a bad line
	// stays put rather than being written back over.
	switch ProbeSettingsIni(path) {
	case IniProbeMalformed:
		return StatusFormatError
	case IniProbeUnreadable:
		return StatusAccessError
	}

	settings := OpenSettings(path)
	if !profileStoreContainsString(settings.ChildGroups(networksGroup), networkID) {
		return StatusAbsent
	}
	// Pending keys survive a failed sync in a process-wide cache; refuse
	// before mutating a file that cannot accept the write.
	if settingsFileNotWritable(path) {
		return StatusAccessError
	}

	settings.RemoveGroup(networksGroup + "/" + networkID)
	return settings.Sync()
}

// profileStoreLoadAutojoinKeys ports loadAutojoinKeys: it pairs each channel
// with the value at the first index whose name matches the channel
// case-insensitively, skipping empty names and values. A length mismatch means
// the persisted arrays are inconsistent, so no keys are returned. The map keys
// are the channels exactly as stored.
func profileStoreLoadAutojoinKeys(channels, names, values []string) map[string]string {
	keys := make(map[string]string)
	if len(names) != len(values) {
		return keys
	}
	for _, channel := range channels {
		for index := 0; index < len(names); index++ {
			if names[index] == "" || values[index] == "" {
				continue
			}
			if !strings.EqualFold(channel, names[index]) {
				continue
			}
			keys[channel] = values[index]
			break
		}
	}
	return keys
}

// profileStoreContainsString reports whether values contains want.
func profileStoreContainsString(values []string, want string) bool {
	for _, value := range values {
		if value == want {
			return true
		}
	}
	return false
}
