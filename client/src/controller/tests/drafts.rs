//! Persisted composer drafts through the controller (`controller/drafts.rs`): what reaches
//! `drafts.json`, what the secret gate keeps off it, and what a later run reads back.
//! The Go equivalent is `tui/internal/controller/drafts_test.go`.

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use super::fixture::{Fixture, OMARCHY};
use crate::storage::drafts::{DraftFile, DRAFT_TTL_MS};
use crate::storage::paths::DRAFTS_FILE;
use crate::storage::Backing;
use crate::util::testdir::TempDir;

const EMPTY: &str = r#"{"drafts":{},"version":1}"#;

/// The seeded demo with its draft store on a disk file in `dir`.
fn on_disk(dir: &TempDir) -> (Fixture, PathBuf) {
    let path = dir.path().join(DRAFTS_FILE);
    let mut fixture = Fixture::seeded();
    fixture
        .controller
        .use_draft_file(DraftFile::new(Backing::Disk(path.clone())));
    (fixture, path)
}

fn on_disk_text(path: &PathBuf) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

#[test]
fn a_draft_is_written_and_read_back_by_a_new_controller() {
    let dir = TempDir::new("ctrl-drafts-roundtrip");
    let (mut first, path) = on_disk(&dir);
    first.controller.remember_draft(OMARCHY, "#a", "half typed");
    assert!(on_disk_text(&path).contains(r##""#a":{"ms":"##));

    let (second, _) = on_disk(&dir);
    assert_eq!(
        second.controller.draft_for(OMARCHY, "#a").as_deref(),
        Some("half typed")
    );
    assert_eq!(second.controller.draft_for(OMARCHY, "#b"), None);
}

#[test]
fn the_key_follows_the_networks_case_mapping() {
    let dir = TempDir::new("ctrl-drafts-case");
    let (mut fixture, path) = on_disk(&dir);
    fixture.controller.remember_draft(OMARCHY, "#Mixed", "text");
    assert!(on_disk_text(&path).contains(r##""#mixed""##), "{}", on_disk_text(&path));
    assert_eq!(
        fixture.controller.draft_for(OMARCHY, "#MIXED").as_deref(),
        Some("text")
    );
}

#[test]
fn an_emptied_draft_is_removed() {
    let dir = TempDir::new("ctrl-drafts-empty");
    let (mut fixture, path) = on_disk(&dir);
    fixture.controller.remember_draft(OMARCHY, "#a", "half typed");
    fixture.controller.remember_draft(OMARCHY, "#a", "  ");
    assert_eq!(on_disk_text(&path), EMPTY);
    assert_eq!(fixture.controller.draft_for(OMARCHY, "#a"), None);
}

#[test]
fn a_draft_not_updated_for_seven_days_is_gone_after_relaunch() {
    let dir = TempDir::new("ctrl-drafts-ttl");
    let (mut first, _) = on_disk(&dir);
    first.controller.remember_draft(OMARCHY, "#a", "half typed");

    let mut kept = Fixture::seeded();
    kept.clock
        .advance(Duration::from_millis(DRAFT_TTL_MS as u64 - 1_000));
    kept.controller
        .use_draft_file(DraftFile::new(Backing::Disk(dir.path().join(DRAFTS_FILE))));
    assert_eq!(kept.controller.draft_for(OMARCHY, "#a").as_deref(), Some("half typed"));

    let mut expired = Fixture::seeded();
    expired
        .clock
        .advance(Duration::from_millis(DRAFT_TTL_MS as u64 + 1_000));
    expired
        .controller
        .use_draft_file(DraftFile::new(Backing::Disk(dir.path().join(DRAFTS_FILE))));
    assert_eq!(expired.controller.draft_for(OMARCHY, "#a"), None);
}

#[test]
fn a_secret_is_never_written_and_clears_an_earlier_draft() {
    let secrets = [
        "/msg NickServ IDENTIFY pw",
        "/ns identify pw",
        "/quote PASS hunter2",
        "/oper a b",
        "identify pw",
        "PASS hunter2",
    ];
    for secret in secrets {
        let dir = TempDir::new("ctrl-drafts-secret");
        let (mut fixture, path) = on_disk(&dir);
        fixture.controller.remember_draft(OMARCHY, "#a", "half typed");
        fixture.controller.remember_draft(OMARCHY, "#a", secret);
        let text = on_disk_text(&path);
        assert!(!text.contains("pw") && !text.contains("hunter2"), "{secret:?}: {text}");
        assert_eq!(text, EMPTY, "{secret:?} must also clear the earlier draft");
        assert_eq!(fixture.controller.draft_for(OMARCHY, "#a"), None);
    }
}

#[test]
fn a_secret_typed_in_a_nickserv_query_is_never_written() {
    for target in ["NickServ", "nickserv"] {
        let dir = TempDir::new("ctrl-drafts-nickserv");
        let (mut fixture, path) = on_disk(&dir);
        fixture.controller.remember_draft(OMARCHY, target, "IDENTIFY pw");
        fixture.controller.remember_draft(OMARCHY, target, "identify account pw");
        assert!(!on_disk_text(&path).contains("pw"), "{target}: {}", on_disk_text(&path));
    }
}

#[test]
fn a_me_line_persists_and_other_slash_lines_do_not() {
    let dir = TempDir::new("ctrl-drafts-slash");
    let (mut fixture, path) = on_disk(&dir);
    fixture.controller.remember_draft(OMARCHY, "#a", "/me waves");
    assert!(on_disk_text(&path).contains("/me waves"));
    for line in ["/join #a", "/msg NickServ IDENTIFY pw", "/quote PASS x"] {
        fixture.controller.remember_draft(OMARCHY, "#a", line);
        assert_eq!(on_disk_text(&path), EMPTY, "{line:?}");
    }
}

#[test]
fn an_ephemeral_controller_keeps_nothing() {
    let mut fixture = Fixture::seeded();
    fixture.controller.remember_draft(OMARCHY, "#a", "half typed");
    assert_eq!(fixture.controller.draft_for(OMARCHY, "#a"), None);
}

#[test]
fn another_clients_draft_survives_an_empty_composer_here() {
    let dir = TempDir::new("ctrl-drafts-theirs");
    let (mut fixture, path) = on_disk(&dir);
    // Another client writes #a after this process loaded its cache.
    let other = DraftFile::new(Backing::Disk(path.clone()));
    other.store(OMARCHY, "#a", Some("theirs"), 1_789_214_400_000);
    fixture.controller.remember_draft(OMARCHY, "#a", "");
    assert!(on_disk_text(&path).contains("theirs"), "{}", on_disk_text(&path));

    // A write of our own for another key does not adopt their draft as ours either.
    fixture.controller.remember_draft(OMARCHY, "#b", "ours");
    fixture.controller.remember_draft(OMARCHY, "#a", "");
    assert!(on_disk_text(&path).contains("theirs"), "{}", on_disk_text(&path));
}

#[test]
fn only_a_change_against_this_processs_own_view_reaches_the_file() {
    let dir = TempDir::new("ctrl-drafts-own-view");
    let (mut fixture, path) = on_disk(&dir);
    fixture.controller.remember_draft(OMARCHY, "#a", "mine");
    assert!(on_disk_text(&path).contains("mine"));
    fixture.controller.remember_draft(OMARCHY, "#a", "");
    assert_eq!(on_disk_text(&path), EMPTY, "a draft we stored is ours to delete");
}
