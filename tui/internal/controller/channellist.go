package controller

import (
	"sort"
	"strings"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
	"github.com/fredimachado/omairc/tui/internal/session"
)

// Per-network /list request state machine, row cache, and /list overlay model.
// Twin: `client/src/controller/channellist.rs`. Topic strip uses
// irc.PlainIrcText. All timers go through the injected session.Clock so tests
// drive the 30s idle timeout with session.FakeClock.
//
// Each network holds one channelListPhase (loading, complete, or failed; no entry is
// idle), so a load is never both loading and complete. The idle timer's generation
// sits beside the phase and identifies the armed callback.

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
const channelListTimeoutText = "Channel list timed out. Try /list again."

// ChannelListRow is one channel-list entry. It mirrors IrcChannelListRow.
type ChannelListRow struct {
	Channel string
	Users   int
	Topic   string
}

// ChannelListRequestState is a snapshot of one network's request. It mirrors
// IrcChannelListRequest::State. The flags are a read-only view derived from the
// network's phase, not stored separately.
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

// channelListPhase is one network's /list lifecycle. A load is exactly one phase,
// so it is never both loading and complete, and a failed load is not complete. A
// network with no entry is idle. Each variant is a pointer: Row mutates a loading
// load's rows in place, while every transition stores a new value, so an old pointer
// can never become a second phase.
type channelListPhase interface{ channelListPhase() }

// channelListLoading is a load in flight: its mask, the rows streamed so far, at most
// one queued mask, and whether a timed-out earlier load's stale END is still to come.
type channelListLoading struct {
	mask          string
	rows          []ChannelListRow
	queuedMask    string
	hasQueuedMask bool
	drainStaleEnd bool
}

// channelListComplete is a finished load. A same-mask request reuses its rows unless
// refresh is set.
type channelListComplete struct {
	mask string
	rows []ChannelListRow
}

// channelListFailed is a load that ended in error. It keeps the rows it had, and
// drainStaleEnd is set while a timed-out load's stale END is still to come.
type channelListFailed struct {
	mask          string
	rows          []ChannelListRow
	errorText     string
	drainStaleEnd bool
}

func (*channelListLoading) channelListPhase()  {}
func (*channelListComplete) channelListPhase() {}
func (*channelListFailed) channelListPhase()   {}

// channelListTimer is one network's armed idle timer. gen identifies that arming, so
// a callback that fired before a newer arming is ignored.
type channelListTimer struct {
	gen   uint64
	timer session.Timer
}

// ChannelListRequest owns per-network request state, the row cache, and the 30s
// idle timeout. It mirrors IrcChannelListRequest.
// A ChannelListRequest is not safe for concurrent use; see the package doc.
type ChannelListRequest struct {
	clock       session.Clock
	idleTimeout time.Duration
	states      map[string]channelListPhase
	timers      map[string]channelListTimer
	generation  uint64
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
		states:      make(map[string]channelListPhase),
		timers:      make(map[string]channelListTimer),
	}
}

// State returns a snapshot of one network's request and whether it exists. The rows
// are copied, so the caller cannot mutate the cache. The flags are read from the
// phase, so they cannot disagree: the snapshot is a read-only view, not storage.
func (r *ChannelListRequest) State(networkID string) (ChannelListRequestState, bool) {
	switch phase := r.states[networkID].(type) {
	case *channelListLoading:
		return ChannelListRequestState{
			Mask:             phase.mask,
			Rows:             channelListCopyRows(phase.rows),
			Loading:          true,
			PendingMask:      phase.queuedMask,
			HasPendingMask:   phase.hasQueuedMask,
			DrainTimedOutEnd: phase.drainStaleEnd,
		}, true
	case *channelListComplete:
		return ChannelListRequestState{
			Mask:     phase.mask,
			Rows:     channelListCopyRows(phase.rows),
			Complete: true,
		}, true
	case *channelListFailed:
		return ChannelListRequestState{
			Mask:             phase.mask,
			Rows:             channelListCopyRows(phase.rows),
			Error:            phase.errorText,
			DrainTimedOutEnd: phase.drainStaleEnd,
		}, true
	default:
		return ChannelListRequestState{}, false
	}
}

// Request starts a /list load for one network. While a load is in flight a
// same-mask request is a no-op and a different mask takes the single queued slot; a
// completed request for the same mask is reused unless refresh is set. It mirrors
// IrcChannelListRequest::request.
func (r *ChannelListRequest) Request(networkID, mask string, refresh bool) ChannelListRequestKind {
	switch phase := r.states[networkID].(type) {
	case *channelListLoading:
		if SameChannelListMask(phase.mask, mask) {
			phase.queuedMask = ""
			phase.hasQueuedMask = false
		} else {
			phase.queuedMask = mask
			phase.hasQueuedMask = true
		}
		return ChannelListLoading
	case *channelListComplete:
		if !refresh && SameChannelListMask(phase.mask, mask) {
			return ChannelListCached
		}
	}
	// Only a failed load that timed out hands its stale END to the next load.
	drain := false
	if failed, ok := r.states[networkID].(*channelListFailed); ok {
		drain = failed.drainStaleEnd
	}
	r.states[networkID] = &channelListLoading{mask: mask, drainStaleEnd: drain}
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
	loading, ok := r.states[networkID].(*channelListLoading)
	if !ok {
		return false
	}
	r.arm(networkID)
	for index := range loading.rows {
		if strings.EqualFold(loading.rows[index].Channel, row.Channel) {
			loading.rows[index] = row
			return true
		}
	}
	if len(loading.rows) >= ChannelListMaxRows {
		return false
	}
	loading.rows = append(loading.rows, row)
	return false
}

// Activity re-arms the idle timer for a network that is still loading, mirroring
// activity.
func (r *ChannelListRequest) Activity(networkID string) {
	if _, ok := r.states[networkID].(*channelListLoading); ok {
		r.arm(networkID)
	}
}

// Finish ends a streamed load. It mirrors IrcChannelListRequest::finish. A failed
// request clears the stale END it is still owed and reports nothing. A load draining a
// stale END clears the flag, re-arms, and reports nothing. Otherwise the load
// completes, and a queued mask that differs from its mask is returned to start next.
func (r *ChannelListRequest) Finish(networkID string) ChannelListFinishResult {
	switch phase := r.states[networkID].(type) {
	case *channelListLoading:
		if phase.drainStaleEnd {
			phase.drainStaleEnd = false
			r.arm(networkID)
			return ChannelListFinishResult{}
		}
		return r.completeLoad(networkID, phase)
	case *channelListFailed:
		// The failed load keeps its error and rows. Its stale END is absorbed here.
		phase.drainStaleEnd = false
	}
	return ChannelListFinishResult{}
}

// completeLoad moves a loading request to complete. The queued mask is handed back
// only when it differs from the completed mask.
func (r *ChannelListRequest) completeLoad(networkID string, loading *channelListLoading) ChannelListFinishResult {
	r.stop(networkID)
	r.states[networkID] = &channelListComplete{mask: loading.mask, rows: loading.rows}
	if loading.hasQueuedMask && !SameChannelListMask(loading.queuedMask, loading.mask) {
		return ChannelListFinishResult{Completed: true, PendingMask: loading.queuedMask, HasPending: true}
	}
	return ChannelListFinishResult{Completed: true}
}

// Fail ends a load in error. It only applies while loading and reports whether
// anything changed. The rows it had stay and the queued mask is dropped. A timed-out
// failure leaves its stale END to drain, and so does a failure of a load that was
// already draining one. It mirrors IrcChannelListRequest::fail.
func (r *ChannelListRequest) Fail(networkID, text string, timedOut bool) bool {
	loading, ok := r.states[networkID].(*channelListLoading)
	if !ok {
		return false
	}
	r.stop(networkID)
	r.states[networkID] = &channelListFailed{
		mask:          loading.mask,
		rows:          loading.rows,
		errorText:     text,
		drainStaleEnd: loading.drainStaleEnd || timedOut,
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

// FireIdleTimeout fails the network's current load with the timeout text and reports
// whether a load was actually failed. It mirrors fireIdleTimeout. The armed timer does
// not call it: the timer calls fireIdle with its own generation.
func (r *ChannelListRequest) FireIdleTimeout(networkID string) bool {
	armed, ok := r.timers[networkID]
	if !ok {
		return false
	}
	return r.fireIdle(networkID, armed.gen)
}

// fireIdle is what an armed idle timer runs. It fails the load only while gen is still
// the network's armed timer, so a callback that fired before a newer arming cannot fail
// that newer load.
func (r *ChannelListRequest) fireIdle(networkID string, gen uint64) bool {
	armed, ok := r.timers[networkID]
	if !ok || armed.gen != gen {
		return false
	}
	return r.Fail(networkID, channelListTimeoutText, true)
}

// SameChannelListMask reports whether two masks are equal after trimming
// case-insensitively. It mirrors IrcChannelListRequest::sameMask.
func SameChannelListMask(left, right string) bool {
	return strings.EqualFold(strings.TrimSpace(left), strings.TrimSpace(right))
}

// arm restarts one network's idle timer. The C++ original reuses a single
// timer; session.Timer has no Reset, so the old timer is stopped and a fresh one is
// scheduled, which has the same one-shot deadline semantics. Every arming takes a new
// generation.
func (r *ChannelListRequest) arm(networkID string) {
	if armed, ok := r.timers[networkID]; ok && armed.timer != nil {
		armed.timer.Stop()
	}
	r.generation++
	gen := r.generation
	r.timers[networkID] = channelListTimer{
		gen: gen,
		timer: r.clock.AfterFunc(r.idleTimeout, func() {
			r.fireIdle(networkID, gen)
		}),
	}
}

// stop cancels and forgets one network's idle timer.
func (r *ChannelListRequest) stop(networkID string) {
	if armed, ok := r.timers[networkID]; ok {
		if armed.timer != nil {
			armed.timer.Stop()
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

// AppendRow stores one streamed row unless the cache is at ChannelListMaxRows
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
	return irc.PlainIrcText(topic)
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
		strings.Contains(strings.ToLower(irc.PlainIrcText(row.Topic)), m.filterQuery)
}

// channelListLess orders rows by users descending then channel
// case-insensitively. It mirrors the anonymous rowLess in `client/src/controller/channellist.rs`.
func channelListLess(left, right ChannelListRow) bool {
	if left.Users != right.Users {
		return left.Users > right.Users
	}
	return strings.ToLower(left.Channel) < strings.ToLower(right.Channel)
}

// The mIRC strip helpers now live in internal/irc.PlainIrcText, shared with
// the formatter; this file keeps only the channel-list model.
