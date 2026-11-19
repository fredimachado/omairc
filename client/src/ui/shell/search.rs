//! The history search (`Ctrl+Shift+F`): what the shell does with the sheet's field and its
//! hits. The sheet is `ui/sheets/search.rs`; the matching, merging and jump rules are
//! `model/search.rs`; the two sources are `controller/search.rs`.
//!
//! Typing restarts the search: the old one is cancelled, a 150 ms pause (the debounce) lets
//! the user finish a word, then the loaded transcripts answer at once and the on-disk logs
//! stream in from a worker. Every message back carries the generation it was started for, and
//! one for an older generation is dropped, so a stale batch never paints.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use gpui::{AppContext, Context, Entity, Task, Window};

use crate::model::search::{
    jump_outcome, JumpOutcome, SearchHit, SearchQuery, SearchResults, OLDER_THAN_LOADED_NOTE,
    SEARCH_CAP,
};
use crate::model::ConversationRef;
use crate::ui::sheets::filter_sheet::FilterSheet;
use crate::ui::sheets::search::SearchSource;
use crate::ui::shell::modal::ActiveModal;
use crate::ui::shell::Shell;

/// How long typing must pause before a search starts.
pub const SEARCH_DEBOUNCE: Duration = Duration::from_millis(150);

/// One open search sheet's running state. It lives in the open modal, so closing the sheet
/// drops it, and dropping it cancels the scan.
pub struct SearchRun {
    /// The trimmed text the current generation searches.
    text: String,
    generation: u64,
    cancel: Arc<AtomicBool>,
    results: SearchResults,
    scanning: bool,
    debounce: Option<Task<()>>,
    /// Hands the worker's batches to the shell.
    delivery: Option<Task<()>>,
    /// The log scan itself, off the UI thread.
    worker: Option<Task<()>>,
}

impl SearchRun {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            generation: 0,
            cancel: Arc::new(AtomicBool::new(false)),
            results: SearchResults::default(),
            scanning: false,
            debounce: None,
            delivery: None,
            worker: None,
        }
    }

    /// Starts over for `text`: whatever ran for the old text is cancelled and forgotten.
    fn restart(&mut self, text: String) {
        self.cancel.store(true, Ordering::Relaxed);
        self.cancel = Arc::new(AtomicBool::new(false));
        self.generation += 1;
        self.text = text;
        self.results = SearchResults::default();
        self.scanning = false;
        self.debounce = None;
        self.delivery = None;
        self.worker = None;
    }
}

impl Drop for SearchRun {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Shell {
    /// Ctrl+Shift+F: opens the search sheet, or closes it when it is already open.
    pub fn toggle_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.chord_live(crate::model::chords::Chord::Search, window, cx) {
            return;
        }
        if matches!(self.modal, Some(ActiveModal::Search { .. })) {
            self.close_modal(window, cx);
        } else {
            self.present_search(window, cx);
        }
    }

    fn search_parts(&mut self) -> Option<(Entity<FilterSheet<SearchSource>>, &mut SearchRun)> {
        match self.modal.as_mut() {
            Some(ActiveModal::Search { sheet, run, .. }) => Some((sheet.clone(), run)),
            _ => None,
        }
    }

    /// The field's text changed, or the sheet just opened. A change that leaves the trimmed
    /// text as it was (a caret move) searches nothing again.
    pub(super) fn search_query_changed(&mut self, cx: &mut Context<Self>) {
        let Some((sheet, run)) = self.search_parts() else {
            return;
        };
        let typed = sheet.read(cx).filter.read(cx).text().trim().to_owned();
        if typed == run.text {
            return;
        }
        run.restart(typed.clone());
        let generation = run.generation;
        if SearchQuery::parse(&typed).is_none() {
            self.push_search_results(cx);
            return;
        }
        let debounce = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SEARCH_DEBOUNCE).await;
            this.update(cx, |shell, cx| shell.run_search(generation, cx)).ok();
        });
        if let Some((_, run)) = self.search_parts() {
            run.debounce = Some(debounce);
        }
    }

    /// The debounce passed: the loaded transcripts answer now, and the logs start streaming.
    fn run_search(&mut self, generation: u64, cx: &mut Context<Self>) {
        let Some((_, run)) = self.search_parts() else {
            return;
        };
        if run.generation != generation {
            return;
        }
        let Some(query) = SearchQuery::parse(&run.text) else {
            return;
        };
        let cancel = run.cancel.clone();
        let results = self.controller.search_loaded(&query);
        let plan = self.controller.search_disk_scan();
        let (delivery, worker) = match plan {
            Some(plan) => {
                let (sender, receiver) = async_channel::unbounded::<Vec<SearchHit>>();
                let worker = cx.background_spawn(async move {
                    plan.run(&query, &cancel, &mut |batch| {
                        let _ = sender.send_blocking(batch);
                    });
                });
                let delivery = cx.spawn(async move |this, cx| {
                    while let Ok(batch) = receiver.recv().await {
                        if this
                            .update(cx, |shell, cx| shell.absorb_search_batch(generation, batch, cx))
                            .is_err()
                        {
                            return;
                        }
                    }
                    this.update(cx, |shell, cx| shell.finish_search(generation, cx)).ok();
                });
                (Some(delivery), Some(worker))
            }
            None => (None, None),
        };
        if let Some((_, run)) = self.search_parts() {
            run.scanning = delivery.is_some();
            run.results = results;
            run.delivery = delivery;
            run.worker = worker;
        }
        self.push_search_results(cx);
    }

    fn absorb_search_batch(&mut self, generation: u64, batch: Vec<SearchHit>, cx: &mut Context<Self>) {
        let Some((_, run)) = self.search_parts() else {
            return;
        };
        if run.generation != generation {
            return;
        }
        run.results.absorb(batch, SEARCH_CAP);
        self.push_search_results(cx);
    }

    fn finish_search(&mut self, generation: u64, cx: &mut Context<Self>) {
        let Some((_, run)) = self.search_parts() else {
            return;
        };
        if run.generation != generation {
            return;
        }
        run.scanning = false;
        self.push_search_results(cx);
    }

    /// Shows the run's results in the sheet. The selection stays on the same hit as the list
    /// grows around it, and stays on the first row while it is there.
    fn push_search_results(&mut self, cx: &mut Context<Self>) {
        let Some((sheet, run)) = self.search_parts() else {
            return;
        };
        let source = SearchSource {
            results: run.results.clone(),
            query_blank: SearchQuery::parse(&run.text).is_none(),
            scanning: run.scanning,
            today: Some(chrono::Local::now().date_naive()),
        };
        sheet.update(cx, |sheet, cx| {
            let held = (sheet.selected > 0)
                .then(|| sheet.source.results.hits.get(sheet.selected))
                .flatten();
            sheet.selected = held
                .and_then(|held| {
                    source.results.hits.iter().position(|hit| {
                        hit.conversation == held.conversation
                            && hit.target == held.target
                            && hit.time == held.time
                    })
                })
                .unwrap_or(0);
            sheet.sync_source(source, cx);
        });
    }

    /// Enter or a click on a hit: closes the sheet, opens the hit's conversation, and lands
    /// on its line. A line older than the loaded transcript opens the conversation as it
    /// opens without a search and says so (no history is fetched for it).
    pub fn activate_search(&mut self, hit: SearchHit, window: &mut Window, cx: &mut Context<Self>) {
        self.close_modal(window, cx);
        self.search_note = None;
        self.reveal(ConversationRef::from_key(&hit.conversation), cx);
        match jump_outcome(&hit.target, self.transcript.read(cx).rows()) {
            JumpOutcome::Landed(_) => {
                let place = hit.target.saved_place();
                // The line is loaded, so the restore lands at once and never waits.
                self.transcript
                    .update(cx, |t, _| t.restore_scroll_place(Some(&place), false, false));
                let follow_end = self.transcript.read(cx).reports_follow_end();
                self.report_transcript_viewport_from_shell(follow_end, cx);
            }
            JumpOutcome::OlderThanLoaded => self.search_note = Some(OLDER_THAN_LOADED_NOTE),
        }
        cx.notify();
    }
}
