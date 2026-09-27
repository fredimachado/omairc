package ui

import (
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"

	"github.com/fredimachado/omairc/tui/internal/version"
)

// This file is the Ctrl+Shift+/ About sheet: the terminal counterpart of the
// Qt AboutSheet (src/qml/AboutSheet.qml). It shows the app name, the running
// version, a short description, the open-source line with the GitHub source
// URL, an OK control, and the copyright footer.
//
// About is an informational modal, not a filter overlay: it has no rows or
// input, it can open on top of the Connect sheet exactly like the Qt
// Ctrl+Shift+/ shortcut, and every navigation chord is blocked while it is
// open. Escape, Enter, or Space dismiss it, matching the Qt OK button plus the
// Escape-before-Connect order. The Qt sheet also offers Check for Updates; the
// terminal sheet is informational only for now.

// aboutRepoURL is the source repository the open-source line names. It mirrors
// omaircGithubRepoUrl() in src/omaircupdatecheck.cpp.
const aboutRepoURL = "https://github.com/fredimachado/omairc"

// aboutCopyright is the footer line, mirroring AboutSheet.qml's aboutCopyright.
const aboutCopyright = "Copyright © 2026 Fredi Machado"

// aboutDescription is the one-paragraph product blurb, verbatim from
// AboutSheet.qml's aboutDescription.
const aboutDescription = "Omairc is an Internet Relay Chat client for Omarchy. " +
	"People use it to communicate, share, play, and work with each other on " +
	"IRC networks around the world."

// aboutHeaderRows and aboutFooterRows are the rows aboutCardBody pins when a
// short terminal windows the sheet: the title and its rule stay at the top, the
// rule and copyright stay at the bottom.
const (
	aboutHeaderRows = 2
	aboutFooterRows = 2
)

// openAbout shows the sheet. It is allowed on top of the Connect sheet, the
// same as the Qt Ctrl+Shift+/ shortcut. A second open is a no-op: the Qt chord
// is disabled while the sheet is visible, so it does not toggle.
func (m *Model) openAbout() {
	if m == nil || m.aboutOpen {
		return
	}
	m.aboutOpen = true
	m.composer.Blur()
}

// closeAbout hides the sheet and returns focus to the composer, or to the
// Connect sheet underneath. It is idempotent.
func (m *Model) closeAbout() {
	if m == nil || !m.aboutOpen {
		return
	}
	m.aboutOpen = false
	m.refocusComposer()
}

// handleAboutKey folds one key while About is open. Escape, Enter, and Space
// dismiss it; every other key is swallowed so the window chords stay blocked.
// Ctrl+Shift+/ does not toggle it closed, matching the Qt shortcut that is
// disabled while the sheet is visible.
func (m *Model) handleAboutKey(key string) (tea.Model, tea.Cmd) {
	switch key {
	case "esc", "escape", "enter", "return", "space":
		m.closeAbout()
	}
	return m, nil
}

// aboutCard renders the About sheet as one card block through the shared
// overlay frame in model.go.
func (m *Model) aboutCard(width int) string {
	return m.overlayCardBlock(width, m.aboutCardBody)
}

// aboutCardBody is the sheet's content, before the shared frame. inner is the
// content width inside the border. The title rule and the copyright are pinned
// so a short terminal never clips the frame; the description windows between
// them.
func (m *Model) aboutCardBody(inner int) []string {
	lines := m.aboutLines(inner)
	return windowCardRows(lines, aboutHeaderRows, aboutFooterRows, -1, m.overlayCardRowBudget())
}

// aboutLines builds the sheet's rows. Every line is wrapped or padded to inner.
func (m *Model) aboutLines(inner int) []string {
	rule := m.styles.Divider.Render(strings.Repeat("─", max(inner, 0)))
	lines := []string{
		m.styles.PanelTitle.Render("About Omairc"),
		rule,
		m.styles.AppTitle.Render("Omairc"),
		m.styles.MutedLine.Render(version.Value),
		"",
	}
	lines = append(lines, wrapStyled(m.styles.SheetRow, inner, aboutDescription)...)
	lines = append(lines,
		"",
		m.styles.SheetRow.Render("This project is open-source."),
		m.styles.Link.Render("View the source on GitHub"),
		m.styles.MutedLine.Render(aboutRepoURL),
		"",
		m.aboutOKLine(inner),
		"",
		rule,
		m.styles.MutedLine.Render(aboutCopyright),
	)
	return lines
}

// aboutOKLine centers the OK pill and its chord hint. The Qt sheet's OK button
// is focused when the sheet opens and closes it on Enter or Space; the terminal
// sheet prints the same affordance and closes on either key.
func (m *Model) aboutOKLine(inner int) string {
	pill := m.styles.SheetButtonFocus.Background(m.styles.Colors.SurfaceRaised).Render(" OK ")
	hint := m.styles.MutedLine.Render("  Enter / Esc to close")
	line := pill + hint
	if pad := (inner - lipgloss.Width(line)) / 2; pad > 0 {
		line = strings.Repeat(" ", pad) + line
	}
	return truncateLine(line, inner)
}

// wrapStyled word-wraps text to width and returns the styled lines. lipgloss
// owns the wrapping, so a long description never overhangs the card.
func wrapStyled(style lipgloss.Style, width int, text string) []string {
	if width < 1 {
		width = 1
	}
	return strings.Split(style.Width(width).Render(text), "\n")
}
