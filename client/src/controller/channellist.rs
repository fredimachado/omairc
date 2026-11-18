//! `tui/internal/controller/channellist.go`:
//! `/list` through the wire.
//! `LIST mask` goes out as a session write. The server's answer streams back as 321
//! (start), one 322 per channel, then 323 (end), and the controller folds those into a
//! per-network request: one phase per network (idle, loading, complete, or failed) that
//! holds the rows, the mask a second `/list` queued, and the idle timeout. A stale 323
//! after a timeout is
//! drained once, so it does not complete the next load. The overlay (`ChannelListView`)
//! shows the rows of the request once it completes, most users first, then by name.
//! The idle timer runs on the `Clock` seam. As in `autoawayruntime.rs`, a fired timer
//! queues its network and generation, and `pump` runs the timeout on the UI thread.
//! The fuzzy filter the sheet applies is presentation and lives in `model`.

use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::rc::Rc;
use std::time::Duration;

use crate::irc::message::IrcMessage;
use crate::irc::wiretext::wire_text;
use crate::session::clock::{Clock, ClockTimer};
use crate::session::events::SessionState;
use crate::session::session::Session;

use super::controller::Controller;

/// The cap on cached channel-list rows (`ChannelListMaxRows`).
pub const CHANNEL_LIST_MAX_ROWS: usize = 8000;

/// The idle timeout before a stalled `/list` fails (`channelListDefaultIdleTimeout`).
const CHANNEL_LIST_IDLE_TIMEOUT: Duration = Duration::from_secs(30);

/// The shortest idle timeout the setter accepts (`channelListMinIdleTimeout`).
#[cfg(test)]
const CHANNEL_LIST_MIN_IDLE_TIMEOUT: Duration = Duration::from_millis(1);

/// The text a timed-out `/list` leaves on the overlay and the request.
pub const CHANNEL_LIST_TIMEOUT_TEXT: &str = "Channel list timed out. Try /list again.";

/// One channel as a `LIST` answer gives it (`ChannelListRow`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListedChannel {
    pub name: String,
    pub users: usize,
    /// The topic as the server sent it, with its mIRC codes.
    pub topic: String,
}

/// What a `/list` request did (`ChannelListRequest::Request`'s kinds).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RequestKind {
    /// A fresh load was started.
    Start,
    /// The network is already loading; a different mask became the pending mask.
    Loading,
    /// A completed request for the same mask is reused.
    Cached,
}

/// One network's `/list` lifecycle. A load is exactly one phase, so it is never both
/// loading and complete, and a failed load is not complete. A network with no entry is
/// `Idle`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) enum ChannelListPhase {
    #[default]
    Idle,
    Loading {
        mask: String,
        rows: Vec<ListedChannel>,
        /// At most one queued mask (a different `/list` while loading).
        queued_mask: Option<String>,
        /// A timed-out earlier load's stale 323 is still to come.
        drain_stale_end: bool,
    },
    Complete {
        mask: String,
        rows: Vec<ListedChannel>,
    },
    Failed {
        mask: String,
        rows: Vec<ListedChannel>,
        error: String,
        /// The stale 323 of the timed-out load is still to come.
        drain_stale_end: bool,
    },
}

/// The phase a network with no entry is in.
static IDLE: ChannelListPhase = ChannelListPhase::Idle;

impl ChannelListPhase {
    /// The mask of this load, or "" when idle.
    pub(super) fn mask(&self) -> &str {
        match self {
            Self::Idle => "",
            Self::Loading { mask, .. }
            | Self::Complete { mask, .. }
            | Self::Failed { mask, .. } => mask.as_str(),
        }
    }

    /// The rows streamed or completed so far. A failed load keeps the rows it had.
    pub(super) fn rows(&self) -> &[ListedChannel] {
        match self {
            Self::Idle => &[],
            Self::Loading { rows, .. }
            | Self::Complete { rows, .. }
            | Self::Failed { rows, .. } => rows.as_slice(),
        }
    }

    /// The error text of a failed load, or "".
    pub(super) fn error(&self) -> &str {
        match self {
            Self::Failed { error, .. } => error.as_str(),
            _ => "",
        }
    }

    pub(super) fn is_loading(&self) -> bool {
        matches!(self, Self::Loading { .. })
    }

    pub(super) fn is_complete(&self) -> bool {
        matches!(self, Self::Complete { .. })
    }
}

/// The shared queue the idle timers write to: the network and the generation each
/// timer was armed under.
type ExpiryQueue = Rc<RefCell<VecDeque<(String, u64)>>>;

/// The per-network `/list` requests, their row caches and idle timers
/// (`IrcChannelListRequest`).
pub(super) struct ChannelListRequests {
    clock: Rc<dyn Clock>,
    idle_timeout: Duration,
    states: BTreeMap<String, ChannelListPhase>,
    /// One armed timer per network, with the generation it was armed under.
    timers: BTreeMap<String, (u64, Box<dyn ClockTimer>)>,
    generation: u64,
    expiries: ExpiryQueue,
}

impl ChannelListRequests {
    pub(super) fn new(clock: Rc<dyn Clock>) -> Self {
        Self {
            clock,
            idle_timeout: CHANNEL_LIST_IDLE_TIMEOUT,
            states: BTreeMap::new(),
            timers: BTreeMap::new(),
            generation: 0,
            expiries: Rc::new(RefCell::new(VecDeque::new())),
        }
    }

    /// One network's phase. A network with no entry is idle.
    pub(super) fn phase(&self, network_id: &str) -> &ChannelListPhase {
        self.states.get(network_id).unwrap_or(&IDLE)
    }

    /// Starts or joins a `/list` load (`Request`). While a load is in flight a same-mask
    /// request is a no-op and a different mask becomes the pending mask. A completed
    /// request for the same mask is reused unless `refresh` is set.
    pub(super) fn request(&mut self, network_id: &str, mask: &str, refresh: bool) -> RequestKind {
        match self.states.get_mut(network_id) {
            Some(ChannelListPhase::Loading {
                mask: in_flight,
                queued_mask,
                ..
            }) => {
                *queued_mask = if same_mask(in_flight, mask) {
                    None
                } else {
                    Some(mask.to_owned())
                };
                return RequestKind::Loading;
            }
            Some(ChannelListPhase::Complete { mask: done, .. })
                if !refresh && same_mask(done, mask) =>
            {
                return RequestKind::Cached;
            }
            _ => {}
        }
        // Only a failed load that timed out hands its stale end to the next load.
        let drain_stale_end = matches!(
            self.states.get(network_id),
            Some(ChannelListPhase::Failed {
                drain_stale_end: true,
                ..
            })
        );
        self.states.insert(
            network_id.to_owned(),
            ChannelListPhase::Loading {
                mask: mask.to_owned(),
                rows: Vec::new(),
                queued_mask: None,
                drain_stale_end,
            },
        );
        self.arm(network_id);
        RequestKind::Start
    }

    /// Drops a load that never reached the wire (`CancelStart`).
    pub(super) fn cancel_start(&mut self, network_id: &str) {
        self.forget(network_id);
    }

    /// Stores one streamed 322 row. A row replaces the one that has the same channel
    /// under case folding. Empty channels and rows outside a load are ignored, and a new
    /// row past `CHANNEL_LIST_MAX_ROWS` is dropped. Returns whether a row was replaced.
    fn row(&mut self, network_id: &str, row: ListedChannel) -> bool {
        if row.name.is_empty() || !self.phase(network_id).is_loading() {
            return false;
        }
        self.arm(network_id);
        let Some(ChannelListPhase::Loading { rows, .. }) = self.states.get_mut(network_id) else {
            return false;
        };
        if let Some(index) = rows
            .iter()
            .position(|existing| existing.name.to_lowercase() == row.name.to_lowercase())
        {
            rows[index] = row;
            return true;
        }
        if rows.len() < CHANNEL_LIST_MAX_ROWS {
            rows.push(row);
        }
        false
    }

    /// The server is still streaming (321 or a row): the idle window starts over.
    fn activity(&mut self, network_id: &str) {
        if self.phase(network_id).is_loading() {
            self.arm(network_id);
        }
    }

    /// The 323 end of a load (`Finish`). A stale end after a timeout is drained and the
    /// load keeps going. Otherwise the load completes, and a pending mask that differs
    /// from the current one is handed back to start next.
    fn finish(&mut self, network_id: &str) -> FinishResult {
        let draining = match self.states.get_mut(network_id) {
            Some(ChannelListPhase::Loading {
                drain_stale_end, ..
            }) if *drain_stale_end => {
                *drain_stale_end = false;
                true
            }
            Some(ChannelListPhase::Loading { .. }) => false,
            Some(ChannelListPhase::Failed {
                drain_stale_end, ..
            }) => {
                // A failed load keeps its error and rows. Its stale end is absorbed here.
                *drain_stale_end = false;
                return FinishResult::default();
            }
            None | Some(ChannelListPhase::Idle | ChannelListPhase::Complete { .. }) => {
                return FinishResult::default();
            }
        };
        if draining {
            self.arm(network_id);
            return FinishResult::default();
        }
        self.complete_load(network_id)
    }

    /// Moves a loading request to complete. A queued mask that differs from it is handed
    /// back to start next.
    fn complete_load(&mut self, network_id: &str) -> FinishResult {
        self.stop(network_id);
        let Some(ChannelListPhase::Loading {
            mask,
            rows,
            queued_mask,
            ..
        }) = self.states.remove(network_id)
        else {
            return FinishResult::default();
        };
        let next = queued_mask.filter(|queued| !same_mask(queued, &mask));
        self.states.insert(
            network_id.to_owned(),
            ChannelListPhase::Complete { mask, rows },
        );
        FinishResult {
            completed: true,
            pending_mask: next,
        }
    }

    /// Fails an in-flight load (`Fail`). Only a loading request can fail. The rows it had
    /// stay, and a timed-out failure leaves its stale end to drain. Returns whether
    /// anything changed.
    fn fail(&mut self, network_id: &str, text: &str, timed_out: bool) -> bool {
        if !self.phase(network_id).is_loading() {
            return false;
        }
        self.stop(network_id);
        let Some(ChannelListPhase::Loading {
            mask,
            rows,
            drain_stale_end,
            ..
        }) = self.states.remove(network_id)
        else {
            return false;
        };
        self.states.insert(
            network_id.to_owned(),
            ChannelListPhase::Failed {
                mask,
                rows,
                error: text.to_owned(),
                drain_stale_end: drain_stale_end || timed_out,
            },
        );
        true
    }

    /// Drops the timer and every fact about one network (`Forget`).
    pub(super) fn forget(&mut self, network_id: &str) {
        self.stop(network_id);
        self.states.remove(network_id);
    }

    /// Sets the idle timeout, floored at one millisecond (`setIdleTimeoutMs`).
    #[cfg(test)]
    pub(super) fn set_idle_timeout(&mut self, timeout: Duration) {
        self.idle_timeout = timeout.max(CHANNEL_LIST_MIN_IDLE_TIMEOUT);
    }

    /// Takes the next timeout that is still current, skipping timers that were stopped
    /// or re-armed after they fired. The network it returns has its timer cleared.
    pub(super) fn take_expiry(&mut self) -> Option<String> {
        loop {
            let (network_id, generation) = self.expiries.borrow_mut().pop_front()?;
            let current = self
                .timers
                .get(&network_id)
                .is_some_and(|(armed, _)| *armed == generation);
            if current {
                self.timers.remove(&network_id);
                return Some(network_id);
            }
        }
    }

    /// Arms the idle timer for `network_id`, replacing any timer it had.
    fn arm(&mut self, network_id: &str) {
        self.stop(network_id);
        self.generation += 1;
        let generation = self.generation;
        let queue = Rc::clone(&self.expiries);
        let owner = network_id.to_owned();
        let timer = self.clock.after(
            self.idle_timeout,
            Box::new(move || queue.borrow_mut().push_back((owner, generation))),
        );
        self.timers
            .insert(network_id.to_owned(), (generation, timer));
    }

    /// Stops and forgets one network's timer.
    fn stop(&mut self, network_id: &str) {
        if let Some((_, timer)) = self.timers.remove(network_id) {
            timer.stop();
        }
    }
}

/// What a 323 did (`ChannelListRequest::FinishResult`).
#[derive(Debug, Default)]
struct FinishResult {
    completed: bool,
    pending_mask: Option<String>,
}

/// The `/list` overlay's state (`ChannelListModel` with the overlay's open flag). The
/// rows are sorted most users first, then by channel name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChannelListView {
    /// Whether the overlay is showing (`ChannelListSnapshot::Open`).
    pub open: bool,
    pub network_id: String,
    pub mask: String,
    pub rows: Vec<ListedChannel>,
    pub loading: bool,
    pub complete: bool,
    pub cached: bool,
    pub error: String,
}

impl ChannelListView {
    /// Replaces the rows with `rows`, capped and sorted, and sets the flags (`Show`).
    fn show(
        &mut self,
        network_id: &str,
        mask: &str,
        rows: &[ListedChannel],
        complete: bool,
        loading: bool,
        cached: bool,
        error: &str,
    ) {
        let mut sorted: Vec<ListedChannel> = rows.iter().take(CHANNEL_LIST_MAX_ROWS).cloned().collect();
        sorted.sort_by(channel_order);
        self.network_id = network_id.to_owned();
        self.mask = mask.to_owned();
        self.rows = sorted;
        self.complete = complete;
        self.loading = loading;
        self.cached = cached;
        self.error = error.to_owned();
    }

    /// Resets the overlay for a streamed load (`BeginLoad`).
    fn begin_load(&mut self, network_id: &str, mask: &str) {
        self.network_id = network_id.to_owned();
        self.mask = mask.to_owned();
        self.rows.clear();
        self.complete = false;
        self.loading = true;
        self.cached = false;
        self.error.clear();
    }

    /// Marks the overlay failed, keeping any rows it had (`Fail`).
    fn fail(&mut self, text: &str) {
        self.loading = false;
        self.complete = false;
        self.cached = false;
        self.error = text.to_owned();
    }

    /// Appends or replaces one streamed row while a load is in flight (`appendRow`).
    fn append_row(&mut self, row: ListedChannel, replaced: bool) {
        if replaced {
            if let Some(index) = self
                .rows
                .iter()
                .position(|existing| existing.name.eq_ignore_ascii_case(&row.name))
            {
                self.rows[index] = row;
            }
            return;
        }
        if self.rows.len() >= CHANNEL_LIST_MAX_ROWS {
            return;
        }
        self.rows.push(row);
        self.rows.sort_by(channel_order);
    }

    /// Forgets the overlay's network and rows (`Clear`). It is a no-op when already empty.
    fn clear(&mut self) {
        if self.rows.is_empty()
            && self.network_id.is_empty()
            && self.error.is_empty()
            && !self.loading
            && !self.complete
        {
            return;
        }
        self.rows.clear();
        self.network_id.clear();
        self.mask.clear();
        self.error.clear();
        self.loading = false;
        self.complete = false;
        self.cached = false;
    }
}

/// The outcome of a `/list` intent. The dispatcher maps it to the command outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListOutcome {
    /// The request went out, or joined a load already in flight.
    Sent,
    /// The network cannot take a `/list`, or the write was refused.
    Refused,
    /// The network has no registered session.
    NotConnected,
}

impl Controller {
    /// Runs `/list mask` on `network_id` (`dispatchList` and `beginChannelListLoad`).
    /// It writes `LIST`, or joins the load already running, and opens the overlay. A
    /// completed request for the same mask is shown from its cache unless the mask is
    /// new or the overlay was closed.
    pub fn list_channels(&mut self, network_id: &str, mask: &str) -> ListOutcome {
        self.pump();
        let outcome = self.start_channel_list(network_id, mask);
        self.pump();
        outcome
    }

    /// Closes the overlay and keeps its cached rows (`DismissChannelList`).
    pub fn dismiss_channel_list(&mut self) {
        self.channel_view.open = false;
    }

    /// The overlay's state for the shell.
    pub fn channel_list(&self) -> &ChannelListView {
        &self.channel_view
    }

    fn start_channel_list(&mut self, network_id: &str, mask: &str) -> ListOutcome {
        if network_id.is_empty() {
            return ListOutcome::Refused;
        }
        let Some(session) = self.session(network_id) else {
            return ListOutcome::NotConnected;
        };
        if session.state() != SessionState::Registered {
            return ListOutcome::NotConnected;
        }
        let mask = mask.trim();
        let same = same_mask(self.channel_lists.phase(network_id).mask(), mask);
        let refresh = self.channel_view.open && self.channel_view.network_id == network_id && same;

        let kind = self.channel_lists.request(network_id, mask, refresh);
        let found = self.channel_lists.phase(network_id);
        match kind {
            RequestKind::Loading => {
                self.channel_view.show(
                    network_id,
                    found.mask(),
                    found.rows(),
                    false,
                    true,
                    false,
                    found.error(),
                );
                self.channel_view.open = true;
                ListOutcome::Sent
            }
            RequestKind::Cached => {
                self.channel_view
                    .show(network_id, found.mask(), found.rows(), true, false, true, "");
                self.channel_view.open = true;
                ListOutcome::Sent
            }
            RequestKind::Start => {
                if self.begin_channel_list_load(&session, network_id, mask) {
                    ListOutcome::Sent
                } else {
                    ListOutcome::Refused
                }
            }
        }
    }

    /// Writes `LIST` for a fresh load and opens the overlay on it. A refused write
    /// unwinds the request and the overlay (`beginChannelListLoad`).
    fn begin_channel_list_load(&mut self, session: &Session, network_id: &str, mask: &str) -> bool {
        self.channel_lists.request(network_id, mask, true);
        self.channel_view.begin_load(network_id, mask);
        self.channel_view.open = true;
        if !session.list(mask) {
            self.channel_lists.cancel_start(network_id);
            self.channel_view.clear();
            self.channel_view.open = false;
            return false;
        }
        true
    }

    /// The 323 end of a load (`finishChannelList`). A pending mask starts the next load.
    /// Otherwise the overlay shows the completed rows, if it is on this network.
    fn finish_channel_list(&mut self, network_id: &str) {
        let result = self.channel_lists.finish(network_id);
        if !result.completed || !self.channel_lists.phase(network_id).is_complete() {
            return;
        }
        if let Some(pending) = result.pending_mask {
            if let Some(session) = self.session(network_id) {
                if session.state() == SessionState::Registered
                    && self.begin_channel_list_load(&session, network_id, &pending)
                {
                    return;
                }
            }
        }
        if self.channel_view.network_id == network_id {
            let cache = self.channel_lists.phase(network_id);
            self.channel_view
                .show(network_id, cache.mask(), cache.rows(), true, false, false, "");
        }
    }

    /// Fails an in-flight load from a 263, a 416, or a 4xx or 5xx numeric that names
    /// LIST (`failChannelListFromNumeric`).
    fn fail_channel_list_from_numeric(&mut self, network_id: &str, message: &IrcMessage) -> bool {
        if !self.channel_lists.phase(network_id).is_loading() {
            return false;
        }
        let command = message.command.as_str();
        if command.len() != 3 {
            return false;
        }
        let try_again = command == "263";
        let too_many = command == "416";
        let error_numeric = command.starts_with(['4', '5']);
        if !try_again && !too_many && !error_numeric {
            return false;
        }
        let mentions_list = (0..message.params.len())
            .any(|index| param_text(message, index).eq_ignore_ascii_case("LIST"));
        if try_again && !mentions_list {
            return false;
        }
        if error_numeric && !too_many && !mentions_list {
            return false;
        }
        let mut text = if message.params.is_empty() {
            String::new()
        } else {
            param_text(message, message.params.len() - 1)
        };
        if text.is_empty() {
            text = if try_again {
                "Server load is too heavy. Try /list again.".to_owned()
            } else if too_many {
                "Too many channel matches. Try a narrower /list.".to_owned()
            } else {
                "Channel list failed. Try /list again.".to_owned()
            };
        }
        self.fail_channel_list(network_id, &text)
    }

    /// Fails the request and, when the overlay is on this network, the overlay too.
    fn fail_channel_list(&mut self, network_id: &str, text: &str) -> bool {
        if !self.channel_lists.fail(network_id, text, false) {
            return false;
        }
        if self.channel_view.network_id == network_id {
            self.channel_view.fail(text);
        }
        true
    }

    /// Routes 321, 322 and 323 and the LIST failure numerics. Returns whether the message
    /// was consumed (`handleChannelListMessage`).
    pub(super) fn handle_channel_list_message(&mut self, network_id: &str, message: &IrcMessage) -> bool {
        match message.command.as_str() {
            "322" => {
                if message.params.len() >= 3 {
                    let users = param_text(message, 2).parse::<usize>().unwrap_or(0);
                    let topic = if message.params.len() >= 4 {
                        param_text(message, 3)
                    } else {
                        String::new()
                    };
                    let row = ListedChannel {
                        name: param_text(message, 1),
                        users,
                        topic,
                    };
                    let replaced = self.channel_lists.row(network_id, row.clone());
                    if self.channel_view.open
                        && self.channel_view.network_id == network_id
                        && self.channel_view.loading
                    {
                        self.channel_view.append_row(row, replaced);
                    }
                }
                true
            }
            "321" => {
                self.channel_lists.activity(network_id);
                true
            }
            "323" => {
                self.finish_channel_list(network_id);
                true
            }
            _ => self.fail_channel_list_from_numeric(network_id, message),
        }
    }

    /// Drops a network's `/list` state and timer, and closes the overlay if it shows that
    /// network (the channel-list part of `DiscardSession` and `ForgetNetworkState`).
    pub(super) fn forget_channel_list(&mut self, network_id: &str) {
        self.channel_lists.forget(network_id);
        if self.channel_view.network_id == network_id {
            self.channel_view.clear();
            self.channel_view.open = false;
        }
    }

    /// Runs one idle-timeout expiry, if one is due. The stalled load fails with the
    /// timeout text. Returns whether a timeout ran.
    pub(super) fn run_channel_list_expiry(&mut self) -> bool {
        let Some(network_id) = self.channel_lists.take_expiry() else {
            return false;
        };
        if self.channel_lists.fail(&network_id, CHANNEL_LIST_TIMEOUT_TEXT, true)
            && self.channel_view.network_id == network_id
        {
            self.channel_view.fail(CHANNEL_LIST_TIMEOUT_TEXT);
        }
        true
    }
}

/// Whether two masks match after trimming, ignoring case (`SameChannelListMask`).
fn same_mask(left: &str, right: &str) -> bool {
    left.trim().to_lowercase() == right.trim().to_lowercase()
}

/// Most users first, then the channel name, case-insensitively (`rowLess`).
fn channel_order(left: &ListedChannel, right: &ListedChannel) -> std::cmp::Ordering {
    right
        .users
        .cmp(&left.users)
        .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
}

/// One message parameter as display text, or "" when absent.
fn param_text(message: &IrcMessage, index: usize) -> String {
    message.param(index).map(wire_text).unwrap_or_default()
}
