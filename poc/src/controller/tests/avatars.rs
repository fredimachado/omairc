//! The avatar fetch, frame by frame. The demo's METADATA reaches the fetch decision,
//! the job runs through the `HttpFetch` seam, and the outcome reaches the photo the
//! controller answers. Go `tui/internal/avatar/avatar_test.go` and Qt
//! `tests/tst_ircavatarstore.cpp` are the spec; the URLs are the demo's own.

use std::io::Cursor;
use std::sync::Arc;
use std::time::Duration;

use image::{DynamicImage, ImageFormat};

use crate::desktop::httpfetch::FakeFetch;
use crate::irc::pref::{PrefName, PrefValue};

use super::fixture::{Fixture, OFTC, OMARCHY};

/// The operator's own avatar, which the demo advertises on both networks.
const SELF_AVATAR: &str = "https://omairc.app/omairc128.png";

fn png() -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    DynamicImage::new_rgb8(8, 8)
        .write_to(&mut out, ImageFormat::Png)
        .expect("encodes");
    out.into_inner()
}

/// A fake peer that serves a small PNG at the demo's avatar URL.
fn serving_the_avatar() -> Arc<FakeFetch> {
    let fake = Arc::new(FakeFetch::new());
    fake.serve_typed(SELF_AVATAR, 200, Some("image/png"), &png());
    fake
}

/// Starts the fetch for `reference`, runs its job, and hands the outcome back as the
/// shell does. Returns whether a photo is now cached for it.
fn fetch_now(fixture: &mut Fixture, reference: &str) -> bool {
    let Some(job) = fixture.controller.request_avatar(reference) else {
        return false;
    };
    let (key, outcome) = job.run();
    let kept = outcome.is_ok();
    fixture.controller.finish_avatar(&key, outcome);
    kept
}

#[test]
fn the_demo_metadata_avatar_becomes_a_fetch_of_its_https_url() {
    let mut fixture = Fixture::seeded();
    let fake = serving_the_avatar();
    fixture.controller.set_avatar_fetch(fake.clone());

    // The seed sends the avatar as METADATA, and the reducer keeps it per nick.
    let fred = fixture.controller.nick_facts(OMARCHY);
    let reference = fred
        .get("fred")
        .and_then(|facts| facts.avatar.clone())
        .expect("fred has an avatar");
    assert_eq!(reference, SELF_AVATAR);
    assert!(
        fixture.controller.nick_facts(OFTC).get("oak").is_some_and(|facts| facts.avatar.as_deref() == Some(SELF_AVATAR)),
        "oak on oftc advertises the same avatar"
    );

    // Seeding starts nothing: a fetch is an answer to a request, never to the seed.
    assert!(fake.requests().is_empty());
    assert!(fixture.controller.avatar_bytes(&reference).is_none());

    let job = fixture
        .controller
        .request_avatar(&reference)
        .expect("the demo avatar is fetched");
    assert_eq!(job.url.as_str(), SELF_AVATAR);
    let (key, outcome) = job.run();
    fixture.controller.finish_avatar(&key, outcome);

    let requests = fake.requests();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].public_addresses_only, "the fetch is pinned to a public address");
    let photo = fixture
        .controller
        .avatar_bytes(&reference)
        .expect("the PNG is cached");
    assert_eq!(&photo[..8], b"\x89PNG\r\n\x1a\n");
}

#[test]
fn a_cached_avatar_is_not_fetched_again() {
    let mut fixture = Fixture::seeded();
    let fake = serving_the_avatar();
    fixture.controller.set_avatar_fetch(fake.clone());
    assert!(fetch_now(&mut fixture, SELF_AVATAR));
    assert!(fixture.controller.request_avatar(SELF_AVATAR).is_none());
    assert_eq!(fake.requests().len(), 1);
}

#[test]
fn a_request_while_in_flight_starts_nothing() {
    let mut fixture = Fixture::seeded();
    fixture.controller.set_avatar_fetch(serving_the_avatar());
    let first = fixture.controller.request_avatar(SELF_AVATAR);
    assert!(first.is_some());
    assert!(
        fixture.controller.request_avatar(SELF_AVATAR).is_none(),
        "one job per URL is in flight"
    );
}

#[test]
fn an_unsafe_reference_never_reaches_the_fetch() {
    let mut fixture = Fixture::seeded();
    let fake = serving_the_avatar();
    fixture.controller.set_avatar_fetch(fake.clone());
    let unsafe_references = [
        "http://omairc.app/omairc128.png",
        "https://localhost/omairc128.png",
        "https://printer.local/omairc128.png",
        "https://10.0.0.4/omairc128.png",
        "https://[::1]/omairc128.png",
        "https://user@omairc.app/omairc128.png",
        "https://omairc.app:8443/omairc128.png",
        "file:///etc/passwd",
        "qrc:/demo/mira-avatar.png",
    ];
    for reference in unsafe_references {
        assert!(
            fixture.controller.request_avatar(reference).is_none(),
            "{reference} starts no fetch"
        );
    }
    assert!(fake.requests().is_empty());
}

#[test]
fn the_avatars_preference_off_starts_no_fetch() {
    let mut fixture = Fixture::seeded();
    let fake = serving_the_avatar();
    fixture.controller.set_avatar_fetch(fake.clone());
    assert!(fixture
        .controller
        .set_preference(PrefName::Avatars, PrefValue::Toggle(false)));

    assert!(fixture.controller.request_avatar(SELF_AVATAR).is_none());
    assert!(fixture.controller.avatar_bytes(SELF_AVATAR).is_none());
    assert!(fake.requests().is_empty(), "no request reached the seam");

    // Turning it back on lets the next draw fetch.
    assert!(fixture
        .controller
        .set_preference(PrefName::Avatars, PrefValue::Toggle(true)));
    assert!(fetch_now(&mut fixture, SELF_AVATAR));
}

#[test]
fn a_failed_fetch_keeps_no_photo_and_waits_out_the_cooldown() {
    let mut fixture = Fixture::seeded();
    let fake = Arc::new(FakeFetch::new());
    fake.fail(SELF_AVATAR, "connection reset");
    fixture.controller.set_avatar_fetch(fake.clone());

    assert!(!fetch_now(&mut fixture, SELF_AVATAR), "the failure keeps no photo");
    assert!(fixture.controller.avatar_bytes(SELF_AVATAR).is_none(), "initials stay");

    // Asking again in the same frame, or a moment later, starts nothing.
    assert!(fixture.controller.request_avatar(SELF_AVATAR).is_none());
    fixture.clock.advance(Duration::from_secs(59));
    assert!(fixture.controller.request_avatar(SELF_AVATAR).is_none());
    assert_eq!(fake.requests().len(), 1, "one request in the cooldown window");

    // Once the cooldown has passed, the next request goes out again.
    fixture.clock.advance(Duration::from_secs(1));
    assert!(fixture.controller.request_avatar(SELF_AVATAR).is_some());
}

#[test]
fn a_reply_that_is_not_an_image_keeps_the_initials() {
    let mut fixture = Fixture::seeded();
    let fake = Arc::new(FakeFetch::new());
    fake.serve_typed(SELF_AVATAR, 200, Some("text/html"), b"<html>not a picture</html>");
    fixture.controller.set_avatar_fetch(fake);
    assert!(!fetch_now(&mut fixture, SELF_AVATAR));
    assert!(fixture.controller.avatar_bytes(SELF_AVATAR).is_none());
}
