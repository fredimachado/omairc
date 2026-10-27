//! pref: argument shapes, the printed lines, and the set that writes through.

use super::*;

use crate::irc::colorpalette::ColorPaletteId;
use crate::irc::membershipnoise::MembershipNoise;

#[test]
fn pref_parses_its_argument_into_the_request() {
    let joins = pref("/pref joins hidden");
    assert_eq!(joins.kind, PrefKind::Set);
    assert_eq!(joins.name, PrefName::Joins);
    assert_eq!(joins.noise, MembershipNoise::Hidden);

    let avatars = pref("/pref avatars off");
    assert_eq!(avatars.kind, PrefKind::Set);
    assert_eq!(avatars.name, PrefName::Avatars);
    assert!(!avatars.enabled);

    let palette = pref("/pref palette vivid");
    assert_eq!(palette.kind, PrefKind::Set);
    assert_eq!(palette.name, PrefName::Palette);
    assert_eq!(palette.palette, ColorPaletteId::Vivid);

    assert_eq!(pref("/pref").kind, PrefKind::QueryAll);
}

#[test]
fn pref_argument_shapes_are_query_set_and_usage() {
    assert_eq!(pref("/pref   ").kind, PrefKind::QueryAll);
    assert_eq!(pref("/pref directs").kind, PrefKind::QueryOne);
    assert_eq!(pref("/pref AVATARS").name, PrefName::Avatars);
    assert_eq!(pref("/pref joins").kind, PrefKind::QueryOne);
    assert_eq!(pref("/pref palette").kind, PrefKind::QueryOne);
    assert_eq!(pref("/pref unread on").kind, PrefKind::Set);
    assert_eq!(pref("/pref avatars   off").kind, PrefKind::Set);
    assert_eq!(pref("/pref joins Every").noise, MembershipNoise::Every);
    assert_eq!(pref("/pref palette Classic").palette, ColorPaletteId::Classic);
    for line in [
        "/pref directs maybe",
        "/pref joins sometimes",
        "/pref joins on",
        "/pref palette on",
        "/pref palette loud",
        "/pref notify on",
        "/pref notify",
        "/pref avatars on now",
    ] {
        assert_eq!(pref(line).kind, PrefKind::Usage, "{line}");
    }
}

#[test]
fn bare_pref_prints_six_lines() {
    let mut prefs = Preferences::default();
    let reply = pref_reply(pref("/pref"), &mut prefs);
    assert_eq!(
        reply.text,
        "Reopen direct messages on startup: on\n\
         Show peer avatars: on\n\
         Open conversations at unread: on\n\
         Show mIRC colors: on\n\
         Join, part, quit, and nick lines: folded\n\
         Color palette: classic"
    );
    assert!(!reply.changed);
}

#[test]
fn one_name_prints_one_line_and_avatars_adds_the_note() {
    let mut prefs = Preferences::default();
    assert_eq!(
        pref_reply(pref("/pref directs"), &mut prefs).text,
        "Reopen direct messages on startup: on"
    );
    assert_eq!(
        pref_reply(pref("/pref avatars"), &mut prefs).text,
        "Show peer avatars: on\n\
         Turn off to keep avatar hosts from seeing your IP on busy channels."
    );
    assert_eq!(
        pref_reply(pref("/pref joins"), &mut prefs).text,
        "Join, part, quit, and nick lines: folded"
    );
    assert_eq!(
        pref_reply(pref("/pref palette"), &mut prefs).text,
        "Color palette: classic"
    );
}

#[test]
fn a_set_writes_through_and_echoes_one_state_line() {
    let mut prefs = Preferences::default();
    let reply = pref_reply(pref("/pref avatars off"), &mut prefs);
    assert_eq!(reply.text, "Show peer avatars: off");
    assert!(reply.changed);
    assert!(!prefs.avatars);

    let reply = pref_reply(pref("/pref joins every"), &mut prefs);
    assert_eq!(reply.text, "Join, part, quit, and nick lines: every");
    assert_eq!(prefs.joins, MembershipNoise::Every);

    let reply = pref_reply(pref("/pref palette vivid"), &mut prefs);
    assert_eq!(reply.text, "Color palette: vivid");
    assert_eq!(prefs.palette, ColorPaletteId::Vivid);
}

#[test]
fn usage_echoes_the_verb_usage_and_changes_nothing() {
    let mut prefs = Preferences::default();
    let reply = pref_reply(pref("/pref joins on"), &mut prefs);
    assert_eq!(
        reply.text,
        "/pref [directs|avatars|unread|colors] [on|off] | joins [folded|every|hidden] | palette [classic|vivid]"
    );
    assert!(!reply.changed);
    assert_eq!(prefs, Preferences::default());
}

#[test]
fn a_pref_value_kind_matches_its_catalog_row() {
    assert_eq!(
        crate::irc::pref::pref_find("joins").map(|spec| spec.value_kind),
        Some(PrefValueKind::Noise)
    );
    assert_eq!(
        crate::irc::pref::pref_find("palette").map(|spec| spec.value_kind),
        Some(PrefValueKind::Palette)
    );
    assert_eq!(
        crate::irc::pref::pref_find("avatars").map(|spec| spec.value_kind),
        Some(PrefValueKind::Toggle)
    );
}
