//! Each conversation's unsent composer text, kept across a restart in `drafts.json`
//! (`storage::drafts`). The shell hands a draft over when it switches away, when the
//! window loses focus, and on quit, never per keystroke, and asks for it back when a
//! conversation is shown. The key is the network id plus the conversation's name under
//! the network's case mapping. `irc::secretpolicy::allows_draft_persistence` decides what
//! may be written; a draft it rejects removes the stored one instead. The Status pane
//! has no draft here. Go: `tui/internal/controller/drafts.go`.

use crate::irc::casemapping::IrcCaseMapping;
use crate::irc::secretpolicy::allows_draft_persistence;
use crate::storage::drafts::{DraftFile, DraftMap, StoredDraft, MAX_DRAFT_BYTES};
use crate::storage::settings::Status;

use super::controller::Controller;

/// The drafts this process loaded or stored, by network. Only the key a write touches is
/// updated, so the cache is this process's own view of the file.
#[derive(Debug)]
pub struct DraftStore {
    file: DraftFile,
    cache: DraftMap,
}

impl DraftStore {
    /// The store over `file`, holding every draft an earlier run kept that is not expired.
    pub fn new(file: DraftFile, now_ms: i64) -> Self {
        let cache = file.load(now_ms);
        Self { file, cache }
    }

    /// The stored draft for `target` on `network_id`, if any.
    pub fn get(&self, network_id: &str, target: &str, mapping: IrcCaseMapping) -> Option<&str> {
        self.cache
            .get(network_id)?
            .get(&mapping.normalize(target))
            .map(|draft| draft.text.as_str())
    }

    /// Keeps `text` as the draft for `target` on `network_id`. Text the secret gate does not
    /// clear, blank text, and text that is too long remove the draft instead.
    pub fn remember(
        &mut self,
        network_id: &str,
        target: &str,
        text: &str,
        mapping: IrcCaseMapping,
        channel_types: &str,
        now_ms: i64,
    ) {
        if network_id.is_empty() || target.is_empty() {
            return;
        }
        let kept = allows_draft_persistence(text, channel_types)
            .then_some(text)
            .filter(|t| !t.trim().is_empty() && t.len() <= MAX_DRAFT_BYTES);
        let normalized = mapping.normalize(target);
        // Only a change against this process's own view reaches the file, so a draft another
        // client wrote after this cache was loaded is not deleted by an untouched composer.
        let have = self
            .cache
            .get(network_id)
            .and_then(|targets| targets.get(&normalized))
            .map(|draft| draft.text.as_str());
        if have == kept {
            return;
        }
        if self.file.store(network_id, &normalized, kept, now_ms) != Status::Written
            || self.file.is_ephemeral()
        {
            return;
        }
        match kept {
            Some(text) => {
                self.cache.entry(network_id.to_owned()).or_default().insert(
                    normalized,
                    StoredDraft {
                        text: text.to_owned(),
                        ms: now_ms,
                    },
                );
            }
            None => {
                if let Some(targets) = self.cache.get_mut(network_id) {
                    targets.remove(&normalized);
                    if targets.is_empty() {
                        self.cache.remove(network_id);
                    }
                }
            }
        }
    }
}

impl Controller {
    /// Keeps a conversation's unsent text across a restart (`RememberDraft`). Blank text
    /// removes it, and so does anything the secret gate rejects.
    pub fn remember_draft(&mut self, network_id: &str, target: &str, text: &str) {
        let features = self.reducer.server_features(network_id);
        let mapping = features.case_mapping();
        let channel_types = features.channel_types().to_owned();
        let now_ms = self.clock.now().timestamp_millis();
        self.drafts
            .remember(network_id, target, text, mapping, &channel_types, now_ms);
    }

    /// The stored draft for a conversation (`DraftFor`).
    pub fn draft_for(&self, network_id: &str, target: &str) -> Option<String> {
        let mapping = self.reducer.server_features(network_id).case_mapping();
        self.drafts
            .get(network_id, target, mapping)
            .map(str::to_owned)
    }

    /// Puts the drafts on `file`, loaded as of now (tests).
    #[cfg(test)]
    pub(crate) fn use_draft_file(&mut self, file: DraftFile) {
        self.drafts = DraftStore::new(file, self.clock.now().timestamp_millis());
    }
}
