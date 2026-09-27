package theme

import "time"

// DefaultPollInterval is the mtime-poll cadence for the theme watcher. Omarchy
// swaps themes rarely, so a coarse poll stays off the CPU while still feeling
// instant.
const DefaultPollInterval = time.Second

// Change is the watcher's notification shape. The shell converts it into a
// Bubble Tea message — for example a tea.Cmd that reads Changes() and returns
// the Change — and re-derives its rendered styles from Colors.
//
// Found is false when colors.toml is missing or unreadable; Colors is then
// Fallback(). Path is the colors.toml that was read ("" off Linux).
type Change struct {
	Colors Colors
	Path   string
	Found  bool
}
