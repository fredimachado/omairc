//! The pure half of the avatar store. It mirrors `tui/internal/avatar/avatar.go`
//! and the bookkeeping in `tui/internal/avatar/avatar.go` (Go). The fetch, the
//! decode, the DNS pin and the image itself are I/O and stay in `desktop/` and
//! the UI. This module holds only the state: a bounded insertion-ordered cache,
//! the set of fetches in flight, the cache key, and the byte and decode budgets.
//! It is generic over the stored value, so the GPUI image type never enters `irc/`.

use std::collections::{HashMap, HashSet, VecDeque};

use url::Url;

use super::avatarurl::{avatar_fetch_pixel_size, avatar_url_is_safe, resolved_avatar_url, sha256_hex};

/// Qt `kMaximumCacheEntries`. Go `maximumCacheEntries`.
pub const MAXIMUM_CACHE_ENTRIES: usize = 64;
/// Qt `kMaximumAvatarBytes`. A body longer than this is refused.
pub const MAXIMUM_AVATAR_BYTES: u64 = 512 * 1024;
/// Qt `kMaximumRedirects`. Each hop is judged by `avatar_url_is_safe`.
pub const MAXIMUM_REDIRECTS: u32 = 3;
/// Qt `kMaximumDecodedEdge`. The decoded budget is independent of the render size,
/// since a small payload can declare a huge image.
pub const MAXIMUM_DECODED_EDGE: u32 = 1024;
/// Qt `kMaximumDecodedPixels`.
pub const MAXIMUM_DECODED_PIXELS: u64 = 1024 * 1024;
/// Qt `kMaximumDecodedAllocationMb`, for the decoder's own allocation limit.
pub const MAXIMUM_DECODED_ALLOCATION_MB: u32 = 16;

/// The cache key for a resolved avatar URL: the SHA-256 of its text. Go `cacheKey`,
/// Qt `IrcAvatarStore::keyForUrl`.
pub fn avatar_cache_key(url: &Url) -> String {
    sha256_hex(url.as_str().as_bytes())
}

/// Whether a `Content-Type` value is an image. Qt `contentTypeIsImage`.
pub fn content_type_is_image(content_type: &str) -> bool {
    content_type
        .trim()
        .to_ascii_lowercase()
        .starts_with("image/")
}

/// Whether a decoded image's declared size is inside the budget. Qt
/// `decodedSizeWithinBudget`.
pub fn decoded_size_within_budget(width: u32, height: u32) -> bool {
    if width == 0 || height == 0 {
        return false;
    }
    if width > MAXIMUM_DECODED_EDGE || height > MAXIMUM_DECODED_EDGE {
        return false;
    }
    u64::from(width) * u64::from(height) <= MAXIMUM_DECODED_PIXELS
}

/// The URL a redirect's `Location` names, resolved against the request URL. Qt
/// `redirectTarget`.
#[cfg(test)]
pub fn redirect_target(base: &Url, location: &str) -> Option<Url> {
    base.join(location).ok()
}

/// The cache key and the resolved URL for a raw `/avatar` URL at a layout size.
/// `None` when the URL is empty, unsafe, or the layout size disables the fetch.
/// The key depends on the fetch size, so 32 and 64 pixel copies cache apart.
pub fn resolve_avatar(raw_url: &str, layout_pixels: i32) -> Option<(String, Url)> {
    let fetch_size = avatar_fetch_pixel_size(layout_pixels);
    if fetch_size == 0 {
        return None;
    }
    let resolved = resolved_avatar_url(raw_url.trim(), fetch_size)?;
    if !avatar_url_is_safe(&resolved) {
        return None;
    }
    Some((avatar_cache_key(&resolved), resolved))
}

/// A bounded, insertion-ordered cache. Re-inserting a key moves it to the newest
/// slot. Past the cap the oldest entries go, and their keys are returned so the
/// caller can drop whatever it holds for them.
pub struct BoundedCache<V> {
    capacity: usize,
    order: VecDeque<String>,
    values: HashMap<String, V>,
}

impl<V> BoundedCache<V> {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            order: VecDeque::new(),
            values: HashMap::new(),
        }
    }

    /// Stores `value` under `key` as the newest entry. Returns the evicted keys,
    /// oldest first.
    pub fn insert(&mut self, key: String, value: V) -> Vec<String> {
        self.order.retain(|existing| existing != &key);
        self.order.push_back(key.clone());
        self.values.insert(key, value);
        let mut evicted = Vec::new();
        while self.order.len() > self.capacity {
            if let Some(oldest) = self.order.pop_front() {
                self.values.remove(&oldest);
                evicted.push(oldest);
            }
        }
        evicted
    }

    pub fn get(&self, key: &str) -> Option<&V> {
        self.values.get(key)
    }

    pub fn contains(&self, key: &str) -> bool {
        self.values.contains_key(key)
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.order.len()
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }
}

/// A fetch the caller must now run. Its `key` names the entry to complete or fail.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingFetch {
    pub key: String,
    pub url: Url,
    /// The URL as the caller gave it, so a ready callback can name the message.
    pub raw_url: String,
}

/// The avatar cache and its in-flight fetches. Go `Store`, Qt `IrcAvatarStore`
/// (minus the network). Only `begin_fetch` says a fetch should start, so a URL is
/// never fetched twice at once.
pub struct AvatarStore<V> {
    images: BoundedCache<V>,
    pending: HashSet<String>,
    closed: bool,
}

impl<V> Default for AvatarStore<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V> AvatarStore<V> {
    pub fn new() -> Self {
        Self {
            images: BoundedCache::new(MAXIMUM_CACHE_ENTRIES),
            pending: HashSet::new(),
            closed: false,
        }
    }

    /// The cached image for a raw URL at a layout size. Never starts a fetch. Go
    /// `Block`'s lookup.
    pub fn lookup(&self, raw_url: &str, layout_pixels: i32) -> Option<&V> {
        let (key, _) = resolve_avatar(raw_url, layout_pixels)?;
        self.images.get(&key)
    }

    /// Asks for a fetch. Returns the fetch to run, with its slot marked in flight,
    /// when the URL is safe, not cached, not already in flight, and the store is
    /// open. Go `Ensure`.
    pub fn begin_fetch(&mut self, raw_url: &str, layout_pixels: i32) -> Option<PendingFetch> {
        if self.closed {
            return None;
        }
        let (key, url) = resolve_avatar(raw_url, layout_pixels)?;
        if self.images.contains(&key) || self.pending.contains(&key) {
            return None;
        }
        self.pending.insert(key.clone());
        Some(PendingFetch {
            key,
            url,
            raw_url: raw_url.to_string(),
        })
    }

    /// Stores a fetched image and frees its slot. Returns the keys evicted to make
    /// room, or `None` when the store closed while the fetch ran, so the late
    /// image is dropped. Go `commit`.
    pub fn complete_fetch(&mut self, key: &str, value: V) -> Option<Vec<String>> {
        self.pending.remove(key);
        if self.closed {
            return None;
        }
        Some(self.images.insert(key.to_string(), value))
    }

    /// Frees the slot of a failed fetch, so a later request may try again. Go `forget`.
    pub fn fail_fetch(&mut self, key: &str) {
        self.pending.remove(key);
    }

    /// Drops every in-flight slot and refuses later fetches. Go `Close`.
    #[cfg(test)]
    pub fn close(&mut self) {
        self.closed = true;
        self.pending.clear();
    }

    #[cfg(test)]
    pub fn is_closed(&self) -> bool {
        self.closed
    }

    #[cfg(test)]
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    #[cfg(test)]
    pub fn cached_count(&self) -> usize {
        self.images.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AVATAR: &str = "https://cdn.example.com/a.png";

    #[test]
    fn the_cache_evicts_the_oldest_past_the_cap() {
        let mut cache = BoundedCache::new(3);
        assert!(cache.insert("a".into(), 1).is_empty());
        assert!(cache.insert("b".into(), 2).is_empty());
        assert!(cache.insert("c".into(), 3).is_empty());
        assert_eq!(cache.insert("d".into(), 4), ["a"]);
        assert!(!cache.contains("a"));
        assert_eq!(cache.get("d"), Some(&4));
        assert_eq!(cache.len(), 3);
        assert!(!cache.is_empty());
    }

    #[test]
    fn reinserting_moves_a_key_to_the_newest_slot() {
        let mut cache = BoundedCache::new(3);
        cache.insert("a".into(), 1);
        cache.insert("b".into(), 2);
        cache.insert("c".into(), 3);
        assert!(cache.insert("a".into(), 10).is_empty(), "a refresh evicts nothing");
        assert_eq!(cache.get("a"), Some(&10));
        assert_eq!(cache.insert("d".into(), 4), ["b"], "b is now the oldest");
    }

    #[test]
    fn the_real_cap_is_sixty_four_entries() {
        let mut store: AvatarStore<u32> = AvatarStore::new();
        for index in 0..MAXIMUM_CACHE_ENTRIES {
            let url = format!("https://cdn.example.com/{index}.png");
            let fetch = store.begin_fetch(&url, 32).expect("fetch");
            assert!(store.complete_fetch(&fetch.key, index as u32).is_some());
        }
        assert_eq!(store.cached_count(), MAXIMUM_CACHE_ENTRIES);
        let late = store
            .begin_fetch("https://cdn.example.com/extra.png", 32)
            .expect("fetch");
        let evicted = store.complete_fetch(&late.key, 999).expect("open");
        assert_eq!(evicted.len(), 1, "one entry past the cap is evicted");
        assert_eq!(store.cached_count(), MAXIMUM_CACHE_ENTRIES);
    }

    #[test]
    fn a_safe_uncached_url_starts_one_fetch() {
        let mut store: AvatarStore<u32> = AvatarStore::new();
        let fetch = store.begin_fetch(AVATAR, 32).expect("starts a fetch");
        assert_eq!(fetch.raw_url, AVATAR);
        assert_eq!(fetch.url.as_str(), AVATAR);
        assert_eq!(store.pending_count(), 1);
        assert!(
            store.begin_fetch(AVATAR, 32).is_none(),
            "a second request while in flight starts nothing"
        );
    }

    #[test]
    fn a_completed_fetch_is_cached_and_not_fetched_again() {
        let mut store: AvatarStore<&str> = AvatarStore::new();
        let fetch = store.begin_fetch(AVATAR, 32).expect("fetch");
        assert_eq!(store.lookup(AVATAR, 32), None, "in flight is not cached");
        store.complete_fetch(&fetch.key, "image");
        assert_eq!(store.lookup(AVATAR, 32), Some(&"image"));
        assert!(store.begin_fetch(AVATAR, 32).is_none());
        assert_eq!(store.pending_count(), 0);
    }

    #[test]
    fn lookup_never_starts_a_fetch() {
        let store: AvatarStore<u32> = AvatarStore::new();
        assert_eq!(store.lookup(AVATAR, 32), None);
        assert_eq!(store.pending_count(), 0);
    }

    #[test]
    fn unsafe_empty_and_unsized_urls_start_nothing() {
        let mut store: AvatarStore<u32> = AvatarStore::new();
        assert!(store.begin_fetch("", 32).is_none());
        assert!(store.begin_fetch("http://cdn.example.com/a.png", 32).is_none());
        assert!(store.begin_fetch("https://localhost/a.png", 32).is_none());
        assert!(store.begin_fetch(AVATAR, 0).is_none(), "a zero layout size disables the fetch");
        assert_eq!(store.pending_count(), 0);
    }

    #[test]
    fn a_failed_fetch_frees_its_slot_for_a_retry() {
        let mut store: AvatarStore<u32> = AvatarStore::new();
        let fetch = store.begin_fetch(AVATAR, 32).expect("fetch");
        store.fail_fetch(&fetch.key);
        assert_eq!(store.pending_count(), 0);
        assert!(store.begin_fetch(AVATAR, 32).is_some(), "a retry is allowed");
    }

    #[test]
    fn the_size_placeholder_keys_each_fetch_size_apart() {
        let template = "https://cdn.example.com/{size}/a.png";
        let small = resolve_avatar(template, 32).expect("32").0;
        let large = resolve_avatar(template, 64).expect("64").0;
        assert_ne!(small, large);
        assert_eq!(small.len(), 64);
        assert_eq!(small, resolve_avatar(template, 34).expect("34").0, "34 quantizes to 32");
    }

    #[test]
    fn closing_drops_pending_fetches_and_late_images() {
        let mut store: AvatarStore<u32> = AvatarStore::new();
        let fetch = store.begin_fetch(AVATAR, 32).expect("fetch");
        store.close();
        assert!(store.is_closed());
        assert_eq!(store.pending_count(), 0);
        assert!(store.complete_fetch(&fetch.key, 1).is_none(), "a late image is dropped");
        assert_eq!(store.cached_count(), 0);
        assert!(store.begin_fetch("https://cdn.example.com/b.png", 32).is_none());
    }

    #[test]
    fn the_budgets_match_qt() {
        assert!(content_type_is_image("image/png"));
        assert!(content_type_is_image("  IMAGE/JPEG; charset=binary "));
        assert!(!content_type_is_image("text/html"));
        assert!(!content_type_is_image(""));

        assert!(decoded_size_within_budget(1024, 1024));
        assert!(!decoded_size_within_budget(1025, 1));
        assert!(!decoded_size_within_budget(1, 1025));
        assert!(!decoded_size_within_budget(0, 10));
        assert!(!decoded_size_within_budget(1024, 1025), "over the pixel budget");
        assert_eq!(MAXIMUM_AVATAR_BYTES, 512 * 1024);
        assert_eq!(MAXIMUM_REDIRECTS, 3);
    }

    #[test]
    fn redirect_targets_resolve_against_the_request() {
        let base = Url::parse("https://cdn.example.com/dir/a.png").expect("parsed");
        assert_eq!(
            redirect_target(&base, "../b.png").expect("resolved").as_str(),
            "https://cdn.example.com/b.png"
        );
        assert_eq!(
            redirect_target(&base, "https://other.example/c.png")
                .expect("absolute")
                .as_str(),
            "https://other.example/c.png"
        );
    }

    #[test]
    fn the_cache_key_is_the_sha256_of_the_resolved_url() {
        let url = Url::parse(AVATAR).expect("parsed");
        let key = avatar_cache_key(&url);
        assert_eq!(key, sha256_hex(AVATAR.as_bytes()));
        assert_eq!(key.len(), 64);
    }
}
