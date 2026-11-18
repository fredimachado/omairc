package ui

import (
	"os"
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"

	"github.com/fredimachado/omairc/tui/internal/connection"
	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/irc"
)

// overlayCase is one filter sheet: how to open it, how to see it is the open
// one, the title it prints on its top border, and how to read its filter input.
type overlayCase struct {
	name    string
	open    func(*Model)
	visible func(*Model) bool
	title   string
	// value reads the sheet's filter input; inbox has none.
	value func(*Model) string
}

func overlayCases() []overlayCase {
	return []overlayCase{
		{"jump", (*Model).openJump, (*Model).jumpVisible, "Jump",
			func(m *Model) string { return m.jumpSheet().input.Value() }},
		{"nick", (*Model).openNickJump, (*Model).nickVisible, "Nick",
			func(m *Model) string { return m.nickSheet().input.Value() }},
		{"link", (*Model).toggleLink, (*Model).linkVisible, "Links",
			func(m *Model) string { return m.linkSheet().input.Value() }},
		{"inbox", (*Model).toggleInbox, (*Model).inboxVisible, "Inbox", nil},
		{"search", (*Model).toggleSearch, (*Model).searchVisible, "Search",
			func(m *Model) string { return m.searchSheet().input.Value() }},
		{"list", (*Model).openChannelList, (*Model).channelListVisible, "Channels",
			func(m *Model) string { return m.listSheet().input.Value() }},
		{"file", (*Model).openFilePick, (*Model).filePickVisible, "File path",
			func(m *Model) string { return m.fileSheet().input.Value() }},
	}
}

// overlayModel is the demo world with a file host on the focused network, so
// every one of the seven sheets can open on a #channel.
func overlayModel(t *testing.T) *Model {
	t.Helper()
	m := seededModel(t)
	if !m.ctrl.IsChannel() || !m.ctrl.ChannelJoined() {
		t.Fatal("the demo must start on a joined channel so the nick sheet can open")
	}
	features := irc.NewServerFeatures()
	features.ApplyToken("CHANTYPES=#")
	features.ApplyToken("soju.im/FILEHOST=https://irc.example/upload")
	m.ctrl.Reducer().SetServerFeatures(m.ctrl.FocusedNetworkID(), features)
	if m.ctrl.FileHost() == "" {
		t.Fatal("the demo network did not take the file host")
	}
	return m
}

// cardTop is the border, header, and filter lines of the topmost card: where a sheet
// names itself, before any row of content can mention another sheet.
func cardTop(m *Model) string {
	card, ok := m.overlayCard()
	if !ok {
		return ""
	}
	plain := ansiPattern.ReplaceAllString(card, "")
	return strings.Join(strings.SplitN(plain, "\n", 5)[:4], "\n")
}

func openCount(m *Model) int {
	count := 0
	for _, c := range overlayCases() {
		if c.visible(m) {
			count++
		}
	}
	return count
}

func TestOpeningEachFilterSheetReplacesTheOther(t *testing.T) {
	for _, first := range overlayCases() {
		for _, second := range overlayCases() {
			if first.name == second.name {
				continue
			}
			t.Run(first.name+" then "+second.name, func(t *testing.T) {
				m := overlayModel(t)
				first.open(m)
				if !first.visible(m) {
					t.Fatalf("%s did not open", first.name)
				}
				second.open(m)
				for _, c := range overlayCases() {
					if got, want := c.visible(m), c.name == second.name; got != want {
						t.Fatalf("after %s then %s: %s visible = %v, want %v",
							first.name, second.name, c.name, got, want)
					}
				}
				if m.composer.Focused() {
					t.Fatal("the composer kept focus under an open filter sheet")
				}
				top := cardTop(m)
				if !strings.Contains(top, second.title) {
					t.Fatalf("card top %q does not show %s", top, second.title)
				}
				if strings.Contains(top, first.title) {
					t.Fatalf("card top %q still shows the replaced %s", top, first.name)
				}
			})
		}
	}
}

func TestReopeningSameSheetKeepsOne(t *testing.T) {
	m := overlayModel(t)
	m.openJump()
	m.jumpSheet().input.SetValue("anna")
	m.openJump()
	if openCount(m) != 1 || !m.jumpVisible() {
		t.Fatalf("reopening jump left %d sheets open", openCount(m))
	}
	if got := m.jumpSheet().input.Value(); got != "" {
		t.Fatalf("reopened jump kept the old query %q", got)
	}
}

func TestEscapeClosesTheFilterSheetAndFocusesTheComposer(t *testing.T) {
	for _, c := range overlayCases() {
		t.Run(c.name, func(t *testing.T) {
			m := overlayModel(t)
			c.open(m)
			m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEscape})
			if openCount(m) != 0 || m.overlay != nil {
				t.Fatal("Escape left a filter sheet open")
			}
			if !m.composer.Focused() {
				t.Fatal("closing the sheet did not return focus to the composer")
			}
		})
	}
}

func TestShortcutsOverConnectEscapeReturnsToConnect(t *testing.T) {
	m, _ := sizedModel(t, connection.New(controller.New(), nil))
	if !m.connectVisible() {
		t.Fatal("first run must show Connect")
	}
	m = press(t, m, ctrlKey('/'))
	if !m.shortcutsOpen || !m.connectVisible() {
		t.Fatalf("Ctrl+/ over Connect: shortcutsOpen=%v connectVisible=%v", m.shortcutsOpen, m.connectVisible())
	}
	top, _ := m.overlayCard()
	if card := ansiPattern.ReplaceAllString(top, ""); !strings.Contains(card, "Shortcuts") {
		t.Fatalf("the shortcuts sheet is not the top card:\n%s", card)
	}
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEscape})
	if m.shortcutsOpen || !m.connectVisible() {
		t.Fatalf("Escape: shortcutsOpen=%v connectVisible=%v, want only shortcuts closed", m.shortcutsOpen, m.connectVisible())
	}
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEscape})
	if !m.connectVisible() {
		t.Fatal("a second Escape closed the Connect sheet that setup still requires")
	}
}

func TestEscapeClosesOnlyTopLayer(t *testing.T) {
	m := overlayModel(t)
	m.openJump()
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEscape})
	if m.jumpVisible() || m.shortcutsOpen || m.aboutOpen || !m.composer.Focused() {
		t.Fatal("Escape on jump must close only jump and focus the composer")
	}
	m = press(t, m, ctrlKey('/'))
	if !m.shortcutsOpen || m.composer.Focused() {
		t.Fatal("Ctrl+/ from the composer must open shortcuts and blur the composer")
	}
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEscape})
	if m.shortcutsOpen || !m.composer.Focused() {
		t.Fatal("Escape on shortcuts must close it and focus the composer")
	}
}

func TestCtrlSlashIgnoredWhileFilterSheetOpen(t *testing.T) {
	for _, c := range overlayCases() {
		t.Run(c.name, func(t *testing.T) {
			m := overlayModel(t)
			c.open(m)
			m = press(t, m, ctrlKey('/'))
			if m.shortcutsOpen {
				t.Fatal("Ctrl+/ opened shortcuts on top of a filter sheet")
			}
			if !c.visible(m) {
				t.Fatal("Ctrl+/ closed the filter sheet")
			}
		})
	}
}

func TestOverlayCardLayerOrder(t *testing.T) {
	m := overlayModel(t)
	m.openJump()
	// Each layer is pushed on top of the previous ones, so the card must come
	// from the new one each time: Connect, shortcuts, About, channel prompt.
	steps := []struct {
		name  string
		push  func()
		query func() string
	}{
		{"filter sheet", func() {}, func() string { return m.jumpCard(m.width) }},
		{"connect", func() { m.conn = connection.New(m.ctrl, nil); m.connectOpen = true }, func() string { return m.connectCard(m.width) }},
		{"shortcuts", func() { m.shortcutsOpen = true }, func() string { return m.shortcutsCard(m.width) }},
		{"about", func() { m.aboutOpen = true }, func() string { return m.aboutCard(m.width) }},
		{"channel prompt", func() { m.channelPrompt.open = true }, func() string { return m.channelPromptCard(m.width) }},
	}
	previous := ""
	for _, step := range steps {
		step.push()
		card, ok := m.overlayCard()
		if !ok || card != step.query() {
			t.Fatalf("overlayCard must pick the %s layer (ok=%v)", step.name, ok)
		}
		if card == previous {
			t.Fatalf("the %s layer rendered the same card as the layer below it", step.name)
		}
		previous = card
	}
}

func TestOpeningJumpOverListDismissesControllerList(t *testing.T) {
	m := overlayModel(t)
	m.composer.SetValue("/list")
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEnter})
	if !m.channelListVisible() || !m.ctrl.ChannelListSnapshot().Open {
		t.Fatal("/list did not open the list sheet and the controller request")
	}
	// The list owns the keys, so the replacement is a programmatic open (the
	// controller can raise one while another sheet is up), not a chord.
	m.openJump()
	if !m.jumpVisible() || m.channelListVisible() {
		t.Fatalf("jump=%v list=%v, want jump replacing list", m.jumpVisible(), m.channelListVisible())
	}
	if m.ctrl.ChannelListSnapshot().Open {
		t.Fatal("replacing the list left the controller's list request open")
	}
}

func TestEscapeOnListCallsDismiss(t *testing.T) {
	m := overlayModel(t)
	m.composer.SetValue("/list")
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEnter})
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEscape})
	if m.channelListVisible() || m.ctrl.ChannelListSnapshot().Open {
		t.Fatal("Escape did not close the list sheet and drop the controller request")
	}
	if !m.composer.Focused() {
		t.Fatal("Escape on the list did not focus the composer")
	}
}

func TestControllerClearedListClosesTheSheetOnSync(t *testing.T) {
	m := overlayModel(t)
	m.composer.SetValue("/list")
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEnter})
	m.ctrl.DismissChannelList()
	m.syncChannelList()
	if m.channelListVisible() || m.overlay != nil {
		t.Fatal("the sheet stayed open after the controller cleared the list")
	}
	if !m.composer.Focused() {
		t.Fatal("the sync close did not return focus to the composer")
	}
	m.syncChannelList()
	if m.channelListVisible() {
		t.Fatal("a second sync reopened a list the controller no longer reports")
	}
}

func TestPasteReachesOnlyTheOpenFilterSheet(t *testing.T) {
	for _, c := range overlayCases() {
		t.Run(c.name, func(t *testing.T) {
			m := overlayModel(t)
			m.composer.SetValue("draft")
			c.open(m)
			updated, _ := m.Update(tea.PasteMsg{Content: "abc"})
			m = updated.(*Model)
			if got := m.composer.Value(); got != "draft" {
				t.Fatalf("composer = %q after a paste into %s, want it untouched", got, c.name)
			}
			if c.value == nil {
				return
			}
			if got := c.value(m); got != "abc" {
				t.Fatalf("%s input = %q, want the pasted text", c.name, got)
			}
		})
	}
}

func TestKeyTypedReachesOnlyOpenSheet(t *testing.T) {
	for _, c := range overlayCases() {
		if c.value == nil {
			continue
		}
		t.Run(c.name, func(t *testing.T) {
			m := overlayModel(t)
			m.composer.SetValue("draft")
			c.open(m)
			m = press(t, m, tea.KeyPressMsg{Code: 'x', Text: "x"})
			if got := m.composer.Value(); got != "draft" {
				t.Fatalf("composer = %q after typing in %s", got, c.name)
			}
			if got := c.value(m); got != "x" {
				t.Fatalf("%s input = %q, want %q", c.name, got, "x")
			}
		})
	}
}

func TestPasteNotUploadedWhileFilterSheetOpen(t *testing.T) {
	m := overlayModel(t)
	m.openJump()
	path := t.TempDir() + "/a.txt"
	if err := os.WriteFile(path, []byte("hello"), 0o644); err != nil {
		t.Fatal(err)
	}
	updated, _ := m.Update(tea.PasteMsg{Content: path})
	m = updated.(*Model)
	if len(m.fileQueue) != 0 || m.ctrl.FileUploadBusy() {
		t.Fatal("a paste into an open filter sheet started an upload")
	}
	if got := m.jumpSheet().input.Value(); got != path {
		t.Fatalf("jump input = %q, want the pasted path", got)
	}
}
