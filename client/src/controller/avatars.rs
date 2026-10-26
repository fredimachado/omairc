//! Avatar photos, the controller's half. It decides whether an avatar is fetched and
//! keeps what came back. Go `tui/internal/avatar/avatar.go` (the store, its `Ensure`
//! and `Block`). The network and the decode run on the UI's background executor through
//! `desktop::avatarhttp`, so this module has no gpui and no socket. It hands out an
//! `AvatarJob` and takes back the outcome.
//! Three rules hold here. Nothing is fetched while the `avatars` preference is off.
//! A URL is requested only when `resolve_avatar` accepts it, and each URL is in flight
//! at most once. A failed fetch waits `FAILURE_COOLDOWN` on the clock before it is
//! requested again, so a dead host costs one request a minute rather than one per frame.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, TimeDelta, Utc};

use crate::controller::controller::Controller;
use crate::desktop::avatarhttp::{AvatarFailure, AvatarJob};
use crate::desktop::httpfetch::HttpFetch;
use crate::irc::avatarstore::{resolve_avatar, AvatarStore};

/// The size avatars are drawn at, in logical pixels. Every avatar the UI draws is
/// 34 px or less, so each quantizes to the same 32 px fetch size and one cache entry
/// serves them all. Go `AvatarFetchPixelSize`, Qt `ircAvatarFetchPixelSize`.
pub const AVATAR_LAYOUT_PIXELS: i32 = 34;

/// How long a failed avatar waits before it is requested again.
pub const FAILURE_COOLDOWN: Duration = Duration::from_secs(60);

/// The avatar state the controller owns: the cache and the in-flight slots, the seam
/// a job fetches through, and when each failed URL last failed.
pub struct AvatarFetches {
    fetch: Arc<dyn HttpFetch>,
    store: AvatarStore<Arc<[u8]>>,
    failed: HashMap<String, DateTime<Utc>>,
}

impl AvatarFetches {
    pub fn new(fetch: Arc<dyn HttpFetch>) -> Self {
        Self {
            fetch,
            store: AvatarStore::new(),
            failed: HashMap::new(),
        }
    }

    /// Whether `key` failed recently enough that it must wait. A clock that ran
    /// backwards also waits, so a failure is never retried early.
    fn cooling_down(&self, key: &str, now: DateTime<Utc>) -> bool {
        let cooldown = TimeDelta::from_std(FAILURE_COOLDOWN).unwrap_or(TimeDelta::zero());
        self.failed
            .get(key)
            .is_some_and(|failed_at| now.signed_duration_since(*failed_at) < cooldown)
    }

    /// Remembers a failure and forgets the ones whose cooldown has ended.
    fn record_failure(&mut self, key: &str, now: DateTime<Utc>) {
        let cooldown = TimeDelta::from_std(FAILURE_COOLDOWN).unwrap_or(TimeDelta::zero());
        self.failed
            .retain(|_, failed_at| now.signed_duration_since(*failed_at) < cooldown);
        self.failed.insert(key.to_owned(), now);
    }
}

impl Controller {
    /// Replaces the seam avatars are fetched through. The controller starts with the
    /// ureq one; tests put a fake in.
    #[cfg(test)]
    pub fn set_avatar_fetch(&mut self, fetch: Arc<dyn HttpFetch>) {
        self.avatars.fetch = fetch;
    }

    /// The job that fetches `reference` now, with its slot marked in flight, or `None`
    /// when nothing should be fetched: the `avatars` preference is off, the reference
    /// is empty or unsafe, the URL is cached or already in flight, or it failed within
    /// the cooldown. The shell runs the job on the background executor and hands the
    /// outcome to `finish_avatar`.
    pub fn request_avatar(&mut self, reference: &str) -> Option<AvatarJob> {
        if !self.prefs.avatars {
            return None;
        }
        let (key, _) = resolve_avatar(reference, AVATAR_LAYOUT_PIXELS)?;
        let now = self.clock.now();
        if self.avatars.cooling_down(&key, now) {
            return None;
        }
        let pending = self
            .avatars
            .store
            .begin_fetch(reference, AVATAR_LAYOUT_PIXELS)?;
        Some(AvatarJob {
            key: pending.key,
            url: pending.url,
            fetch: Arc::clone(&self.avatars.fetch),
        })
    }

    /// Takes back a finished job. A PNG is cached under its key; a failure frees the
    /// slot and starts the cooldown.
    pub fn finish_avatar(&mut self, key: &str, outcome: Result<Arc<[u8]>, AvatarFailure>) {
        match outcome {
            Ok(png) => {
                self.avatars.store.complete_fetch(key, png);
                self.avatars.failed.remove(key);
            }
            Err(failure) => {
                log::warn!("{failure}");
                self.avatars.store.fail_fetch(key);
                let now = self.clock.now();
                self.avatars.record_failure(key, now);
            }
        }
    }

    /// The cached PNG for `reference`, when its photo may be drawn. It is a lookup:
    /// it never starts a fetch, and it answers nothing while the preference is off.
    pub fn avatar_bytes(&self, reference: &str) -> Option<Arc<[u8]>> {
        if !self.prefs.avatars {
            return None;
        }
        self.avatars
            .store
            .lookup(reference, AVATAR_LAYOUT_PIXELS)
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::httpfetch::UreqFetch;

    #[test]
    fn the_layout_size_quantizes_to_the_32_pixel_fetch() {
        let (key_34, url_34) = resolve_avatar("https://cdn.example.com/{size}/a.png", AVATAR_LAYOUT_PIXELS)
            .expect("resolved");
        let (key_32, url_32) = resolve_avatar("https://cdn.example.com/{size}/a.png", 32).expect("resolved");
        assert_eq!(key_34, key_32);
        assert_eq!(url_34, url_32);
        assert!(url_34.as_str().contains("/32/"));
    }

    #[test]
    fn the_cooldown_is_sixty_seconds_on_the_clock() {
        let mut fetches = AvatarFetches::new(Arc::new(UreqFetch));
        let start = DateTime::parse_from_rfc3339("2026-09-12T12:00:00Z").unwrap().to_utc();
        fetches.record_failure("k", start);
        assert!(fetches.cooling_down("k", start));
        assert!(fetches.cooling_down("k", start + TimeDelta::seconds(59)));
        assert!(!fetches.cooling_down("k", start + TimeDelta::seconds(60)));
        assert!(!fetches.cooling_down("other", start));
    }

    #[test]
    fn a_backwards_clock_keeps_the_cooldown() {
        let mut fetches = AvatarFetches::new(Arc::new(UreqFetch));
        let start = DateTime::parse_from_rfc3339("2026-09-12T12:00:00Z").unwrap().to_utc();
        fetches.record_failure("k", start);
        assert!(fetches.cooling_down("k", start - TimeDelta::seconds(5)));
    }
}
