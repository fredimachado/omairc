package controller

import "github.com/fredimachado/omairc/tui/internal/irc"

// NetworkDisplayName returns the roster display name for one network,
// mirroring IrcConnection::rosterDisplayName over the registered sessions.
// An unknown or empty network id returns "".
func (c *Controller) NetworkDisplayName(networkID string) string {
	if networkID == "" {
		return ""
	}
	s := c.manager.Find(networkID)
	if s == nil {
		return ""
	}
	ids := c.manager.NetworkIDs()
	others := make([]string, 0, len(ids))
	for _, id := range ids {
		if id == networkID {
			continue
		}
		if other := c.manager.Find(id); other != nil {
			others = append(others, other.Name())
		}
	}
	return irc.RosterDisplayName(s.Name(), s.Nick(), others)
}
