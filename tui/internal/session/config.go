package session

import "strings"

// SessionConfig is the Go shape of IrcSessionConfig in
// src/irc/ircsession.h. It carries one network's identity, credentials,
// autojoin state, and timer policy. Build the defaults with
// DefaultSessionConfig and adjust fields before NewSession.
type SessionConfig struct {
	NetworkID  string
	Name       string
	Host       string
	Port       uint16
	TLSEnabled bool

	Nick             string
	Username         string
	Realname         string
	Password         string
	NickServPassword string
	SASLAccount      string

	AutojoinChannels []string
	AutojoinKeys     map[string]string

	ReconnectEnabled bool

	ReconnectBaseDelayMs     int
	ReconnectMaximumDelayMs  int
	CapabilityTimeoutMs      int
	PingTimeoutMs            int
	LabeledResponseTimeoutMs int
}

// DefaultSessionConfig builds a config with the same defaults as
// IrcSessionConfig in src/irc/ircsession.h: port 6697, TLS on, reconnect
// enabled, 1000ms base backoff doubling to 30000ms, a 10000ms capability
// timeout, a 60000ms ping watchdog, and a 45000ms labeled-response timeout.
//
// Username and realname default to nick, mirroring the fallback in
// IrcConnection::sessionConfigFor (src/irc/ircconnection.cpp:1581-1583): an
// empty profile field is filled from the nick before the session is built.
func DefaultSessionConfig(networkID, name, host, nick string) SessionConfig {
	return SessionConfig{
		NetworkID:  networkID,
		Name:       name,
		Host:       host,
		Port:       6697,
		TLSEnabled: true,

		Nick:     nick,
		Username: nick,
		Realname: nick,

		ReconnectEnabled:         true,
		ReconnectBaseDelayMs:     1000,
		ReconnectMaximumDelayMs:  30000,
		CapabilityTimeoutMs:      10000,
		PingTimeoutMs:            60000,
		LabeledResponseTimeoutMs: 45000,
	}
}

// SASLAccountName returns the SASL account name. An empty SASLAccount falls
// back to the nick, mirroring IrcNetworkProfile::saslAccount()
// (src/irc/ircnetworkprofile.cpp:180-187). A bouncer's upstream network is
// carried in SASLAccount itself, after the slash, so no extra composition
// happens here.
func (c SessionConfig) SASLAccountName() string {
	if c.SASLAccount == "" {
		return c.Nick
	}
	return c.SASLAccount
}

// saslSecret returns the credential used for SASL, preferring the NickServ
// password when both are configured. It mirrors the anonymous saslSecret in
// src/irc/ircsession.cpp:38-41.
func (c SessionConfig) saslSecret() string {
	if c.NickServPassword != "" {
		return c.NickServPassword
	}
	return c.Password
}

// resolvedName returns the display name: the trimmed configured name, else the
// trimmed host. It mirrors IrcNetworkProfile::resolvedName
// (src/irc/ircnetworkprofile.cpp:167-173).
func (c SessionConfig) resolvedName() string {
	if trimmed := strings.TrimSpace(c.Name); trimmed != "" {
		return trimmed
	}
	return strings.TrimSpace(c.Host)
}
