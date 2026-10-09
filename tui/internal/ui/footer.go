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

	tryAssemble := func(versionOn bool, status footerStatusLevel, degrade bool) (string, bool) {
		return m.assembleFooterLine(h, bindings, versionLabel, versionOn, status, width, degrade)
	}
	if line, ok := tryAssemble(true, footerStatusFull, false); ok {
		return line
	}
	if line, ok := tryAssemble(false, footerStatusFull, false); ok {
		return line
	}
	if line, ok := tryAssemble(true, footerStatusFull, true); ok {
		return line
	}
	if line, ok := tryAssemble(false, footerStatusFull, true); ok {
		return line
	}
	for statusLevel := footerStatusEllipsisNetwork; statusLevel <= footerStatusMarkOnly; statusLevel++ {
		if line, ok := tryAssemble(false, statusLevel, false); ok {
			return line
		}
		if line, ok := tryAssemble(false, statusLevel, true); ok {
			return line
		}
	}
	left := m.footerStatusAt(footerStatusMarkOnly, 0)
	right := fitFooterHelp(h, bindings, width-lipgloss.Width(left))
	if line, ok := padFooterLine(left, right, "", width); ok {
		return line
	}
	return truncateLine(left+right, width)
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
	h help.Model,
	bindings []key.Binding,
	versionLabel string,
	versionOn bool,
	statusLevel footerStatusLevel,
	width int,
	allowHintDegrade bool,
) (string, bool) {
	tailKey := renderHelpKey(h, footerShortcutBinding)
	minRight := lipgloss.Width(tailKey)
	versionReserve := 0
	if versionOn {
		versionReserve = lipgloss.Width(versionLabel) + 1
	}
	textBudget := width - 1 - minRight - versionReserve
	if textBudget < 1 {
		textBudget = 1
	}
	left := m.footerStatusAt(statusLevel, textBudget)
	leftW := lipgloss.Width(left)

	rightBudget := width - leftW - 1 - versionReserve
	if rightBudget < lipgloss.Width(tailKey) {
		return "", false
	}
	right := renderFooterHelpFull(h, bindings, rightBudget)
	if right == "" {
		if !allowHintDegrade {
			return "", false
		}
		right = fitFooterHelp(h, bindings, rightBudget)
		if right == "" {
			return "", false
		}
	}

	versionPart := ""
	if versionOn {
		if leftW+1+lipgloss.Width(right)+1+lipgloss.Width(versionLabel) > width {
			return "", false
		}
		versionPart = " " + versionLabel
	}
	return padFooterLine(left, right, versionPart, width)
}

func renderFooterHelpFull(h help.Model, bindings []key.Binding, budget int) string {
	if len(bindings) == 0 || budget <= 0 {
		return ""
	}
	tail := bindings[len(bindings)-1]
	prefix := bindings[:len(bindings)-1]
	tailFull := renderHelpItem(h, tail)
	sep := h.Styles.ShortSeparator.Inline(true).Render(h.ShortSeparator)
	sepWidth := lipgloss.Width(sep)
	steps := make([]int, len(prefix))
	tailW := lipgloss.Width(tailFull)
	if tailW > budget {
		return ""
	}
	prefixBudget := budget - tailW
	if len(prefix) > 0 {
		prefixBudget -= sepWidth
	}
	if prefixBudget < 0 {
		if len(prefix) == 0 {
			return tailFull
		}
		return ""
	}
	candidate := tailFull
	if len(prefix) > 0 {
		candidate = joinHelpWithSteps(h, prefix, steps, sep) + sep + tailFull
	}
	if lipgloss.Width(candidate) <= budget {
		return candidate
	}
	return ""
}

// fitFooterHelp renders contextual help within budget, keeping the Ctrl+/
// shortcuts tail whole or, when necessary, the Ctrl+/ key alone.
func fitFooterHelp(h help.Model, bindings []key.Binding, budget int) string {
	if len(bindings) == 0 || budget <= 0 {
		return ""
	}
	tail := bindings[len(bindings)-1]
	prefix := bindings[:len(bindings)-1]
	tailFull := renderHelpItem(h, tail)
	tailKey := renderHelpKey(h, tail)
	sep := h.Styles.ShortSeparator.Inline(true).Render(h.ShortSeparator)
	sepWidth := lipgloss.Width(sep)

	tryTail := func(tailRender string) string {
		tailW := lipgloss.Width(tailRender)
		if tailW > budget {
			return ""
		}
		prefixBudget := budget - tailW
		if len(prefix) > 0 {
			prefixBudget -= sepWidth
		}
		if prefixBudget < 0 {
			if len(prefix) == 0 {
				return tailRender
			}
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
				right := tailRender
				if len(kept) > 0 {
					right = joinHelpWithSteps(h, kept, steps, sep) + sep + tailRender
				}
				if lipgloss.Width(right) <= budget {
					return right
				}
			}
		}
		return ""
	}
	if got := tryTail(tailFull); got != "" {
		return got
	}
	if got := tryTail(tailKey); got != "" {
		return got
	}
	if lipgloss.Width(tailKey) <= budget {
		return tailKey
	}
	return tailKey
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
