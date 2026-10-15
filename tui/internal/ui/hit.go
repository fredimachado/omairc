package ui

import "charm.land/lipgloss/v2"

// cellRect is one inclusive-origin, exclusive-end rectangle in terminal cells.
// (0, 0) is the top-left of the frame. Drawing and hit-testing share these
// rectangles so a click cannot drift from the row on screen.
type cellRect struct {
	X, Y, W, H int
}

func (r cellRect) contains(x, y int) bool {
	return r.W > 0 && r.H > 0 && x >= r.X && y >= r.Y && x < r.X+r.W && y < r.Y+r.H
}

func (r cellRect) empty() bool { return r.W <= 0 || r.H <= 0 }

// hitKind names what a cell belongs to. Wheel and click dispatch on it.
type hitKind int

const (
	hitNone hitKind = iota
	hitSidebarHeader
	hitSidebarRow
	hitTranscript
	hitMember
	hitOverlay
	hitOverlayRow
	hitOverlayOutside
)

// pointerHit is one clickable or scrollable target.
type pointerHit struct {
	kind      hitKind
	rect      cellRect
	networkID string
	target    string
	index     int
}

// overlayLine is one content line of an overlay card. row is the activatable
// index, or -1 when the line is a filter, a heading, or an empty state.
type overlayLine struct {
	text string
	row  int
}

func overlayTexts(lines []overlayLine) []string {
	out := make([]string, len(lines))
	for index, line := range lines {
		out[index] = line.text
	}
	return out
}

// listWindow returns the [start, end) slice of a list drawn from offset.
// A list that fits visible is returned whole, so a card that already fit is
// unchanged.
func listWindow(count, offset, visible int) (start, end int) {
	if count < 0 {
		count = 0
	}
	if visible < 1 {
		visible = 1
	}
	if count <= visible {
		return 0, count
	}
	max := count - visible
	return clampInt(offset, 0, max), clampInt(offset, 0, max) + visible
}

// revealList moves offset the least it can so selected is inside the window.
func (m *Model) revealList(offset *int, selected, count, header int) {
	if m == nil || offset == nil {
		return
	}
	visible := m.overlayRowBudget(header)
	if count <= visible {
		*offset = 0
		return
	}
	if selected < 0 {
		selected = 0
	}
	if selected >= count {
		selected = count - 1
	}
	if selected < *offset {
		*offset = selected
	} else if selected >= *offset+visible {
		*offset = selected - visible + 1
	}
	*offset = clampInt(*offset, 0, count-visible)
}

// wheelList pages offset by one window, the same step Page Up/Down uses on
// the transcript. direction < 0 moves toward the start of the list.
func wheelList(offset *int, count, visible, direction int) {
	if offset == nil {
		return
	}
	if visible < 1 {
		visible = 1
	}
	if count <= visible {
		*offset = 0
		return
	}
	if direction < 0 {
		*offset -= visible
	} else {
		*offset += visible
	}
	*offset = clampInt(*offset, 0, count-visible)
}

// overlayRowBudget is how many content lines remain after header lines.
func (m *Model) overlayRowBudget(header int) int {
	if m == nil {
		return 1
	}
	budget := m.overlayCardRowBudget() - header
	if budget < 1 {
		return 1
	}
	return budget
}

// overlayOrigin is the card's top-left cell. compositeOverlay and hit-testing
// both call it so the card cannot be drawn in one place and clicked in another.
func overlayOrigin(bodyWidth, cardWidth int) (x, y int) {
	x = (bodyWidth - cardWidth) / 2
	if x < 0 {
		x = 0
	}
	y = overlayCardTopInset
	return x, y
}

// columnRects is the outer geometry of the three columns. The transcript rect
// stops above the composer; the side columns include their frames and run the
// full body height. Widths come from columnLayout, the same split render uses.
func (m *Model) columnRects() (sidebar, transcript, members cellRect) {
	if m == nil {
		return
	}
	bodyH := m.bodyHeight()
	sidebarW, transcriptW, membersW := m.columnLayout()
	if sidebarW > 0 {
		sidebar = cellRect{X: 0, Y: 0, W: sidebarW, H: bodyH}
	}
	transcript = cellRect{X: sidebarW, Y: 0, W: transcriptW, H: m.transcriptHeight()}
	if membersW > 0 {
		members = cellRect{X: sidebarW + transcriptW, Y: 0, W: membersW, H: bodyH}
	}
	return sidebar, transcript, members
}

// framedInner is the content box inside a framed column. It follows
// framedColumn: a border that does not fit falls back to the outer rect.
func (m *Model) framedInner(outer cellRect) (x, y, w, h int) {
	if m == nil || outer.empty() {
		return
	}
	fx, fy := m.styles.Panel.GetFrameSize()
	innerW := outer.W - fx
	innerH := outer.H - fy
	if innerW < 1 || innerH < 1 {
		return outer.X, outer.Y, outer.W, outer.H
	}
	return outer.X + fx/2, outer.Y + fy/2, innerW, innerH
}

// overlayContentWidth is the card's inner width, the same value
// overlayCardFramed passes to the body builder.
func (m *Model) overlayContentWidth() int {
	if m == nil {
		return 8
	}
	fx, _ := m.styles.Panel.GetFrameSize()
	inner := m.width - 2*overlayCardInset - fx
	if inner < 8 {
		inner = 8
	}
	return inner
}

// overlayCardRect is the card the compositor places, clipped to the body so a
// row past the fold is not clickable.
func (m *Model) overlayCardRect() (cellRect, bool) {
	if m == nil {
		return cellRect{}, false
	}
	card, ok := m.overlayCard()
	if !ok || card == "" {
		return cellRect{}, false
	}
	w := lipgloss.Width(card)
	h := lipgloss.Height(card)
	x, y := overlayOrigin(m.width, w)
	rect := cellRect{X: x, Y: y, W: w, H: h}
	bodyH := m.bodyHeight()
	if rect.Y >= bodyH || rect.H < 1 {
		return cellRect{}, false
	}
	if rect.Y+rect.H > bodyH {
		rect.H = bodyH - rect.Y
	}
	return rect, true
}

// pointerDismissable reports whether a click outside the card closes it.
// Connect, About, and the shortcuts sheet stay keyboard-owned.
func (m *Model) pointerDismissable() bool {
	return m != nil && (m.jump.open || m.nick.open || m.link.open || m.inbox.open || m.list.open)
}

// activeOverlayLines is the open list's content lines, or nil when the open
// card is not one of the clickable lists.
func (m *Model) activeOverlayLines(inner int) []overlayLine {
	if m == nil {
		return nil
	}
	switch {
	case m.jump.open:
		return m.jumpOverlayLines(inner)
	case m.nick.open:
		return m.nickOverlayLines(inner)
	case m.link.open:
		return m.linkOverlayLines(inner)
	case m.inbox.open:
		return m.inboxOverlayLines(inner)
	case m.list.open:
		return m.listOverlayLines(inner)
	default:
		return nil
	}
}

// screenLayout is the clickable geometry for the current model. It is computed
// from the same column split, frame inset, sidebar slots, member slots, and
// overlay lines the draw path uses.
type screenLayout struct {
	sidebar    cellRect
	transcript cellRect
	members    cellRect
	overlay    cellRect
	overlayOn  bool

	sidebarHits []pointerHit
	memberHits  []pointerHit
	overlayHits []pointerHit
}

func (m *Model) screenLayout() screenLayout {
	var layout screenLayout
	if m == nil || m.ctrl == nil || m.terminalTooSmall() {
		return layout
	}
	layout.sidebar, layout.transcript, layout.members = m.columnRects()
	layout.sidebarHits = m.sidebarPointerHits(layout.sidebar)
	layout.memberHits = m.memberPointerHits(layout.members)
	if rect, ok := m.overlayCardRect(); ok {
		layout.overlay = rect
		layout.overlayOn = true
		layout.overlayHits = m.overlayPointerHits(rect)
	}
	return layout
}

func (m *Model) sidebarPointerHits(outer cellRect) []pointerHit {
	x, y, w, h := m.framedInner(outer)
	if w < 1 || h < 1 {
		return nil
	}
	slots := m.sidebarVisibleSlots(w, h)
	hits := make([]pointerHit, 0, len(slots))
	for index, slot := range slots {
		if index >= h {
			break
		}
		kind := hitNone
		switch slot.kind {
		case sidebarSlotHeader:
			kind = hitSidebarHeader
		case sidebarSlotConversation:
			kind = hitSidebarRow
		default:
			continue
		}
		hits = append(hits, pointerHit{
			kind:      kind,
			rect:      cellRect{X: x, Y: y + index, W: w, H: 1},
			networkID: slot.networkID,
			target:    slot.target,
		})
	}
	return hits
}

func (m *Model) memberPointerHits(outer cellRect) []pointerHit {
	x, y, w, h := m.framedInner(outer)
	if w < 1 || h < 1 {
		return nil
	}
	slots := m.memberVisibleSlots(w, h)
	hits := make([]pointerHit, 0, len(slots))
	seen := map[int]bool{}
	for index, slot := range slots {
		if index >= h || slot.index < 0 || seen[slot.index] {
			continue
		}
		// The whole member block is one target. A status subline belongs to
		// the nick above it, so the first line of the block owns the rect and
		// later lines of the same member extend it.
		seen[slot.index] = true
		height := 1
		for next := index + 1; next < len(slots) && next < h && slots[next].index == slot.index; next++ {
			height++
		}
		hits = append(hits, pointerHit{
			kind:  hitMember,
			rect:  cellRect{X: x, Y: y + index, W: w, H: height},
			index: slot.index,
		})
	}
	return hits
}

func (m *Model) overlayPointerHits(card cellRect) []pointerHit {
	fx, fy := m.styles.Panel.GetFrameSize()
	innerW := card.W - fx
	innerH := card.H - fy
	if innerW < 1 || innerH < 1 {
		return nil
	}
	lines := m.activeOverlayLines(m.overlayContentWidth())
	x := card.X + fx/2
	y := card.Y + fy/2
	hits := make([]pointerHit, 0, len(lines))
	for index, line := range lines {
		if line.row < 0 || index >= innerH {
			continue
		}
		hits = append(hits, pointerHit{
			kind:  hitOverlayRow,
			rect:  cellRect{X: x, Y: y + index, W: innerW, H: 1},
			index: line.row,
		})
	}
	return hits
}

// hitTest resolves the topmost target at a cell. An open card wins over the
// columns underneath it, including the dimmed area outside the card.
func (m *Model) hitTest(x, y int) pointerHit {
	layout := m.screenLayout()
	if layout.overlayOn {
		if layout.overlay.contains(x, y) {
			for _, hit := range layout.overlayHits {
				if hit.rect.contains(x, y) {
					return hit
				}
			}
			return pointerHit{kind: hitOverlay, rect: layout.overlay}
		}
		if m.pointerDismissable() {
			return pointerHit{kind: hitOverlayOutside}
		}
		return pointerHit{}
	}
	for _, hit := range layout.sidebarHits {
		if hit.rect.contains(x, y) {
			return hit
		}
	}
	if layout.sidebar.contains(x, y) {
		return pointerHit{kind: hitNone, rect: layout.sidebar}
	}
	for _, hit := range layout.memberHits {
		if hit.rect.contains(x, y) {
			return hit
		}
	}
	if layout.transcript.contains(x, y) {
		return pointerHit{kind: hitTranscript, rect: layout.transcript}
	}
	if layout.members.contains(x, y) {
		return pointerHit{kind: hitNone, rect: layout.members}
	}
	return pointerHit{}
}
