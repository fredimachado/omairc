package controller

import (
	"sort"
	"strings"
	"time"

	"github.com/fredimachado/omairc/tui/internal/session"
)

// This file is the Go port of src/irc/ircchannellistrequest.{h,cpp} and
// src/irc/channellistmodel.{h,cpp}: the per-network /list request state
// machine, the row cache, and the /list overlay model. It also carries a
// private port of the two IrcTextFormatter helpers the channel list needs
// (stripIrcColors and plainIrcText); phase 10 moves those to a real formatter.
//
// All timers go through the injected session.Clock so tests drive the 30s idle
// timeout with session.FakeClock.

// ChannelListMaxRows is the cap on cached channel-list rows. It mirrors
// IrcChannelListRequest's use of ChannelListModel::kMaxRows in the C++
// original.
const ChannelListMaxRows = 8000

// channelListDefaultIdleTimeout mirrors IrcChannelListRequest's default
// m_idleTimeoutMs of 30000.
const channelListDefaultIdleTimeout = 30 * time.Second

// channelListMinIdleTimeout is the floor setIdleTimeoutMs applies.
const channelListMinIdleTimeout = time.Millisecond

// channelListTimeoutText mirrors the fireIdleTimeout text in
// ircchannellistrequest.cpp.
const channelListTimeoutText = "Channel list timed out. Try /list again."

// ChannelListRow is one channel-list entry. It mirrors IrcChannelListRow.
type ChannelListRow struct {
	Channel string
	Users   int
	Topic   string
}

// ChannelListRequestState is a snapshot of one network's request. It mirrors
// IrcChannelListRequest::State.
type ChannelListRequestState struct {
	Mask             string
	Rows             []ChannelListRow
	Complete         bool
	Loading          bool
	Error            string
	PendingMask      string
	HasPendingMask   bool
	DrainTimedOutEnd bool
}

// ChannelListRequestKind is the outcome of ChannelListRequest.Request, mirroring
// IrcChannelListRequest::Request.
type ChannelListRequestKind int

const (
	// ChannelListStart means a fresh load was started.
	ChannelListStart ChannelListRequestKind = iota
	// ChannelListLoading means the network is already loading; a different mask
	// becomes the pending mask.
	ChannelListLoading
	// ChannelListCached means a completed request for the same mask is reused.
	ChannelListCached
)

// ChannelListFinishResult is the outcome of ChannelListRequest.Finish, mirroring
// IrcChannelListRequest::FinishResult.
type ChannelListFinishResult struct {
	Completed   bool
	PendingMask string
	HasPending  bool
}

// channelListEntry is the mutable per-network request record behind a
// ChannelListRequestState snapshot.
type channelListEntry struct {
	mask             string
	rows             []ChannelListRow
	complete         bool
	loading          bool
	errorText        string
	pendingMask      string
	hasPendingMask   bool
	drainTimedOutEnd bool
}

// ChannelListRequest owns per-network request state, the row cache, and the 30s
// idle timeout. It mirrors IrcChannelListRequest.
//
// A ChannelListRequest is not safe for concurrent use; see the package doc.
type ChannelListRequest struct {
	clock       session.Clock
	idleTimeout time.Duration
	states      map[string]*channelListEntry
	timers      map[string]session.Timer
}

// NewChannelListRequest returns an empty request registry that arms its idle
// timers on clock.
func NewChannelListRequest(clock session.Clock) *ChannelListRequest {
	if clock == nil {
		clock = session.NewRealClock()
	}
	return &ChannelListRequest{
		clock:       clock,
		idleTimeout: channelListDefaultIdleTimeout,
		states:      make(map[string]*channelListEntry),
		timers:      make(map[string]session.Timer),
	}
}

// State returns a snapshot of one network's request and whether it exists. The
// rows are copied, so the caller cannot mutate the cache.
func (r *ChannelListRequest) State(networkID string) (ChannelListRequestState, bool) {
	entry, ok := r.states[networkID]
	if !ok {
		return ChannelListRequestState{}, false
	}
	return ChannelListRequestState{
		Mask:             entry.mask,
		Rows:             channelListCopyRows(entry.rows),
		Complete:         entry.complete,
		Loading:          entry.loading,
		Error:            entry.errorText,
		PendingMask:      entry.pendingMask,
		HasPendingMask:   entry.hasPendingMask,
		DrainTimedOutEnd: entry.drainTimedOutEnd,
	}, true
}

// Request starts a /list load for one network. While a load is in flight a
// same-mask request is a no-op and a different mask becomes the pending mask; a
// completed request for the same mask is reused unless refresh is set. It
// mirrors IrcChannelListRequest::request.
func (r *ChannelListRequest) Request(networkID, mask string, refresh bool) ChannelListRequestKind {
	existing, ok := r.states[networkID]
	if ok && existing.loading {
		if SameChannelListMask(existing.mask, mask) {
			existing.pendingMask = ""
			existing.hasPendingMask = false
		} else {
			existing.pendingMask = mask
			existing.hasPendingMask = true
		}
		return ChannelListLoading
	}
	if ok && !refresh && existing.complete && SameChannelListMask(existing.mask, mask) {
		return ChannelListCached
	}
	drain := false
	if ok {
		drain = existing.drainTimedOutEnd
	}
	r.states[networkID] = &channelListEntry{
		mask:             mask,
		loading:          true,
		drainTimedOutEnd: drain,
	}
	r.arm(networkID)
	return ChannelListStart
}

// CancelStart drops the request and its timer, mirroring cancelStart.
func (r *ChannelListRequest) CancelStart(networkID string) {
	r.Forget(networkID)
}

// Row stores one streamed row. It reports whether an existing row was replaced.
// Empty channels and rows for a network that is not loading are ignored, and a
// new row is dropped once the cache is at ChannelListMaxRows. It mirrors
// IrcChannelListRequest::row.
func (r *ChannelListRequest) Row(networkID string, row ChannelListRow) bool {
	if row.Channel == "" {
		return false
	}
	entry, ok := r.states[networkID]
	if !ok || !entry.loading {
		return false
	}
	r.arm(networkID)
	for index := range entry.rows {
		if strings.EqualFold(entry.rows[index].Channel, row.Channel) {
			entry.rows[index] = row
			return true
		}
	}
	if len(entry.rows) >= ChannelListMaxRows {
		return false
	}
	entry.rows = append(entry.rows, row)
	return false
}

// Activity re-arms the idle timer for a network that is still loading, mirroring
// activity.
func (r *ChannelListRequest) Activity(networkID string) {
	entry, ok := r.states[networkID]
	if ok && entry.loading {
		r.arm(networkID)
	}
}

// Finish ends a streamed load. It mirrors IrcChannelListRequest::finish: a
// network that is not loading clears the drain flag and reports nothing; a
// network draining a stale END clears the flag, re-arms, and reports nothing;
// otherwise the timer stops, the request completes, and a pending mask that
// differs from the current mask is returned.
func (r *ChannelListRequest) Finish(networkID string) ChannelListFinishResult {
	entry, ok := r.states[networkID]
	if !ok {
		return ChannelListFinishResult{}
	}
	if !entry.loading {
		entry.drainTimedOutEnd = false
		return ChannelListFinishResult{}
	}
	if entry.drainTimedOutEnd {
		entry.drainTimedOutEnd = false
		r.arm(networkID)
		return ChannelListFinishResult{}
	}
	r.stop(networkID)
	pending := entry.pendingMask
	hasPending := entry.hasPendingMask
	entry.pendingMask = ""
	entry.hasPendingMask = false
	entry.loading = false
	entry.complete = true
	entry.errorText = ""
	if hasPending && !SameChannelListMask(pending, entry.mask) {
		return ChannelListFinishResult{Completed: true, PendingMask: pending, HasPending: true}
	}
	return ChannelListFinishResult{Completed: true}
}

// Fail ends a load in error. It only applies while loading and reports whether
// anything changed. It mirrors IrcChannelListRequest::fail.
func (r *ChannelListRequest) Fail(networkID, text string, timedOut bool) bool {
	entry, ok := r.states[networkID]
	if !ok || !entry.loading {
		return false
	}
	r.stop(networkID)
	entry.loading = false
	entry.complete = false
	entry.pendingMask = ""
	entry.hasPendingMask = false
	entry.errorText = text
	if timedOut {
		entry.drainTimedOutEnd = true
	}
	return true
}

// Forget stops the timer and drops all state for one network, mirroring forget.
func (r *ChannelListRequest) Forget(networkID string) {
	r.stop(networkID)
	delete(r.states, networkID)
}

// SetIdleTimeout sets the idle timeout, clamped to at least 1ms. It mirrors
// setIdleTimeoutMs.
func (r *ChannelListRequest) SetIdleTimeout(d time.Duration) {
	if d < channelListMinIdleTimeout {
		d = channelListMinIdleTimeout
	}
	r.idleTimeout = d
}

// FireIdleTimeout fails a loading request with the timeout text and reports
// whether a request was actually failed. It mirrors fireIdleTimeout and is the
// entry point the armed timer calls.
func (r *ChannelListRequest) FireIdleTimeout(networkID string) bool {
	return r.Fail(networkID, channelListTimeoutText, true)
}

// SameChannelListMask reports whether two masks are equal after trimming,
// case-insensitively. It mirrors IrcChannelListRequest::sameMask.
func SameChannelListMask(left, right string) bool {
	return strings.EqualFold(strings.TrimSpace(left), strings.TrimSpace(right))
}

// arm restarts one network's idle timer. The C++ original reuses a single
// QTimer; session.Timer has no Reset, so the old timer is stopped and a fresh
// one is scheduled, which has the same one-shot deadline semantics.
func (r *ChannelListRequest) arm(networkID string) {
	if timer, ok := r.timers[networkID]; ok && timer != nil {
		timer.Stop()
	}
	r.timers[networkID] = r.clock.AfterFunc(r.idleTimeout, func() {
		r.FireIdleTimeout(networkID)
	})
}

// stop cancels and forgets one network's idle timer.
func (r *ChannelListRequest) stop(networkID string) {
	if timer, ok := r.timers[networkID]; ok {
		if timer != nil {
			timer.Stop()
		}
		delete(r.timers, networkID)
	}
}

func channelListCopyRows(rows []ChannelListRow) []ChannelListRow {
	if len(rows) == 0 {
		return nil
	}
	out := make([]ChannelListRow, len(rows))
	copy(out, rows)
	return out
}

// ChannelListModelState is a snapshot for the /list overlay. Rows are the
// visible rows sorted by users descending then channel case-insensitively.
type ChannelListModelState struct {
	NetworkID string
	Mask      string
	Rows      []ChannelListRow
	Sources   int
	Loading   bool
	Complete  bool
	Cached    bool
	Error     string
	Filter    string
}

// ChannelListModel is the /list overlay source: a cached row set, a fuzzy
// filter, and a users-descending sort. It mirrors ChannelListModel.
//
// A ChannelListModel is not safe for concurrent use; see the package doc.
type ChannelListModel struct {
	source        []ChannelListRow
	visible       []int
	visibleSorted bool
	filter        string
	filterQuery   string
	networkID     string
	mask          string
	errorText     string
	loading       bool
	complete      bool
	cached        bool
}

// NewChannelListModel returns an empty channel-list model.
func NewChannelListModel() *ChannelListModel {
	return &ChannelListModel{}
}

// Show replaces the cache with rows, capped at ChannelListMaxRows and sorted by
// users descending then channel. It mirrors ChannelListModel::show.
func (m *ChannelListModel) Show(
	networkID, mask string,
	rows []ChannelListRow,
	complete, loading, cached bool,
	err string,
) {
	if len(rows) > ChannelListMaxRows {
		rows = rows[:ChannelListMaxRows]
	}
	sorted := channelListCopyRows(rows)
	sort.SliceStable(sorted, func(left, right int) bool {
		return channelListLess(sorted[left], sorted[right])
	})
	m.networkID = networkID
	m.mask = mask
	m.source = sorted
	m.complete = complete
	m.loading = loading
	m.cached = cached
	m.errorText = err
	m.rebuildVisible()
}

// BeginLoad resets the model for a streamed load. It mirrors
// ChannelListModel::beginLoad.
func (m *ChannelListModel) BeginLoad(networkID, mask string) {
	m.networkID = networkID
	m.mask = mask
	m.source = nil
	m.visible = nil
	m.visibleSorted = true
	m.complete = false
	m.loading = true
	m.cached = false
	m.errorText = ""
}

// AppendRow stores one streamed row unless the cache is at ChannelListMaxRows,
// adding it to the visible set only when it matches the current filter. It
// mirrors ChannelListModel::appendRow.
func (m *ChannelListModel) AppendRow(row ChannelListRow) {
	if len(m.source) >= ChannelListMaxRows {
		return
	}
	index := len(m.source)
	m.source = append(m.source, row)
	if m.matches(row) {
		m.visible = append(m.visible, index)
		m.visibleSorted = false
	}
}

// Fail marks the model failed. It mirrors ChannelListModel::fail.
func (m *ChannelListModel) Fail(err string) {
	m.loading = false
	m.complete = false
	m.cached = false
	m.errorText = err
}

// Clear resets the model. It is a no-op when the model is already empty, and it
// preserves the filter. It mirrors ChannelListModel::clear.
func (m *ChannelListModel) Clear() {
	if len(m.source) == 0 && m.networkID == "" && m.errorText == "" && !m.loading && !m.complete {
		return
	}
	m.source = nil
	m.visible = nil
	m.visibleSorted = true
	m.networkID = ""
	m.mask = ""
	m.errorText = ""
	m.loading = false
	m.complete = false
	m.cached = false
}

// Filter returns the current filter text.
func (m *ChannelListModel) Filter() string {
	return m.filter
}

// SetFilter stores filter and rebuilds the visible set. It mirrors
// ChannelListModel::setFilter.
func (m *ChannelListModel) SetFilter(filter string) {
	if m.filter == filter {
		return
	}
	m.filter = filter
	m.filterQuery = strings.ToLower(strings.TrimSpace(filter))
	m.rebuildVisible()
}

// State returns a snapshot of the model for the overlay.
func (m *ChannelListModel) State() ChannelListModelState {
	m.ensureVisibleSorted()
	var rows []ChannelListRow
	if len(m.visible) > 0 {
		rows = make([]ChannelListRow, len(m.visible))
		for index, source := range m.visible {
			rows[index] = m.source[source]
		}
	}
	return ChannelListModelState{
		NetworkID: m.networkID,
		Mask:      m.mask,
		Rows:      rows,
		Sources:   len(m.source),
		Loading:   m.loading,
		Complete:  m.complete,
		Cached:    m.cached,
		Error:     m.errorText,
		Filter:    m.filter,
	}
}

// PlainTopic strips mIRC colors and formatting from topic, mirroring
// plainIrcText.
func (m *ChannelListModel) PlainTopic(topic string) string {
	return channelListPlainText(topic)
}

// rebuildVisible recomputes the visible index set and sorts it by users
// descending then channel. It mirrors ChannelListModel::rebuildVisible.
func (m *ChannelListModel) rebuildVisible() {
	next := make([]int, 0, len(m.source))
	for index := range m.source {
		if m.matches(m.source[index]) {
			next = append(next, index)
		}
	}
	sort.SliceStable(next, func(left, right int) bool {
		return channelListLess(m.source[next[left]], m.source[next[right]])
	})
	m.visible = next
	m.visibleSorted = true
}

// ensureVisibleSorted sorts visible in place after a streaming append. The C++
// model leaves m_visible in stream order until rebuildVisible runs; State
// presents the documented users-descending order without disturbing AppendRow's
// semantics.
func (m *ChannelListModel) ensureVisibleSorted() {
	if m.visibleSorted {
		return
	}
	sort.SliceStable(m.visible, func(left, right int) bool {
		return channelListLess(m.source[m.visible[left]], m.source[m.visible[right]])
	})
	m.visibleSorted = true
}

// matches reports whether row passes the current filter. It mirrors
// ChannelListModel::matches: an empty query matches all, otherwise a
// case-insensitive substring of the channel or the plain-text topic.
func (m *ChannelListModel) matches(row ChannelListRow) bool {
	if m.filterQuery == "" {
		return true
	}
	return strings.Contains(strings.ToLower(row.Channel), m.filterQuery) ||
		strings.Contains(strings.ToLower(channelListPlainText(row.Topic)), m.filterQuery)
}

// channelListLess orders rows by users descending then channel
// case-insensitively. It mirrors the anonymous rowLess in channellistmodel.cpp.
func channelListLess(left, right ChannelListRow) bool {
	if left.Users != right.Users {
		return left.Users > right.Users
	}
	return strings.ToLower(left.Channel) < strings.ToLower(right.Channel)
}

// The mIRC control codes stripIrcColors and plainIrcText understand.
const (
	channelListBold          = 0x02
	channelListColor         = 0x03
	channelListHexColor      = 0x04
	channelListReset         = 0x0f
	channelListMonospace     = 0x11
	channelListReverse       = 0x16
	channelListItalic        = 0x1d
	channelListStrikethrough = 0x1e
	channelListUnderline     = 0x1f
)

// channelListStripColors removes mIRC color and hex-color codes, mirroring
// IrcTextFormatter::stripIrcColors. It walks bytes: the consumed sequence is
// always ASCII, and no UTF-8 continuation byte can equal a control code, so
// byte indexing preserves the rest of the UTF-8 text.
func channelListStripColors(text string) string {
	var out strings.Builder
	out.Grow(len(text))
	for index := 0; index < len(text); index++ {
		switch text[index] {
		case channelListColor:
			index = channelListConsumeMircColor(text, index)
		case channelListHexColor:
			index = channelListConsumeHexColor(text, index)
		default:
			out.WriteByte(text[index])
		}
	}
	return out.String()
}

// channelListPlainText strips colors and then drops the formatting control
// codes, keeping everything else. It mirrors IrcTextFormatter::plainIrcText.
func channelListPlainText(text string) string {
	stripped := channelListStripColors(text)
	var out strings.Builder
	out.Grow(len(stripped))
	for index := 0; index < len(stripped); index++ {
		switch stripped[index] {
		case channelListBold, channelListReset, channelListMonospace,
			channelListReverse, channelListItalic, channelListStrikethrough,
			channelListUnderline:
		default:
			out.WriteByte(stripped[index])
		}
	}
	return out.String()
}

func channelListASCIIDigit(code byte) bool {
	return code >= '0' && code <= '9'
}

func channelListASCIIHex(code byte) bool {
	return channelListASCIIDigit(code) ||
		(code >= 'A' && code <= 'F') ||
		(code >= 'a' && code <= 'f')
}

// channelListSixHexAt reports whether text carries six hex digits at start.
func channelListSixHexAt(text string, start int) bool {
	if start+5 >= len(text) {
		return false
	}
	for offset := 0; offset < 6; offset++ {
		if !channelListASCIIHex(text[start+offset]) {
			return false
		}
	}
	return true
}

// channelListConsumeMircColor returns the last index of
// \x03(?:\d{1,2}(?:,\d{1,2})?)?, or index when nothing follows. It mirrors
// consumeMircColor.
func channelListConsumeMircColor(text string, index int) int {
	size := len(text)
	i := index + 1
	if i >= size || !channelListASCIIDigit(text[i]) {
		return index
	}
	i++
	if i < size && channelListASCIIDigit(text[i]) {
		i++
	}
	if i < size && text[i] == ',' && i+1 < size && channelListASCIIDigit(text[i+1]) {
		i += 2
		if i < size && channelListASCIIDigit(text[i]) {
			i++
		}
	}
	return i - 1
}

// channelListConsumeHexColor returns the last index of
// \x04(?:[0-9A-Fa-f]{6}(?:,[0-9A-Fa-f]{6})?)?, or index when nothing follows.
// It mirrors consumeHexColor.
func channelListConsumeHexColor(text string, index int) int {
	size := len(text)
	i := index + 1
	if !channelListSixHexAt(text, i) {
		return index
	}
	i += 6
	if i < size && text[i] == ',' && channelListSixHexAt(text, i+1) {
		i += 7
	}
	return i - 1
}
