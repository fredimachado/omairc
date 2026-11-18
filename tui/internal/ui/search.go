package ui

import (
	"context"
	"strings"
	"time"

	"charm.land/bubbles/v2/textinput"
	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"

	"github.com/fredimachado/omairc/tui/internal/controller"
)

// This file is the Ctrl+Shift+F history search sheet. It mirrors
// `client/src/ui/shell/search.rs` and `client/src/ui/sheets/search.rs`: a
// filter field over a list of hits from every conversation, newest first. The
// matching, merging and jump rules are `irc/search.go`; the two sources are
// `controller/search.go`.
//
// Typing restarts the search: the old one is cancelled, a 150 ms pause lets the
// user finish a word, then the loaded transcripts answer at once and the
// on-disk logs stream in from a worker goroutine as tea.Cmd batches. Every
// message back carries the generation it was started for, and one for an older
// generation is dropped, so a stale batch never paints.

// searchDebounce is how long typing must pause before a search starts.
const searchDebounce = 150 * time.Millisecond

const searchPlaceholder = "Search every conversation…"

// searchDebounceMsg fires once the typing pause for generation gen has passed.
type searchDebounceMsg struct{ gen uint64 }

// searchBatchMsg carries one batch of log hits for generation gen. done is true
// once the worker has no more.
type searchBatchMsg struct {
	gen  uint64
	hits []controller.SearchHit
	done bool
	next <-chan []controller.SearchHit
}

// searchState is the open sheet and its running search.
type searchState struct {
	input    textinput.Model
	selected int
	scroll   int
	// text is the trimmed text the current generation searches.
	text       string
	generation uint64
	results    controller.SearchResults
	scanning   bool
	cancel     context.CancelFunc
}

// searchVisible reports whether the search sheet is open.
func (m *Model) searchVisible() bool { return m.searchSheet() != nil }

// toggleSearch opens or closes the search sheet.
func (m *Model) toggleSearch() {
	if m.searchSheet() != nil {
		m.closeSearch()
		return
	}
	if m.ctrl == nil {
		return
	}
	sheet := &searchState{input: newTextInput(m.styles, searchPlaceholder, overlayFilterPrompt)}
	sheet.input.SetWidth(m.overlayInputWidth())
	m.setOverlay(sheet)
	m.composer.Blur()
	_ = sheet.input.Focus()
}

// closeSearch dismisses the sheet, cancels what it was scanning and returns
// focus to the composer.
func (m *Model) closeSearch() {
	if m.searchSheet() == nil {
		return
	}
	m.clearOverlay()
	_ = m.composer.Focus()
	m.loadDraft()
}

// stopSearch cancels the running scan.
func (s *searchState) stopSearch() {
	if s.cancel != nil {
		s.cancel()
		s.cancel = nil
	}
	s.scanning = false
}

// searchQuery is the query the current generation searches, if it has one.
func (s *searchState) query() (controller.SearchQuery, bool) {
	return controller.ParseSearchQuery(s.text)
}

// searchQueryChanged restarts the search when the trimmed field text changed
// (a caret move changes nothing) and returns the command that ends its pause.
func (m *Model) searchQueryChanged() tea.Cmd {
	sheet := m.searchSheet()
	if sheet == nil {
		return nil
	}
	typed := strings.TrimSpace(sheet.input.Value())
	if typed == sheet.text {
		return nil
	}
	sheet.stopSearch()
	sheet.generation++
	sheet.text = typed
	sheet.results = controller.SearchResults{}
	sheet.selected = 0
	sheet.scroll = 0
	if _, ok := sheet.query(); !ok {
		return nil
	}
	gen := sheet.generation
	return tea.Tick(searchDebounce, func(time.Time) tea.Msg { return searchDebounceMsg{gen: gen} })
}

// handleSearchDebounce runs the search for the generation whose pause ended:
// the loaded transcripts answer now, and the logs start streaming.
func (m *Model) handleSearchDebounce(msg searchDebounceMsg) tea.Cmd {
	sheet := m.searchSheet()
	if sheet == nil || msg.gen != sheet.generation || m.ctrl == nil {
		return nil
	}
	query, ok := sheet.query()
	if !ok {
		return nil
	}
	sheet.results = m.ctrl.SearchLoaded(query)
	sheet.selected = 0
	sheet.scroll = 0
	plan := m.ctrl.SearchDiskScan()
	if plan == nil {
		return nil
	}
	ctx, cancel := context.WithCancel(context.Background())
	sheet.cancel = cancel
	sheet.scanning = true
	batches := make(chan []controller.SearchHit, 8)
	go func() {
		defer close(batches)
		plan.Run(ctx, query, func(hits []controller.SearchHit) {
			select {
			case batches <- hits:
			case <-ctx.Done():
			}
		})
	}()
	return waitSearchBatch(msg.gen, batches)
}

// waitSearchBatch is the command that waits for the worker's next batch.
func waitSearchBatch(gen uint64, batches <-chan []controller.SearchHit) tea.Cmd {
	return func() tea.Msg {
		hits, ok := <-batches
		return searchBatchMsg{gen: gen, hits: hits, done: !ok, next: batches}
	}
}

// handleSearchBatch folds one batch of log hits into the list. A batch for an
// older generation is dropped.
func (m *Model) handleSearchBatch(msg searchBatchMsg) tea.Cmd {
	sheet := m.searchSheet()
	if sheet == nil || msg.gen != sheet.generation {
		return nil
	}
	if msg.done {
		sheet.scanning = false
		return nil
	}
	var held *controller.SearchHit
	if sheet.selected > 0 && sheet.selected < len(sheet.results.Hits) {
		hit := sheet.results.Hits[sheet.selected]
		held = &hit
	}
	sheet.results.Absorb(msg.hits, controller.SearchCap)
	// The selection stays on the same hit as the list grows around it, and stays
	// on the first row while it is there.
	sheet.selected = 0
	if held != nil {
		for index, hit := range sheet.results.Hits {
			if hit.Conversation == held.Conversation && hit.Target == held.Target && hit.Time.Equal(held.Time) {
				sheet.selected = index
				break
			}
		}
	}
	m.revealList(&sheet.scroll, sheet.selected, len(sheet.results.Hits), searchChromeLines)
	return waitSearchBatch(msg.gen, msg.next)
}

// searchChromeLines is the card's non-row lines: the field and the footer.
const searchChromeLines = 2

func (m *Model) moveSearch(delta int) {
	sheet := m.searchSheet()
	if sheet == nil {
		return
	}
	count := len(sheet.results.Hits)
	if count == 0 {
		sheet.selected = 0
		return
	}
	sheet.selected = ((sheet.selected+delta)%count + count) % count
	m.revealList(&sheet.scroll, sheet.selected, count, searchChromeLines)
}

// selectSearchRow highlights hit index (clamped to the list), scrolling it into
// view. Home and End use it; the field's caret keeps Left and Right.
func (m *Model) selectSearchRow(index int) {
	sheet := m.searchSheet()
	if sheet == nil {
		return
	}
	count := len(sheet.results.Hits)
	if count == 0 {
		sheet.selected = 0
		return
	}
	sheet.selected = clampInt(index, 0, count-1)
	m.revealList(&sheet.scroll, sheet.selected, count, searchChromeLines)
}

// activateSearch opens the selected hit and closes the sheet.
func (m *Model) activateSearch() {
	sheet := m.searchSheet()
	if sheet == nil {
		return
	}
	hits := sheet.results.Hits
	if sheet.selected < 0 || sheet.selected >= len(hits) {
		return
	}
	m.activateSearchHit(hits[sheet.selected])
}

// activateSearchHit closes the sheet, opens the hit's conversation (by its key,
// never its label), and lands on its line. A line older than the loaded
// transcript opens the conversation as it opens without a search and says so:
// no history is fetched for it.
func (m *Model) activateSearchHit(hit controller.SearchHit) {
	m.closeSearch()
	m.searchNote = ""
	if m.ctrl == nil {
		return
	}
	m.sidebarNetworkFocusID = ""
	m.switchSelection(func() { m.ctrl.SelectSearchHit(hit) })
	row, landed := controller.JumpLanding(hit, m.ctrl.SearchJumpRows())
	if !landed {
		m.searchNote = controller.OlderThanLoadedNote
		return
	}
	m.revealTranscriptRow(row)
	m.rememberOpenTranscript()
}

// handleSearchKey folds one key while the sheet is open. Escape and the toggle
// chord close it; Up/Down walk the hits, Home and End go to the first and last;
// Enter opens one; everything else edits
// the field.
func (m *Model) handleSearchKey(key string, msg tea.KeyPressMsg) (tea.Model, tea.Cmd) {
	sheet := m.searchSheet()
	if sheet == nil {
		return m, nil
	}
	switch key {
	case "esc", "escape", "ctrl+shift+f":
		m.closeSearch()
		return m, nil
	case "up":
		m.moveSearch(-1)
		return m, nil
	case "down":
		m.moveSearch(1)
		return m, nil
	case "home":
		m.selectSearchRow(0)
		return m, nil
	case "end":
		m.selectSearchRow(len(sheet.results.Hits) - 1)
		return m, nil
	case "enter", "return":
		m.activateSearch()
		return m, nil
	}
	var cmd tea.Cmd
	sheet.input, cmd = sheet.input.Update(msg)
	return m, tea.Batch(cmd, m.searchQueryChanged())
}

// searchCard renders the sheet as one card block through the shared overlay
// frame. The name and hit count sit on the top edge and the highlighted row's
// position on the bottom edge.
func (m *Model) searchCard(width int) string {
	sheet := m.searchSheet()
	if sheet == nil {
		return ""
	}
	count := len(sheet.results.Hits)
	return m.overlayCardFramed(width,
		overlayCountTitle("Search", count),
		overlayCountLabel(sheet.selected, count),
		m.searchCardBody)
}

func (m *Model) searchCardBody(inner int) []string {
	return overlayTexts(m.searchOverlayLines(inner))
}

// searchOverlayLines is the card body: the field, one line per hit, and the
// footer that says where the search stands.
func (m *Model) searchOverlayLines(inner int) []overlayLine {
	sheet := m.searchSheet()
	if sheet == nil {
		return nil
	}
	hits := sheet.results.Hits
	lines := []overlayLine{{text: truncateLine(sheet.input.View(), inner), row: -1}}
	start, end := listWindow(len(hits), sheet.scroll, m.overlayRowBudget(searchChromeLines))
	today := m.searchToday()
	for index := start; index < end; index++ {
		lines = append(lines, overlayLine{
			text: m.searchRow(inner, index == sheet.selected, hits[index], today),
			row:  index,
		})
	}
	_, hasQuery := sheet.query()
	footer := controller.SearchFooter(!hasQuery, sheet.results, sheet.scanning)
	return append(lines, overlayLine{text: m.overlaySheetEmpty(footer), row: -1})
}

// searchToday is the local day a row from today is told apart by.
func (m *Model) searchToday() time.Time { return time.Now() }

// searchRow renders one hit as a sheet row: network, conversation, time,
// author and the line, with the matched nick and text in the find-match style
// (the one Ctrl+F paints on a transcript row). The segments are styled before
// the line is fitted, and the fit never splits an escape or a wide character,
// so a match past the clip is simply cut with the rest of the row.
func (m *Model) searchRow(inner int, selected bool, hit controller.SearchHit, today time.Time) string {
	prefix, rowStyle := m.overlayRowChrome(selected)
	budget := inner - lipgloss.Width(prefix)
	if budget < 0 {
		budget = 0
	}
	label, marks := searchRowLabel(hit, today)
	var styled strings.Builder
	at := 0
	for _, mark := range marks {
		if mark[0] < at || mark[1] > len(label) || mark[0] >= mark[1] {
			continue
		}
		styled.WriteString(rowStyle.Render(label[at:mark[0]]))
		styled.WriteString(m.styles.FindMatch.Render(label[mark[0]:mark[1]]))
		at = mark[1]
	}
	styled.WriteString(rowStyle.Render(label[at:]))
	return m.overlayFillRow(inner, prefix, rowStyle, ellipsizeLine(styled.String(), budget))
}

// searchRowLabel is one row's plain text: network, conversation, time, author
// and the line. marks are the byte ranges in it to highlight, in order: the nick
// when it matched, then the matches in the line.
func searchRowLabel(hit controller.SearchHit, today time.Time) (string, [][2]int) {
	var label strings.Builder
	label.WriteString(hit.NetworkName + "  " + hit.ConversationLabel + "  " +
		controller.SearchTimeLabel(hit.Time, today) + "  ")
	var marks [][2]int
	authorAt := label.Len()
	label.WriteString(hit.Author)
	if hit.AuthorMatch {
		marks = append(marks, [2]int{authorAt, label.Len()})
	}
	label.WriteString("  ")
	bodyAt := label.Len()
	label.WriteString(hit.Text)
	for _, match := range hit.Matches {
		marks = append(marks, [2]int{bodyAt + match.Start, bodyAt + match.End})
	}
	return label.String(), marks
}
