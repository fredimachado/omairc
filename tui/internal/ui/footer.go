package ui

import (
	"strings"

	"charm.land/bubbles/v2/help"
	"charm.land/bubbles/v2/key"
	"charm.land/lipgloss/v2"

	"github.com/fredimachado/omairc/tui/internal/version"
)

// footerHeight is the single row the status/help footer occupies below the
// composer. Model.bodyHeight subtracts it whenever footerVisible reports there
// is room.
const footerHeight = 1

type footerStatusLevel int

const (
	footerStatusFull footerStatusLevel = 0
	footerStatusEllipsisNetwork footerStatusLevel = 1
	footerStatusStateOnly footerStatusLevel = 2
	footerStatusMarkOnly footerStatusLevel = 3
)

// helpStyles maps the theme-driven shell palette onto the bubbles/help view so
// the footer's hint reads like the rest of the interface instead of the
// library defaults.
func helpStyles(styles Styles) help.Styles {
	return help.Styles{
		ShortKey:       styles.Keycap,
		ShortDesc:      styles.FooterHint,
		ShortSeparator: styles.Divider,
		Ellipsis:       styles.FooterHint,
		FullKey:        styles.Keycap,
		FullDesc:       styles.FooterHint,
		FullSeparator:  styles.Divider,
	}
}

// footerVisible reports whether the footer fits: the footer row, the composer
// block inside the middle column, and at least one transcript row above it. A
// short terminal drops the footer before it starves the transcript. The slash
// menu does not affect it — the menu floats over the transcript instead of
// stacking above the composer. It is false without a controller, so the empty
// render stays a single line.
func (m *Model) footerVisible() bool {
	if m == nil || m.ctrl == nil {
		return false
	}
	return m.height-composerFieldHeight-footerHeight >= 1
}

// footerStatusView renders the footer's left side: a spinner while the focused
// network is disconnected (a steady dot once connected), the connection state,
// and the focused network's display name.
func (m *Model) footerStatusView() string {
	return m.footerStatusAt(footerStatusFull, 0)
}

func (m *Model) footerStatusAt(level footerStatusLevel, textBudget int) string {
	state := "Offline"
	network := ""
	if m.ctrl != nil {
		if got := m.ctrl.ConnectionStatus(); got != "" {
			state = got
		}
		if id := m.ctrl.FocusedNetworkID(); id != "" {
			network = m.sidebarNetworkDisplayName(id)
		}
	}
	mark := m.spinner.View()
	if state == "Connected" {
		mark = m.styles.StatusOK.Render("●")
	}
	if level == footerStatusMarkOnly {
		return mark
	}
	text := state
	if level <= footerStatusEllipsisNetwork && network != "" {
		text += " · " + network
	}
	styled := m.styles.Footer.Render(text)
	if level == footerStatusEllipsisNetwork && textBudget > 0 {
		room := textBudget - lipgloss.Width(mark) - 1
		if room > 0 {
			styled = ellipsizeLine(styled, room)
		}
	}
	return mark + " " + styled
}

// footerView lays the status line on the left and the contextual help on the
// right, separated out to the window width, with the build version trailing the
// help at the far right. The help is rendered from footerKeyMap, so the hint set
// is the chord map, never a hand-written string.
func (m *Model) footerView() string {
	width := m.width
	if width <= 0 {
		return ""
	}
	h := m.help
	bindings := footerEnabledBindings(footerKeyMap{m: m}.ShortHelp())
	versionLabel := m.styles.FooterHint.Render(version.Value)

	assemble := func(versionOn bool, status footerStatusLevel, right string) (string, bool) {
		return m.assembleFooterLine(versionLabel, versionOn, status, right, width)
	}

	rightBudgetFor := func(versionOn bool, status footerStatusLevel) int {
		versionW := 0
		if versionOn {
			versionW = 1 + lipgloss.Width(versionLabel)
		}
		minLeft := lipgloss.Width(m.footerStatusAt(footerStatusMarkOnly, 0))
		switch status {
		case footerStatusFull:
			minLeft = lipgloss.Width(m.footerStatusAt(footerStatusFull, 0))
		case footerStatusEllipsisNetwork:
			minLeft = lipgloss.Width(m.footerStatusAt(footerStatusStateOnly, 0))
		case footerStatusStateOnly:
			minLeft = lipgloss.Width(m.footerStatusAt(footerStatusStateOnly, 0))
		}
		budget := width - versionW - minLeft - 1
		if budget < 1 {
			return 1
		}
		return budget
	}

	tryHelpSpend := func(versionOn bool, statusLimit footerStatusLevel) (string, bool) {
		prefix := bindings[:len(bindings)-1]
		dropStart := 0
		dropEnd := len(prefix)
		shortModes := []bool{false, true}
		statusStart := footerStatusFull
		if versionOn {
			dropEnd = 0
			shortModes = []bool{false}
		}
		for drop := dropStart; drop <= dropEnd; drop++ {
			for _, shortLast := range shortModes {
				if shortLast {
					kept := prefix[:len(prefix)-drop]
					if len(kept) == 0 || len(footerBindingSteps(kept[len(kept)-1])) < 2 {
						continue
					}
				}
				for status := statusStart; status <= statusLimit; status++ {
					budget := rightBudgetFor(versionOn, status)
					right := renderFooterHelpDropped(h, bindings, drop, shortLast, budget)
					if right == "" {
						continue
					}
					if line, ok := assemble(versionOn, status, right); ok {
						if !versionOn && drop == 0 && !shortLast && width > 80 {
							if withVersion, ok := assemble(true, status, right); ok {
								return withVersion, true
							}
						}
						return line, true
					}
				}
			}
		}
		return "", false
	}

	if line, ok := tryHelpSpend(true, footerStatusFull); ok {
		return line
	}
	if line, ok := tryHelpSpend(false, footerStatusMarkOnly); ok {
		return line
	}

	left := m.footerStatusAt(footerStatusMarkOnly, 0)
	keyBudget := width - lipgloss.Width(left) - 1
	key := renderHelpKey(h, bindings[len(bindings)-1])
	if lipgloss.Width(key) > keyBudget {
		key = truncateLine(key, keyBudget)
	}
	if line, ok := assemble(false, footerStatusMarkOnly, key); ok {
		return line
	}
	if line, ok := padFooterLine(left, key, "", width); ok {
		return line
	}
	return truncateLine(left+key, width)
}

func footerEnabledBindings(bindings []key.Binding) []key.Binding {
	enabled := make([]key.Binding, 0, len(bindings))
	for _, binding := range bindings {
		if binding.Enabled() {
			enabled = append(enabled, binding)
		}
	}
	return enabled
}

func padFooterLine(left, right, versionPart string, width int) (string, bool) {
	gap := width - lipgloss.Width(left) - lipgloss.Width(right) - lipgloss.Width(versionPart)
	if gap < 1 {
		return "", false
	}
	line := left + strings.Repeat(" ", gap) + right + versionPart
	if lipgloss.Width(line) != width {
		return "", false
	}
	return line, true
}

func (m *Model) assembleFooterLine(
	versionLabel string,
	versionOn bool,
	statusLevel footerStatusLevel,
	right string,
	width int,
) (string, bool) {
	if right == "" {
		return "", false
	}
	versionPart := ""
	versionW := 0
	if versionOn {
		versionPart = " " + versionLabel
		versionW = lipgloss.Width(versionPart)
	}
	rightW := lipgloss.Width(right)
	textBudget := width - versionW - rightW - 1
	if textBudget < 1 {
		return "", false
	}
	var left string
	if statusLevel == footerStatusEllipsisNetwork {
		left = m.footerStatusAt(statusLevel, textBudget)
	} else {
		left = m.footerStatusAt(statusLevel, 0)
		if lipgloss.Width(left) > textBudget {
			return "", false
		}
	}
	return padFooterLine(left, right, versionPart, width)
}

func renderFooterHelpDropped(h help.Model, bindings []key.Binding, drop int, shortLast bool, budget int) string {
	if len(bindings) == 0 || budget <= 0 {
		return ""
	}
	tail := bindings[len(bindings)-1]
	prefix := bindings[:len(bindings)-1]
	if drop > len(prefix) {
		return ""
	}
	kept := prefix[:len(prefix)-drop]
	tailFull := renderHelpItem(h, tail)
	if lipgloss.Width(tailFull) > budget {
		return ""
	}
	sep := h.Styles.ShortSeparator.Inline(true).Render(h.ShortSeparator)
	steps := make([]int, len(kept))
	if shortLast && len(kept) > 0 {
		steps[len(kept)-1] = 1
	}
	right := tailFull
	if len(kept) > 0 {
		right = joinHelpWithSteps(h, kept, steps, sep) + sep + tailFull
	}
	if lipgloss.Width(right) <= budget {
		return right
	}
	return ""
}

func renderFooterHelpFull(h help.Model, bindings []key.Binding, budget int) string {
	return renderFooterHelpDropped(h, bindings, 0, false, budget)
}

// fitFooterHelp renders contextual help within budget, keeping the Ctrl+/
// shortcuts tail whole and dropping or shortening earlier hints from the end.
func fitFooterHelp(h help.Model, bindings []key.Binding, budget int) string {
	if len(bindings) == 0 || budget <= 0 {
		return ""
	}
	tail := bindings[len(bindings)-1]
	prefix := bindings[:len(bindings)-1]
	tailFull := renderHelpItem(h, tail)
	sep := h.Styles.ShortSeparator.Inline(true).Render(h.ShortSeparator)

	tailW := lipgloss.Width(tailFull)
	if tailW > budget {
		return ""
	}
	for drop := 0; drop <= len(prefix); drop++ {
		kept := prefix[:len(prefix)-drop]
		for _, shortLast := range []bool{false, true} {
			if shortLast && len(kept) == 0 {
				continue
			}
			steps := make([]int, len(kept))
			if shortLast {
				last := len(kept) - 1
				if len(footerBindingSteps(kept[last])) < 2 {
					continue
				}
				steps[last] = 1
			}
			right := tailFull
			if len(kept) > 0 {
				right = joinHelpWithSteps(h, kept, steps, sep) + sep + tailFull
			}
			if lipgloss.Width(right) <= budget {
				return right
			}
		}
	}
	return ""
}

func joinHelpWithSteps(h help.Model, bindings []key.Binding, steps []int, sep string) string {
	if len(bindings) == 0 {
		return ""
	}
	var b strings.Builder
	for i, binding := range bindings {
		if i > 0 {
			b.WriteString(sep)
		}
		variants := footerBindingSteps(binding)
		step := steps[i]
		if step >= len(variants) {
			step = len(variants) - 1
		}
		b.WriteString(renderHelpItem(h, variants[step]))
	}
	return b.String()
}

// shortHelpKeepingTail renders the footer's short help and keeps the last
// binding on screen. bubbles/help drops items from the end, and that last
// binding is the Ctrl+/ shortcuts toggle. A longer completion hint would hide
// it. When the row is too narrow for every chord, earlier hints ellipsize and
// the toggle stays.
func shortHelpKeepingTail(h help.Model, bindings []key.Binding) string {
	enabled := footerEnabledBindings(bindings)
	if len(enabled) == 0 {
		return ""
	}
	return fitFooterHelp(h, enabled, h.Width())
}

func renderHelpItem(h help.Model, binding key.Binding) string {
	helpText := binding.Help()
	return h.Styles.ShortKey.Inline(true).Render(helpText.Key) + " " +
		h.Styles.ShortDesc.Inline(true).Render(helpText.Desc)
}

func renderHelpKey(h help.Model, binding key.Binding) string {
	return h.Styles.ShortKey.Inline(true).Render(binding.Help().Key)
}

func fitHelpPrefix(h help.Model, prefix []key.Binding, budget int, sep string) ([]key.Binding, bool) {
	sepWidth := lipgloss.Width(sep)
	used := 0
	kept := make([]key.Binding, 0, len(prefix))
	for _, binding := range prefix {
		extra := lipgloss.Width(renderHelpItem(h, binding))
		if len(kept) > 0 {
			extra += sepWidth
		}
		if used+extra > budget {
			return kept, false
		}
		used += extra
		kept = append(kept, binding)
	}
	return kept, true
}

func joinHelpItems(h help.Model, bindings []key.Binding, sep string) string {
	var b strings.Builder
	for i, binding := range bindings {
		if i > 0 {
			b.WriteString(sep)
		}
		b.WriteString(renderHelpItem(h, binding))
	}
	return b.String()
}
