package ui

// composerView renders the single-line composer. Phase 4 has no slash parsing:
// a leading "/" is just text and is sent literally.
func (m *Model) composerView() string {
	prompt := m.styles.Prompt.Render("› ")
	line := prompt + m.composer.View()
	return m.styles.Composer.MaxWidth(m.width).Inline(true).Render(line)
}
