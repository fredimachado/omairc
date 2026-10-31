//! `control-omairc-desktop` click-member lists must match demo `ordered_members`.

use std::fs;
use std::path::PathBuf;

use super::fixture::{Fixture, OMARCHY};

const MARKER: &str = "# verify-desktop-member-list:";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repo root")
        .to_path_buf()
}

fn driver_script() -> String {
    let path = repo_root().join(".cursor/skills/verify-omairc-desktop/control-omairc-desktop");
    fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
}

fn parse_driver_lists(script: &str) -> Vec<(String, String, Vec<String>)> {
    let mut out = Vec::new();
    for line in script.lines() {
        let Some(rest) = line.strip_prefix(MARKER) else {
            continue;
        };
        let mut parts = rest.split_whitespace();
        let network = parts.next().expect("network id").to_owned();
        let channel = parts.next().expect("channel name").to_owned();
        let nicks: Vec<String> = parts.map(str::to_owned).collect();
        assert!(
            !nicks.is_empty(),
            "verify-desktop-member-list needs at least one nick: {line}"
        );
        out.push((network, channel, nicks));
    }
    assert!(
        !out.is_empty(),
        "control-omairc-desktop must declare verify-desktop-member-list markers"
    );
    out
}

#[test]
fn verify_desktop_click_member_lists() {
    let lists = parse_driver_lists(&driver_script());
    let mut fixture = Fixture::seeded();
    for (network, channel, expected) in lists {
        fixture
            .controller
            .select_conversation(&network, &channel);
        let got: Vec<String> = fixture
            .controller
            .members()
            .into_iter()
            .map(|member| member.nick)
            .collect();
        assert_eq!(got, expected, "{network} {channel}");
    }
}

#[test]
fn verify_desktop_click_member_lists_cover_omarchy_channels() {
    let lists = parse_driver_lists(&driver_script());
    assert!(
        lists
            .iter()
            .any(|(network, channel, _)| network == OMARCHY && channel == "#desktop"),
        "driver must list omarchy #desktop for click-member"
    );
    assert!(
        lists
            .iter()
            .any(|(network, channel, _)| network == OMARCHY && channel == "#omarchy"),
        "driver must list omarchy #omarchy for click-member"
    );
}
