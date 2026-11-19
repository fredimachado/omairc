//! Where the history search looks: the lines every conversation holds in memory, and the
//! on-disk logs of the conversations on known networks.
//! `tui/internal/controller/search.go` holds the same two sources.
//!
//! The memory source is read on the UI thread and is cheap. The disk source is a plan the
//! shell hands to a worker: [`DiskScan`] owns everything it needs, so it can run off the
//! UI thread without touching the controller.
//!
//! A conversation's loaded lines are the newest the log holds, so the disk scan adds only
//! the lines older than the oldest one loaded. A line the transcript policy never stores is
//! never found, by either source.

use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::ops::ControlFlow;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};

use crate::irc::casemapping::IrcCaseMapping;
use crate::irc::reducer::{zero_time, TranscriptLine};
use crate::irc::secretpolicy::allows_transcript;
use crate::irc::world::MessageKind;
use crate::model::search::{
    merge_results, SearchConversation, SearchHit, SearchLine, SearchQuery, SearchResults,
    SEARCH_CAP,
};
use crate::storage::conversationlog::FileConversationLog;
use crate::storage::logscan::scan_log;

use super::controller::Controller;

/// Hits wait this long, or until a batch is this big, before they are handed on.
const BATCH_HITS: usize = 20;
const BATCH_WAIT: Duration = Duration::from_millis(30);

/// Log lines are mostly in time order, but a prepended history page or a skewed server
/// clock can put a line a little out of place. A file's scan stops only once a line is
/// older than the cutoff by more than this.
const ORDER_SLACK: chrono::Duration = chrono::Duration::seconds(60);

impl Controller {
    /// The newest hits among the lines every conversation holds in memory. Muted
    /// conversations count.
    pub fn search_loaded(&self, query: &SearchQuery) -> SearchResults {
        let mut hits = Vec::new();
        for (key, conversation) in self.reducer.conversations() {
            let channel_types = self.features(&key.network_id).channel_types();
            let place = self.search_conversation(key, &conversation.target);
            for message in &conversation.messages {
                let kind = match message.kind {
                    MessageKind::Message => "message",
                    MessageKind::Notice => "notice",
                    MessageKind::Action => "action",
                    MessageKind::Event | MessageKind::Error | MessageKind::Whois => continue,
                };
                // A loaded line was admitted when it arrived. The same gate the log writer
                // uses runs again, so the two sources can never disagree about a credential.
                if !allows_transcript(&message.body, channel_types)
                    || !allows_transcript(&message.author, channel_types)
                {
                    continue;
                }
                let line = SearchLine {
                    time: message.time,
                    author: &message.author,
                    kind,
                    body: &message.body,
                    msgid: &message.msgid.value,
                };
                hits.extend(place.hit(query, &line));
            }
        }
        merge_results(vec![hits], SEARCH_CAP)
    }

    /// What a worker needs to search the on-disk logs, or `None` when nothing is kept on
    /// disk (the demo, and tests without a transcript root) or no conversation is on a
    /// known network.
    pub fn search_disk_scan(&self) -> Option<DiskScan> {
        let root = self.transcript_root.clone()?;
        let targets: Vec<DiskTarget> = self
            .reducer
            .conversations()
            .iter()
            .filter(|(key, _)| self.knows_network(&key.network_id))
            .map(|(key, conversation)| DiskTarget {
                conversation: self.search_conversation(key, &conversation.target),
                mapping: self.features(&key.network_id).case_mapping(),
                loaded: LoadedSpan::of(&conversation.messages),
            })
            .collect();
        (!targets.is_empty()).then_some(DiskScan { root, targets })
    }

    fn search_conversation(
        &self,
        key: &crate::irc::event::ConversationKey,
        label: &str,
    ) -> SearchConversation {
        SearchConversation {
            network_id: key.network_id.clone(),
            network_name: self.network_name(&key.network_id),
            conversation: key.clone(),
            label: label.to_owned(),
        }
    }
}

/// How far back a conversation's loaded lines reach: the log lines the search skips because
/// memory already answered for them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LoadedSpan {
    pub(crate) oldest_ms: Option<i64>,
    /// The loaded lines stamped exactly at the oldest instant, as (msgid, author, body).
    /// Several lines can share a millisecond, so the instant alone cannot say which of them
    /// are loaded.
    pub(crate) at_oldest: Vec<(String, String, String)>,
}

impl LoadedSpan {
    pub fn of(messages: &[crate::irc::world::Message]) -> Self {
        let zero = zero_time();
        let stamped = || messages.iter().filter(|message| message.time != zero);
        let Some(oldest_ms) = stamped().map(|message| message.time.timestamp_millis()).min()
        else {
            return Self::default();
        };
        Self {
            oldest_ms: Some(oldest_ms),
            at_oldest: stamped()
                .filter(|message| message.time.timestamp_millis() == oldest_ms)
                .map(|message| {
                    (
                        message.msgid.value.clone(),
                        message.author.clone(),
                        message.body.clone(),
                    )
                })
                .collect(),
        }
    }

    /// Whether the conversation's memory already holds this log line.
    pub fn covers(&self, line: &TranscriptLine) -> bool {
        let Some(oldest_ms) = self.oldest_ms else {
            return false;
        };
        let ms = line.timestamp.timestamp_millis();
        if ms != oldest_ms {
            return ms > oldest_ms;
        }
        self.at_oldest.iter().any(|(msgid, author, body)| {
            if !msgid.is_empty() || !line.msgid.is_empty() {
                *msgid == line.msgid
            } else {
                *author == line.author && *body == line.body
            }
        })
    }
}

/// One conversation's log, and what memory already holds of it.
#[derive(Clone, Debug)]
pub struct DiskTarget {
    pub(crate) conversation: SearchConversation,
    pub(crate) mapping: IrcCaseMapping,
    pub(crate) loaded: LoadedSpan,
}

/// The logs a search reads off the UI thread. It owns its inputs.
#[derive(Clone, Debug)]
pub struct DiskScan {
    pub(crate) root: PathBuf,
    pub(crate) targets: Vec<DiskTarget>,
}

impl DiskScan {
    /// Reads each conversation's log newest first and hands the hits on in batches: the
    /// first at once, then every [`BATCH_HITS`] or [`BATCH_WAIT`]. It stops early in a file
    /// once the lines left there are older than everything that could still make the list,
    /// and stops altogether when `cancel` is set.
    pub fn run(
        &self,
        query: &SearchQuery,
        cancel: &AtomicBool,
        emit: &mut dyn FnMut(Vec<SearchHit>),
    ) {
        let log = FileConversationLog::new(self.root.clone());
        // The newest `SEARCH_CAP + 1` hit times so far: the extra one is what tells the list
        // there is more than it can show.
        let mut newest: BinaryHeap<Reverse<DateTime<Utc>>> = BinaryHeap::new();
        let mut batch: Vec<SearchHit> = Vec::new();
        let mut sent_any = false;
        let mut last_sent = Instant::now();
        for target in &self.targets {
            if cancel.load(Ordering::Relaxed) {
                return;
            }
            let path = log.path_for(
                &target.conversation.network_id,
                &target.conversation.conversation.normalized_target,
                target.mapping,
            );
            scan_log(&path, cancel, &mut |line| {
                if let Some(cutoff) = cutoff(&newest) {
                    if line.timestamp + ORDER_SLACK < cutoff {
                        return ControlFlow::Break(());
                    }
                }
                if target.loaded.covers(&line) {
                    return ControlFlow::Continue(());
                }
                let found = SearchLine {
                    time: line.timestamp,
                    author: &line.author,
                    kind: &line.kind,
                    body: &line.body,
                    msgid: &line.msgid,
                };
                let Some(hit) = target.conversation.hit(query, &found) else {
                    return ControlFlow::Continue(());
                };
                newest.push(Reverse(hit.time));
                if newest.len() > SEARCH_CAP + 1 {
                    newest.pop();
                }
                batch.push(hit);
                if !sent_any || batch.len() >= BATCH_HITS || last_sent.elapsed() >= BATCH_WAIT {
                    sent_any = true;
                    last_sent = Instant::now();
                    emit(std::mem::take(&mut batch));
                }
                ControlFlow::Continue(())
            });
        }
        if !batch.is_empty() && !cancel.load(Ordering::Relaxed) {
            emit(batch);
        }
    }
}

/// The time a line must beat to still count: the oldest of the newest `SEARCH_CAP + 1` hits,
/// once that many have been found.
fn cutoff(newest: &BinaryHeap<Reverse<DateTime<Utc>>>) -> Option<DateTime<Utc>> {
    (newest.len() > SEARCH_CAP)
        .then(|| newest.peek().map(|oldest| oldest.0))
        .flatten()
}
