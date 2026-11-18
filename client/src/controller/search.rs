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

use chrono::{DateTime, TimeZone, Utc};

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
    oldest_ms: Option<i64>,
    /// The loaded lines stamped exactly at the oldest instant, as (msgid, author, body).
    /// Several lines can share a millisecond, so the instant alone cannot say which of them
    /// are loaded.
    at_oldest: Vec<(String, String, String)>,
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
    conversation: SearchConversation,
    mapping: IrcCaseMapping,
    loaded: LoadedSpan,
}

/// The logs a search reads off the UI thread. It owns its inputs.
#[derive(Clone, Debug)]
pub struct DiskScan {
    root: PathBuf,
    targets: Vec<DiskTarget>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::tests::fixture::{Fixture, OMARCHY};
    use crate::irc::reducer::ConversationLog;
    use crate::util::testdir::TempDir;

    fn at(second: i64) -> DateTime<Utc> {
        Utc.timestamp_millis_opt(1_700_000_000_000 + second * 1000).unwrap()
    }

    fn stored(second: i64, author: &str, body: &str, msgid: &str) -> TranscriptLine {
        TranscriptLine {
            timestamp: at(second),
            author: author.to_owned(),
            kind: "message".to_owned(),
            body: body.to_owned(),
            msgid: msgid.to_owned(),
            only_visible_to_you: false,
        }
    }

    fn target(network: &str, name: &str, loaded: LoadedSpan) -> DiskTarget {
        DiskTarget {
            conversation: SearchConversation {
                network_id: network.to_owned(),
                network_name: network.to_owned(),
                conversation: crate::irc::event::ConversationKey::new(network, name),
                label: name.to_owned(),
            },
            mapping: IrcCaseMapping::default(),
            loaded,
        }
    }

    fn write(dir: &TempDir, network: &str, name: &str, lines: &[TranscriptLine]) {
        let log = FileConversationLog::new(dir.path().to_path_buf());
        for line in lines {
            assert!(log.append(network, name, IrcCaseMapping::default(), line));
        }
    }

    fn run(scan: &DiskScan, query: &str) -> (Vec<SearchHit>, usize) {
        let query = SearchQuery::parse(query).unwrap();
        let never = AtomicBool::new(false);
        let mut hits = Vec::new();
        let mut batches = 0;
        scan.run(&query, &never, &mut |batch| {
            batches += 1;
            hits.extend(batch);
        });
        (hits, batches)
    }

    fn texts(hits: &[SearchHit]) -> Vec<&str> {
        hits.iter().map(|hit| hit.text.as_str()).collect()
    }

    #[test]
    fn the_disk_scan_finds_lines_in_every_conversation_and_names_them() {
        let dir = TempDir::new("search-disk");
        write(&dir, "omarchy", "#ricing", &[stored(1, "anna", "tea at five", "m1")]);
        write(&dir, "omarchy", "dax", &[stored(2, "dax", "more TEA?", "")]);
        let scan = DiskScan {
            root: dir.path().to_path_buf(),
            targets: vec![
                target("omarchy", "#ricing", LoadedSpan::default()),
                target("omarchy", "dax", LoadedSpan::default()),
            ],
        };
        let (hits, _) = run(&scan, "tea");
        let merged = merge_results(vec![hits], SEARCH_CAP);
        assert_eq!(texts(&merged.hits), ["more TEA?", "tea at five"]);
        assert_eq!(merged.hits[0].conversation.normalized_target, "dax");
        assert_eq!(merged.hits[1].target, crate::model::search::JumpTarget::Msgid("m1".into()));
    }

    #[test]
    fn a_line_memory_holds_is_not_found_twice_and_older_ones_are() {
        let dir = TempDir::new("search-covered");
        let lines = [
            stored(1, "anna", "old tea", ""),
            stored(5, "anna", "tea five", "m5"),
            stored(6, "anna", "tea six", "m6"),
        ];
        write(&dir, "omarchy", "#ricing", &lines);
        // Memory holds the newest two lines.
        let loaded = LoadedSpan {
            oldest_ms: Some(at(5).timestamp_millis()),
            at_oldest: vec![("m5".to_owned(), "anna".to_owned(), "tea five".to_owned())],
        };
        let scan = DiskScan {
            root: dir.path().to_path_buf(),
            targets: vec![target("omarchy", "#ricing", loaded)],
        };
        let (hits, _) = run(&scan, "tea");
        assert_eq!(texts(&hits), ["old tea"]);
    }

    #[test]
    fn a_line_sharing_the_oldest_loaded_millisecond_is_told_apart_by_content() {
        let dir = TempDir::new("search-boundary");
        write(
            &dir,
            "omarchy",
            "#ricing",
            &[
                stored(5, "anna", "tea not loaded", "m4"),
                stored(5, "anna", "tea loaded", "m5"),
            ],
        );
        let loaded = LoadedSpan {
            oldest_ms: Some(at(5).timestamp_millis()),
            at_oldest: vec![("m5".to_owned(), "anna".to_owned(), "tea loaded".to_owned())],
        };
        let scan = DiskScan {
            root: dir.path().to_path_buf(),
            targets: vec![target("omarchy", "#ricing", loaded)],
        };
        assert_eq!(texts(&run(&scan, "tea").0), ["tea not loaded"]);
    }

    #[test]
    fn a_credential_in_a_log_is_never_found() {
        let dir = TempDir::new("search-secret");
        let path = FileConversationLog::new(dir.path().to_path_buf()).path_for(
            "omarchy",
            "#ricing",
            IrcCaseMapping::default(),
        );
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        // The log writer would have dropped these. A hand-written log may hold them.
        let secret = |second: i64, author: &str, body: &str| {
            serde_json::json!({
                "timestamp": at(second).format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
                "author": author, "kind": "message", "body": body,
            })
            .to_string()
        };
        std::fs::write(
            &path,
            [
                secret(1, "fred", "hunter2 is not it"),
                secret(2, "fred", "IDENTIFY hunter2"),
                secret(3, "NickServ", "PRIVMSG NickServ :IDENTIFY hunter2"),
                secret(4, "anna", "a hunter2 joke"),
            ]
            .join("\n")
                + "\n",
        )
        .unwrap();
        let scan = DiskScan {
            root: dir.path().to_path_buf(),
            targets: vec![target("omarchy", "#ricing", LoadedSpan::default())],
        };
        let (hits, _) = run(&scan, "hunter2");
        let found = texts(&hits);
        assert!(
            !found.iter().any(|text| text.contains("IDENTIFY")),
            "a credential line was found: {found:?}"
        );
        assert!(found.contains(&"a hunter2 joke") && found.contains(&"hunter2 is not it"));
    }

    #[test]
    fn two_hundred_and_fifty_matches_on_disk_give_the_newest_and_more() {
        let dir = TempDir::new("search-cap");
        let lines: Vec<_> = (0..250)
            .map(|n| stored(n, "anna", &format!("tea {n}"), ""))
            .collect();
        write(&dir, "omarchy", "#ricing", &lines);
        let scan = DiskScan {
            root: dir.path().to_path_buf(),
            targets: vec![target("omarchy", "#ricing", LoadedSpan::default())],
        };
        let (hits, _) = run(&scan, "tea");
        let merged = merge_results(vec![hits], SEARCH_CAP);
        assert_eq!(merged.hits.len(), SEARCH_CAP);
        assert!(merged.more);
        assert_eq!(merged.hits[0].text, "tea 249");
        assert_eq!(merged.hits[SEARCH_CAP - 1].text, "tea 50");
    }

    #[test]
    fn a_file_stops_once_older_lines_cannot_make_the_list() {
        let dir = TempDir::new("search-early");
        // 300 recent matches in one conversation, and a huge old backlog in another that
        // the first one's hits already outrank.
        let recent: Vec<_> = (0..300)
            .map(|n| stored(100_000 + n, "anna", &format!("tea recent {n}"), ""))
            .collect();
        write(&dir, "omarchy", "#recent", &recent);
        let old: Vec<_> = (0..2000)
            .map(|n| stored(n, "anna", &format!("tea old {n}"), ""))
            .collect();
        write(&dir, "omarchy", "#old", &old);
        let scan = DiskScan {
            root: dir.path().to_path_buf(),
            targets: vec![
                target("omarchy", "#recent", LoadedSpan::default()),
                target("omarchy", "#old", LoadedSpan::default()),
            ],
        };
        let (hits, _) = run(&scan, "tea");
        assert!(
            hits.iter().all(|hit| hit.text.starts_with("tea recent")),
            "the old backlog was read past the point it could matter"
        );
        let merged = merge_results(vec![hits], SEARCH_CAP);
        assert!(merged.more);
        assert_eq!(merged.hits[0].text, "tea recent 299");
    }

    #[test]
    fn the_first_hit_goes_out_at_once_and_the_rest_in_batches() {
        let dir = TempDir::new("search-batches");
        let lines: Vec<_> = (0..100)
            .map(|n| stored(n, "anna", &format!("tea {n}"), ""))
            .collect();
        write(&dir, "omarchy", "#ricing", &lines);
        let scan = DiskScan {
            root: dir.path().to_path_buf(),
            targets: vec![target("omarchy", "#ricing", LoadedSpan::default())],
        };
        let (hits, batches) = run(&scan, "tea");
        assert_eq!(hits.len(), 100);
        assert!(batches >= 2, "the first hit does not wait for a full batch");
        assert!(batches < 100);
    }

    #[test]
    fn a_cancelled_scan_sends_nothing_more() {
        let dir = TempDir::new("search-cancel");
        let lines: Vec<_> = (0..100)
            .map(|n| stored(n, "anna", &format!("tea {n}"), ""))
            .collect();
        write(&dir, "omarchy", "#ricing", &lines);
        let scan = DiskScan {
            root: dir.path().to_path_buf(),
            targets: vec![target("omarchy", "#ricing", LoadedSpan::default())],
        };
        let query = SearchQuery::parse("tea").unwrap();
        let cancel = AtomicBool::new(false);
        let mut sent = 0;
        scan.run(&query, &cancel, &mut |batch| {
            sent += batch.len();
            cancel.store(true, Ordering::Relaxed);
        });
        assert_eq!(sent, 1, "only the first hit went out before the cancel");
    }

    #[test]
    fn loaded_lines_are_searched_newest_first_with_their_conversation() {
        let mut fixture = Fixture::seeded();
        fixture.deliver(OMARCHY, ":kai!u@h PRIVMSG #help :zebra crossing ahead");
        fixture.deliver(OMARCHY, ":dax!u@h PRIVMSG fred :a ZEBRA in private");
        let query = SearchQuery::parse("zebra").unwrap();
        let found = fixture.controller.search_loaded(&query);
        assert_eq!(found.hits.len(), 2);
        assert!(!found.more);
        let targets: Vec<_> = found
            .hits
            .iter()
            .map(|hit| hit.conversation.normalized_target.as_str())
            .collect();
        assert!(targets.contains(&"#help") && targets.contains(&"dax"));
        assert!(found.hits[0].time >= found.hits[1].time);
        assert!(found.hits.iter().all(|hit| hit.network_id == OMARCHY));
    }

    #[test]
    fn a_loaded_line_matches_by_its_plain_text_and_its_author() {
        let mut fixture = Fixture::seeded();
        fixture.deliver(OMARCHY, ":kai!u@h PRIVMSG #help :\u{2}bold\u{2} claim");
        let by_text = fixture
            .controller
            .search_loaded(&SearchQuery::parse("bold claim").unwrap());
        assert_eq!(by_text.hits.len(), 1);
        assert_eq!(by_text.hits[0].text, "bold claim");
        let by_author = fixture
            .controller
            .search_loaded(&SearchQuery::parse("KAI").unwrap());
        assert!(by_author.hits.iter().any(|hit| hit.author_match && hit.author == "kai"));
    }

    #[test]
    fn the_demo_keeps_nothing_on_disk_so_there_is_nothing_to_scan() {
        let fixture = Fixture::seeded();
        assert!(fixture.controller.search_disk_scan().is_none());
    }

    #[test]
    fn a_controller_with_a_transcript_root_plans_a_scan_of_each_conversation() {
        let dir = TempDir::new("search-plan");
        let mut fixture = Fixture::seeded();
        fixture.controller.set_transcript_root(dir.path());
        let scan = fixture.controller.search_disk_scan().expect("a plan");
        assert_eq!(scan.root, dir.path());
        assert!(scan.targets.len() > 1);
        assert!(scan.targets.iter().all(|target| target.conversation.network_id == OMARCHY
            || !target.conversation.network_id.is_empty()));
    }

    /// `cargo test --release search_scan_large -- --ignored --nocapture`: 200k lines, one
    /// big log and fifty small ones, and the times the issue asks for.
    #[test]
    #[ignore = "a timing run, not a gate"]
    fn search_scan_large() {
        use std::io::Write;
        let dir = TempDir::new("search-large");
        let log = FileConversationLog::new(dir.path().to_path_buf());
        let mut targets = Vec::new();
        let mut write_log = |name: &str, lines: usize, base: i64| {
            let path = log.path_for("omarchy", name, IrcCaseMapping::default());
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            let mut out = std::io::BufWriter::new(std::fs::File::create(&path).unwrap());
            for n in 0..lines {
                let marker = if n % 4000 == 0 { " needle" } else { "" };
                writeln!(
                    out,
                    "{}",
                    serde_json::json!({
                        "timestamp": at(base + n as i64).format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
                        "author": format!("nick{}", n % 17),
                        "kind": "message",
                        "body": format!("line {n} of {name}: the quick brown fox jumps over the lazy dog{marker}"),
                        "msgid": format!("{name}-{n}"),
                    })
                )
                .unwrap();
            }
            targets.push(target("omarchy", name, LoadedSpan::default()));
        };
        write_log("#big", 150_000, 0);
        for room in 0..50 {
            write_log(&format!("#small{room}"), 1_000, 1_000_000);
        }
        let bytes: u64 = std::fs::read_dir(dir.path().join("omarchy"))
            .unwrap()
            .map(|entry| entry.unwrap().metadata().unwrap().len())
            .sum();
        let scan = DiskScan {
            root: dir.path().to_path_buf(),
            targets,
        };

        // A query most lines match: the first hit goes out at once.
        let query = SearchQuery::parse("quick brown").unwrap();
        let never = AtomicBool::new(false);
        let started = Instant::now();
        let mut first = None;
        scan.run(&query, &never, &mut |_| {
            first.get_or_insert_with(|| started.elapsed());
        });
        eprintln!("first hit: {:?} (full run with an early exit: {:?})", first.unwrap(), started.elapsed());

        // A rare word: every line is read.
        let query = SearchQuery::parse("needle").unwrap();
        let started = Instant::now();
        let mut hits = 0;
        scan.run(&query, &never, &mut |batch| hits += batch.len());
        eprintln!(
            "full scan: {:?} for {} lines, {:.1} MB, {hits} hits",
            started.elapsed(),
            200_000,
            bytes as f64 / 1e6
        );

        // A cancel while the big file is being read.
        let cancel = std::sync::Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let stopper = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            let asked = Instant::now();
            flag.store(true, Ordering::Relaxed);
            asked
        });
        let query = SearchQuery::parse("no such text anywhere").unwrap();
        scan.run(&query, &cancel, &mut |_| {});
        let returned = Instant::now();
        let asked = stopper.join().unwrap();
        eprintln!("cancel latency: {:?}", returned.saturating_duration_since(asked));
    }
}
