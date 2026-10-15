package ui

import (
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"
)

func mouseModel(t *testing.T, width, height int) *Model {
	t.Helper()
	m := resizeModel(t, seededModel(t), width, height)
	m.SetMouseEnabled(true)
	return m
}

func clickLeft(t *testing.T, m *Model, x, y int) *Model {
	t.Helper()
	return updateMsg(t, m, tea.MouseClickMsg{X: x, Y: y, Button: tea.MouseLeft})
}

func wheelAt(t *testing.T, m *Model, x, y int, button tea.MouseButton) *Model {
	t.Helper()
	return updateMsg(t, m, tea.MouseWheelMsg{X: x, Y: y, Button: button})
}

func TestMouseOptIn(t *testing.T) {
	if MouseOptIn(false, "") || MouseOptIn(false, "0") || MouseOptIn(false, "no") {
		t.Fatal("mouse must stay off without the flag or an accepted environment value")
	}
	for _, env := range []string{"1", "true", "YES", "on", "  yes  "} {
		if !MouseOptIn(false, env) {
			t.Fatalf("MouseOptIn(false, %q) = false, want true", env)
		}
	}
	if !MouseOptIn(true, "") {
		t.Fatal("the flag enables the pointer path")
	}
}

func TestDefaultMouseModeIsOff(t *testing.T) {
	m := seededModel(t)
	if m.MouseEnabled() {
		t.Fatal("a new shell must leave the pointer path off")
	}
	view := m.View()
	if view.MouseMode != tea.MouseModeNone {
		t.Fatalf("MouseMode = %v, want none", view.MouseMode)
	}
	before := m.ctrl.SelectedTarget()
	network := m.ctrl.FocusedNetworkID()
	// A click on the sidebar must not select anything while the path is off.
	m = clickLeft(t, m, 2, 2)
	if m.ctrl.SelectedTarget() != before || m.ctrl.FocusedNetworkID() != network {
		t.Fatalf("disabled click changed selection to %s/%s", m.ctrl.FocusedNetworkID(), m.ctrl.SelectedTarget())
	}
	if m.View().MouseMode != tea.MouseModeNone {
		t.Fatal("a click must not turn mouse mode on")
	}
}

func TestMouseModeCellMotionWhenEnabled(t *testing.T) {
	m := mouseModel(t, 118, 30)
	if m.View().MouseMode != tea.MouseModeCellMotion {
		t.Fatalf("MouseMode = %v, want cell motion", m.View().MouseMode)
	}
}

func TestHitRectsFollowColumnLayout(t *testing.T) {
	cases := []struct {
		name        string
		width       int
		height      int
		hideMembers bool
		wantMembers bool
	}{
		{name: "narrow", width: 50, height: 30, wantMembers: false},
		{name: "auto-hide", width: 60, height: 30, wantMembers: false},
		{name: "shrunk-members", width: 80, height: 30, wantMembers: true},
		{name: "default", width: 118, height: 30, wantMembers: true},
		{name: "wide", width: 160, height: 30, wantMembers: true},
		{name: "hidden-members", width: 118, height: 30, hideMembers: true, wantMembers: false},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			m := mouseModel(t, tc.width, tc.height)
			if tc.hideMembers {
				m.toggleMembers()
			}
			sidebarW, transcriptW, membersW := m.columnLayout()
			layout := m.screenLayout()
			if layout.sidebar.W != sidebarW || layout.transcript.W != transcriptW || layout.members.W != membersW {
				t.Fatalf("rects %d/%d/%d, columnLayout %d/%d/%d",
					layout.sidebar.W, layout.transcript.W, layout.members.W, sidebarW, transcriptW, membersW)
			}
			if layout.sidebar.W+layout.transcript.W+layout.members.W != tc.width {
				t.Fatalf("rects sum %d, want %d", layout.sidebar.W+layout.transcript.W+layout.members.W, tc.width)
			}
			if layout.transcript.X != layout.sidebar.W {
				t.Fatalf("transcript x = %d, want %d", layout.transcript.X, layout.sidebar.W)
			}
			if membersW > 0 && layout.members.X != layout.sidebar.W+layout.transcript.W {
				t.Fatalf("members x = %d, want %d", layout.members.X, layout.sidebar.W+layout.transcript.W)
			}
			assertHitsInside(t, layout.sidebarHits, layout.sidebar)
			assertHitsInside(t, layout.memberHits, layout.members)
			if tc.wantMembers {
				if len(layout.memberHits) == 0 {
					t.Fatal("member column is on screen and has no row hits")
				}
			} else if len(layout.memberHits) != 0 || layout.members.W != 0 {
				t.Fatalf("members rect %v hits %d, want neither", layout.members, len(layout.memberHits))
			}
			if len(layout.sidebarHits) == 0 {
				t.Fatal("sidebar has no row hits")
			}
			sample := m.hitTest(layout.transcript.X+layout.transcript.W/2, layout.transcript.Y+1)
			if sample.kind != hitTranscript {
				t.Fatalf("transcript sample kind = %v, want transcript", sample.kind)
			}
			var conversation pointerHit
			found := false
			for _, hit := range layout.sidebarHits {
				if hit.kind == hitSidebarRow && hit.target != "" {
					conversation = hit
					found = true
					break
				}
			}
			if !found {
				t.Fatal("sidebar has no conversation hit")
			}
			clicked := clickLeft(t, m, conversation.rect.X, conversation.rect.Y)
			if clicked.ctrl.SelectedTarget() != conversation.target || clicked.ctrl.FocusedNetworkID() != conversation.networkID {
				t.Fatalf("click selected %s/%s, want %s/%s",
					clicked.ctrl.FocusedNetworkID(), clicked.ctrl.SelectedTarget(),
					conversation.networkID, conversation.target)
			}
		})
	}
}

func assertHitsInside(t *testing.T, hits []pointerHit, outer cellRect) {
	t.Helper()
	for _, hit := range hits {
		if hit.rect.X < outer.X || hit.rect.Y < outer.Y ||
			hit.rect.X+hit.rect.W > outer.X+outer.W || hit.rect.Y+hit.rect.H > outer.Y+outer.H {
			t.Fatalf("hit %+v escapes column %+v", hit.rect, outer)
		}
	}
}

func TestOverlayHitRectsSitOnTheCard(t *testing.T) {
	for _, height := range []int{30, TerminalMinHeight()} {
		m := mouseModel(t, 118, height)
		m.openJump()
		layout := m.screenLayout()
		if !layout.overlayOn || layout.overlay.empty() {
			t.Fatalf("height %d: jump card is not hit-testable", height)
		}
		if len(layout.overlayHits) == 0 {
			t.Fatalf("height %d: jump card has no row hits", height)
		}
		assertHitsInside(t, layout.overlayHits, layout.overlay)
		outside := m.hitTest(0, 0)
		if outside.kind != hitOverlayOutside {
			t.Fatalf("height %d: corner kind = %v, want outside", height, outside.kind)
		}
		row := layout.overlayHits[0]
		got := m.hitTest(row.rect.X, row.rect.Y)
		if got.kind != hitOverlayRow || got.index != row.index {
			t.Fatalf("height %d: row hit = %+v, want overlay row %d", height, got, row.index)
		}
		// A cell on the card must not fall through to the column under it.
		covered := m.hitTest(layout.overlay.X+1, layout.overlay.Y+1)
		if covered.kind != hitOverlay && covered.kind != hitOverlayRow {
			t.Fatalf("height %d: card cell kind = %v, want the card", height, covered.kind)
		}
	}
}

func TestClickConversationMatchesSelect(t *testing.T) {
	m := mouseModel(t, 118, 30)
	hit, ok := sidebarTarget(m, "#desktop")
	if !ok {
		t.Fatal("seeded sidebar has no #desktop hit")
	}
	chord := seededModel(t)
	chord.switchSelection(func() {
		chord.ctrl.SelectConversation(hit.networkID, hit.target)
	})
	m = clickLeft(t, m, hit.rect.X+1, hit.rect.Y)
	if m.ctrl.SelectedTarget() != chord.ctrl.SelectedTarget() || m.ctrl.FocusedNetworkID() != chord.ctrl.FocusedNetworkID() {
		t.Fatalf("click %s/%s, chord %s/%s",
			m.ctrl.FocusedNetworkID(), m.ctrl.SelectedTarget(),
			chord.ctrl.FocusedNetworkID(), chord.ctrl.SelectedTarget())
	}
}

func TestClickNetworkHeaderTogglesCollapse(t *testing.T) {
	m := mouseModel(t, 118, 30)
	var header pointerHit
	found := false
	for _, hit := range m.screenLayout().sidebarHits {
		if hit.kind == hitSidebarHeader && hit.networkID != "" {
			header = hit
			found = true
			break
		}
	}
	if !found {
		t.Fatal("sidebar has no network header")
	}
	if m.ctrl.IsNetworkCollapsed(header.networkID) {
		t.Fatal("seeded network starts expanded")
	}
	m = clickLeft(t, m, header.rect.X, header.rect.Y)
	if !m.ctrl.IsNetworkCollapsed(header.networkID) {
		t.Fatal("clicking the header must collapse the network")
	}
	for _, hit := range m.screenLayout().sidebarHits {
		if hit.kind == hitSidebarRow && hit.networkID == header.networkID {
			t.Fatalf("collapsed network %s still shows %s", header.networkID, hit.target)
		}
	}
	m = clickLeft(t, m, header.rect.X, header.rect.Y)
	if m.ctrl.IsNetworkCollapsed(header.networkID) {
		t.Fatal("clicking the header again must expand the network")
	}
}

func TestClickMemberMatchesOpenDirect(t *testing.T) {
	m := mouseModel(t, 118, 30)
	hit, ok := memberNick(m, "anna")
	if !ok {
		t.Fatal("member column has no anna hit")
	}
	chord := seededModel(t)
	if !chord.ctrl.OpenDirectMessage("anna") {
		t.Fatal("OpenDirectMessage(anna) failed")
	}
	m = clickLeft(t, m, hit.rect.X, hit.rect.Y)
	if m.ctrl.SelectedTarget() != chord.ctrl.SelectedTarget() || m.ctrl.FocusedNetworkID() != chord.ctrl.FocusedNetworkID() {
		t.Fatalf("member click %s/%s, OpenDirectMessage %s/%s",
			m.ctrl.FocusedNetworkID(), m.ctrl.SelectedTarget(),
			chord.ctrl.FocusedNetworkID(), chord.ctrl.SelectedTarget())
	}
	if m.ctrl.SelectedTarget() != "anna" {
		t.Fatalf("selected %q, want anna", m.ctrl.SelectedTarget())
	}
}

func TestClickSelfMemberIsIgnored(t *testing.T) {
	m := mouseModel(t, 118, 30)
	self := m.ctrl.CurrentNick()
	hit, ok := memberNick(m, self)
	if !ok {
		t.Fatalf("member column has no %s hit", self)
	}
	before := m.ctrl.SelectedTarget()
	m = clickLeft(t, m, hit.rect.X, hit.rect.Y)
	if m.ctrl.SelectedTarget() != before {
		t.Fatalf("clicking %s changed selection to %q", self, m.ctrl.SelectedTarget())
	}
}

func TestClickJumpRowMatchesEnter(t *testing.T) {
	m := mouseModel(t, 118, 30)
	m.openJump()
	var row pointerHit
	found := false
	for _, hit := range m.screenLayout().overlayHits {
		if hit.index != m.jump.selected {
			row = hit
			found = true
			break
		}
	}
	if !found {
		row = m.screenLayout().overlayHits[0]
	}
	chord := seededModel(t)
	chord.openJump()
	chord.jump.selected = row.index
	chord.activateJump()

	m = clickLeft(t, m, row.rect.X, row.rect.Y)
	if m.jump.open {
		t.Fatal("clicking a jump row must dismiss the overlay")
	}
	if m.ctrl.SelectedTarget() != chord.ctrl.SelectedTarget() || m.ctrl.FocusedNetworkID() != chord.ctrl.FocusedNetworkID() {
		t.Fatalf("jump click %s/%s, Enter %s/%s",
			m.ctrl.FocusedNetworkID(), m.ctrl.SelectedTarget(),
			chord.ctrl.FocusedNetworkID(), chord.ctrl.SelectedTarget())
	}
}

func TestClickNickRowMatchesEnter(t *testing.T) {
	m := mouseModel(t, 118, 30)
	m.openNickJump()
	entries := m.nickEntries()
	var row pointerHit
	found := false
	for _, hit := range m.screenLayout().overlayHits {
		if hit.index >= 0 && hit.index < len(entries) && entries[hit.index] != m.ctrl.CurrentNick() {
			row = hit
			found = true
			break
		}
	}
	if !found {
		t.Fatal("nick jump has no other-user row")
	}
	chord := seededModel(t)
	chord.openNickJump()
	chord.nick.selected = row.index
	chord.activateNick()

	m = clickLeft(t, m, row.rect.X, row.rect.Y)
	if m.nick.open {
		t.Fatal("clicking a nick row must dismiss the overlay")
	}
	if m.ctrl.SelectedTarget() != chord.ctrl.SelectedTarget() || m.ctrl.FocusedNetworkID() != chord.ctrl.FocusedNetworkID() {
		t.Fatalf("nick click %s/%s, Enter %s/%s",
			m.ctrl.FocusedNetworkID(), m.ctrl.SelectedTarget(),
			chord.ctrl.FocusedNetworkID(), chord.ctrl.SelectedTarget())
	}
}

func TestClickOutsideDismissesListOverlays(t *testing.T) {
	openers := []struct {
		name string
		open func(*Model)
		done func(*Model) bool
	}{
		{"jump", func(m *Model) { m.openJump() }, func(m *Model) bool { return m.jump.open }},
		{"nick", func(m *Model) { m.openNickJump() }, func(m *Model) bool { return m.nick.open }},
		{"link", func(m *Model) { m.toggleLink() }, func(m *Model) bool { return m.link.open }},
		{"inbox", func(m *Model) { m.toggleInbox() }, func(m *Model) bool { return m.inbox.open }},
		{"list", func(m *Model) { m.openChannelList() }, func(m *Model) bool { return m.list.open }},
	}
	for _, opener := range openers {
		t.Run(opener.name, func(t *testing.T) {
			sheet := mouseModel(t, 118, 30)
			opener.open(sheet)
			if !opener.done(sheet) {
				t.Fatal("overlay did not open")
			}
			before := sheet.ctrl.SelectedTarget()
			sheet = clickLeft(t, sheet, 0, 0)
			if opener.done(sheet) {
				t.Fatal("clicking outside the card must dismiss it")
			}
			if sheet.ctrl.SelectedTarget() != before {
				t.Fatalf("outside click changed selection to %q", sheet.ctrl.SelectedTarget())
			}
		})
	}
}

func TestRightClickAndReleaseDoNotSelect(t *testing.T) {
	m := mouseModel(t, 118, 30)
	hit, ok := sidebarTarget(m, "#desktop")
	if !ok {
		t.Fatal("missing #desktop")
	}
	before := m.ctrl.SelectedTarget()
	m = updateMsg(t, m, tea.MouseClickMsg{X: hit.rect.X, Y: hit.rect.Y, Button: tea.MouseRight})
	m = updateMsg(t, m, tea.MouseReleaseMsg{X: hit.rect.X, Y: hit.rect.Y, Button: tea.MouseLeft})
	if m.ctrl.SelectedTarget() != before {
		t.Fatalf("non-left click selected %q", m.ctrl.SelectedTarget())
	}
}

func TestWheelTranscriptMatchesPageUp(t *testing.T) {
	wheeled, _ := unseenDemoModel(t)
	wheeled.SetMouseEnabled(true)
	paged, _ := unseenDemoModel(t)
	paged = press(t, paged, tea.KeyPressMsg{Code: tea.KeyPgUp})

	point := transcriptPoint(wheeled)
	wheeled = wheelAt(t, wheeled, point.X, point.Y, tea.MouseWheelUp)
	if wheeled.transcriptFollowEnd {
		t.Fatal("wheel up must leave follow-the-end")
	}
	if wheeled.transcriptScroll != paged.transcriptScroll || wheeled.transcriptFollowEnd != paged.transcriptFollowEnd {
		t.Fatalf("wheel scroll=%d follow=%v, Page Up scroll=%d follow=%v",
			wheeled.transcriptScroll, wheeled.transcriptFollowEnd, paged.transcriptScroll, paged.transcriptFollowEnd)
	}
	if wheeled.transcriptPlaceLabel() == "" || wheeled.transcriptPlaceLabel() != paged.transcriptPlaceLabel() {
		t.Fatalf("place label wheel=%q page=%q", wheeled.transcriptPlaceLabel(), paged.transcriptPlaceLabel())
	}
	plain := ansiPattern.ReplaceAllString(wheeled.View().Content, "")
	if !strings.Contains(plain, wheeled.transcriptPlaceLabel()) {
		t.Fatalf("the border must show the place counter %q:\n%s", wheeled.transcriptPlaceLabel(), plain)
	}

	wheeled = wheelAt(t, wheeled, point.X, point.Y, tea.MouseWheelDown)
	if !wheeled.transcriptFollowEnd || wheeled.transcriptScroll != 0 || wheeled.transcriptPlaceLabel() != "" {
		t.Fatalf("wheel down follow=%v scroll=%d label=%q, want reattached",
			wheeled.transcriptFollowEnd, wheeled.transcriptScroll, wheeled.transcriptPlaceLabel())
	}
}

func TestWheelUpArmsUnseenMarker(t *testing.T) {
	m, d := unseenDemoModel(t)
	m.SetMouseEnabled(true)
	point := transcriptPoint(m)
	m = wheelAt(t, m, point.X, point.Y, tea.MouseWheelUp)
	if m.transcriptFollowEnd {
		t.Fatal("wheel up must detach")
	}
	before := m.transcriptRowTotal()
	d.InjectOmarchy([]byte("@msgid=wheel-new :nora!u@h PRIVMSG #omarchy :new while reading\r\n"))
	m = updateMsg(t, m, NotifyMsg{})
	if !m.jumpArmed() || m.firstUnseenRow != before {
		t.Fatalf("armed=%v firstUnseen=%d, want armed at %d", m.jumpArmed(), m.firstUnseenRow, before)
	}
	plain := ansiPattern.ReplaceAllString(m.View().Content, "")
	if !strings.Contains(plain, "↓ new") {
		t.Fatalf("the header must show ↓ new:\n%s", plain)
	}
	if m.transcriptPlaceLabel() == "" || !strings.Contains(plain, m.transcriptPlaceLabel()) {
		t.Fatalf("a detached reader must keep the place counter %q:\n%s", m.transcriptPlaceLabel(), plain)
	}
}

func TestWheelIgnoredWhenMouseOff(t *testing.T) {
	m, _ := unseenDemoModel(t)
	point := transcriptPoint(m)
	m = wheelAt(t, m, point.X, point.Y, tea.MouseWheelUp)
	if !m.transcriptFollowEnd || m.transcriptScroll != 0 || m.transcriptPlaceLabel() != "" {
		t.Fatal("a wheel event must not scroll while the pointer path is off")
	}
	if m.View().MouseMode != tea.MouseModeNone {
		t.Fatal("mouse mode must stay off")
	}
}

func TestWheelSidebarAndMembersPage(t *testing.T) {
	m := mouseModel(t, 118, TerminalMinHeight())
	_, _, w, h := m.framedInner(m.columnRectsSidebar())
	m.sidebarVisibleSlots(w, h)
	if m.sidebarWindowMax < 1 {
		t.Fatal("a short sidebar must have a scroll range")
	}
	m = wheelAt(t, m, 1, 1, tea.MouseWheelDown)
	if !m.sidebarScrollPinned || m.sidebarScroll == 0 {
		t.Fatalf("sidebar wheel pin=%v scroll=%d, want a paged pin", m.sidebarScrollPinned, m.sidebarScroll)
	}
	if !m.transcriptFollowEnd {
		t.Fatal("a sidebar wheel must not detach the transcript")
	}

	wide := mouseModel(t, 118, TerminalMinHeight())
	_, _, mw, mh := wide.framedInner(wide.columnRectsMembers())
	wide.memberVisibleSlots(mw, mh)
	if wide.memberWindowMax < 1 {
		t.Fatal("a short member list must have a scroll range")
	}
	members := wide.columnRectsMembers()
	wide = wheelAt(t, wide, members.X+1, 1, tea.MouseWheelDown)
	if !wide.memberScrollPinned || wide.memberScroll == 0 {
		t.Fatalf("member wheel pin=%v scroll=%d, want a paged pin", wide.memberScrollPinned, wide.memberScroll)
	}
}

func TestWheelOverlayPagesTheOpenList(t *testing.T) {
	m := mouseModel(t, 118, TerminalMinHeight())
	m.openJump()
	if len(m.jumpEntries()) <= m.overlayRowBudget(1) {
		t.Fatal("the short jump list must overflow so the wheel has a range")
	}
	card := m.screenLayout().overlay
	m = wheelAt(t, m, card.X+2, card.Y+2, tea.MouseWheelDown)
	if m.jump.scroll == 0 {
		t.Fatal("wheel down on the jump card must page the list")
	}
	if !m.transcriptFollowEnd {
		t.Fatal("an overlay wheel must not detach the transcript")
	}
	scrolled := m.jump.scroll
	m = wheelAt(t, m, 0, 0, tea.MouseWheelDown)
	if m.jump.scroll != scrolled {
		t.Fatal("a wheel outside the card must not page the list")
	}
}

func sidebarTarget(m *Model, target string) (pointerHit, bool) {
	for _, hit := range m.screenLayout().sidebarHits {
		if hit.kind == hitSidebarRow && hit.target == target {
			return hit, true
		}
	}
	return pointerHit{}, false
}

func sidebarTargetExists(m *Model, target string) bool {
	_, ok := sidebarTarget(m, target)
	return ok
}

func memberNick(m *Model, nick string) (pointerHit, bool) {
	members := m.ctrl.Members()
	for _, hit := range m.screenLayout().memberHits {
		if hit.index >= 0 && hit.index < len(members) && members[hit.index].Nick == nick {
			return hit, true
		}
	}
	return pointerHit{}, false
}

func transcriptPoint(m *Model) cellRect {
	_, transcript, _ := m.columnRects()
	return cellRect{X: transcript.X + transcript.W/2, Y: transcript.Y + 1}
}

func (m *Model) columnRectsSidebar() cellRect {
	sidebar, _, _ := m.columnRects()
	return sidebar
}

func (m *Model) columnRectsMembers() cellRect {
	_, _, members := m.columnRects()
	return members
}
