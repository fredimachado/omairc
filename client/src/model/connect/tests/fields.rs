//! The draft's fields: labels and help, the checks that disable Apply, normalising, and dirtiness.

use super::*;

#[test]
fn seed_mirrors_each_demo_network_as_qt_seeds_it() {
    let (_, state) = seeded();
    assert_eq!(stored_ids(&state), ["omarchy", "oftc"]);
    assert_eq!(state.selected(), "omarchy");
    let omarchy = &state.saved()[0];
    assert_eq!(omarchy.name, "");
    assert_eq!(omarchy.host, "irc.example");
    assert_eq!(omarchy.port, "6697");
    assert!(omarchy.tls);
    assert_eq!(omarchy.nick, "fred");
    assert_eq!(omarchy.username, "fred");
    assert_eq!(omarchy.realname, "fred");
    assert_eq!(omarchy.autojoin, "#omarchy #desktop #ricing #help");
    assert!(!omarchy.connect_on_startup);
    assert_eq!(omarchy.icon_color, Some(1));
    assert_eq!(state.saved()[1].icon_color, Some(2));
    assert_eq!(state.saved()[1].autojoin, "#omarchy #lab #build");
}

#[test]
fn a_seeded_draft_is_clean_and_applicable() {
    let (_, state) = seeded();
    assert!(!state.dirty());
    assert_eq!(state.problem(), None);
    assert_eq!(state.draft(), &state.saved()[0]);
}

#[test]
fn rail_labels_match_the_sidebar_names_of_the_demo() {
    let (_, state) = seeded();
    let labels: Vec<String> = state.rows().into_iter().map(|row| row.label).collect();
    // Both demo networks answer for the host, so the clash rule adds each nick: the
    // same names the sidebar draws.
    assert_eq!(labels, ["irc.example · fred", "irc.example · oak"]);
}

#[test]
fn field_labels_help_and_badges_match_the_qt_sheet() {
    assert_eq!(ConnectField::Name.label(), "Name");
    assert_eq!(ConnectField::Host.label(), "Host");
    assert_eq!(ConnectField::Port.label(), "Port");
    assert_eq!(ConnectField::Nick.label(), "Nick");
    assert_eq!(ConnectField::Username.label(), "Username");
    assert_eq!(ConnectField::RealName.label(), "Real name");
    assert_eq!(ConnectField::Autojoin.label(), "Autojoin");
    assert_eq!(ConnectField::ServerPassword.label(), "Server password");
    assert_eq!(ConnectField::NickServPassword.label(), "NickServ password");
    assert_eq!(
        ConnectField::ServerPassword.help(),
        Some("Used as the SASL secret when no NickServ password is set; otherwise sent as PASS while connecting.")
    );
    assert_eq!(
        ConnectField::NickServPassword.help(),
        Some("Preferred SASL secret. Sent as NickServ IDENTIFY when SASL did not succeed.")
    );
    assert_eq!(ConnectField::ServerPassword.badge(), Some("PASS"));
    assert_eq!(ConnectField::NickServPassword.badge(), None);
    assert!(ConnectField::ServerPassword.secret());
    assert!(ConnectField::NickServPassword.secret());
    assert!(!ConnectField::Host.secret());
    assert_eq!(ConnectField::Host.help(), None);
}

#[test]
fn preference_rows_reuse_the_pref_labels_with_qt_help() {
    let rows = preference_rows();
    let names: Vec<PrefName> = rows.iter().map(|row| row.name).collect();
    assert_eq!(
        names,
        [
            PrefName::Directs,
            PrefName::Avatars,
            PrefName::Unread,
            PrefName::Colors,
            PrefName::Palette,
        ]
    );
    assert_eq!(rows[0].label, "Reopen direct messages on startup");
    assert_eq!(rows[1].label, "Show peer avatars");
    assert_eq!(rows[2].label, "Open conversations at unread");
    assert_eq!(rows[3].label, "Show mIRC colors");
    assert_eq!(rows[4].label, "Color palette");
    assert_eq!(
        rows[0].help,
        "Reopens direct messages you had open last session after each connect. Only tabs you opened or sent in are remembered."
    );
    assert_eq!(
        rows[2].help,
        "Scrolls to the New messages marker when you switch conversations. Already-focused buffers and Status are left at the end."
    );
    assert_eq!(
        rows[3].help,
        "Paints the foreground and background colors other clients send in messages and topics. Off, text takes your theme's colors; bold, italic, underline, and strikethrough still show. Applies to lines already on screen."
    );
    assert!(
        rows[4].help.to_ascii_lowercase().contains("classic")
            && rows[4].help.to_ascii_lowercase().contains("vivid"),
        "palette help should name both tokens"
    );
}

#[test]
fn tabs_label_glyph_and_wrap() {
    assert_eq!(ConnectTab::Connection.label(), "Connection");
    assert_eq!(ConnectTab::Preferences.label(), "Preferences");
    assert_eq!(ConnectTab::Connection.glyph(), Some("#"));
    assert_eq!(ConnectTab::Preferences.glyph(), None);
    assert_eq!(ConnectTab::Connection.step(1), ConnectTab::Preferences);
    assert_eq!(ConnectTab::Preferences.step(1), ConnectTab::Connection);
    assert_eq!(ConnectTab::Connection.step(-1), ConnectTab::Preferences);
}

#[test]
fn remove_label_arms_with_the_name() {
    assert_eq!(remove_label(false, "irc.example · fred"), "Remove");
    assert_eq!(
        remove_label(true, "irc.example · fred"),
        "Remove irc.example · fred?"
    );
}

#[test]
fn problem_text_matches_qt() {
    assert_eq!(Problem::MissingHost.text(), "Host is required");
    assert_eq!(Problem::InvalidPort.text(), "Port is required");
    assert_eq!(Problem::MissingNick.text(), "Nick is required");
    assert_eq!(
        Problem::UnsendableIdentity.text(),
        "Nick, username, or real name cannot be sent"
    );
    assert_eq!(
        Problem::UnsendableChannel.text(),
        "An autojoin channel cannot be sent"
    );
}

#[test]
fn an_empty_nick_disables_apply_with_qt_wording() {
    let (_, mut state) = seeded();
    state.set_text(ConnectField::Nick, "");
    assert_eq!(state.problem(), Some(Problem::MissingNick));
    assert_eq!(state.problem().map(Problem::text), Some("Nick is required"));
    state.set_text(ConnectField::Nick, "   ");
    assert_eq!(state.problem(), Some(Problem::MissingNick));
}

#[test]
fn host_then_port_then_nick_is_the_order_of_the_checks() {
    let mut profile = Profile::blank("n");
    profile.nick = "fred".into();
    assert_eq!(profile.problem(), Some(Problem::MissingHost));
    profile.host = "  ".into();
    assert_eq!(profile.problem(), Some(Problem::MissingHost));
    profile.host = "irc.example".into();
    profile.port = "".into();
    assert_eq!(profile.problem(), Some(Problem::InvalidPort));
    profile.port = "0".into();
    assert_eq!(profile.problem(), Some(Problem::InvalidPort));
    profile.port = "66a".into();
    assert_eq!(profile.problem(), Some(Problem::InvalidPort));
    profile.port = "70000".into();
    assert_eq!(profile.problem(), Some(Problem::InvalidPort));
    profile.port = "6697".into();
    assert_eq!(profile.problem(), None);
    profile.nick = "".into();
    profile.port = "0".into();
    assert_eq!(profile.problem(), Some(Problem::InvalidPort));
}

#[test]
fn port_numbers_parse_like_qt_numbers_for_whole_values() {
    assert_eq!(port_number("6697"), 6697);
    assert_eq!(port_number(" 6697 "), 6697);
    assert_eq!(port_number("0"), 0);
    assert_eq!(port_number(""), 0);
    assert_eq!(port_number("66a"), 0);
    assert_eq!(port_number("6697.5"), 0);
    assert_eq!(port_number("65535"), 65535);
    assert_eq!(port_number("65536"), 0);
}

#[test]
fn a_nick_with_a_space_cannot_be_sent() {
    let (_, mut state) = seeded();
    state.set_text(ConnectField::Nick, "fr ed");
    assert_eq!(state.problem(), Some(Problem::UnsendableIdentity));
    state.set_text(ConnectField::Nick, "fr\0ed");
    assert_eq!(state.problem(), Some(Problem::UnsendableIdentity));
}

#[test]
fn username_and_real_name_are_checked_when_given() {
    let (_, mut state) = seeded();
    state.set_text(ConnectField::Username, "fr ed");
    assert_eq!(state.problem(), Some(Problem::UnsendableIdentity));
    state.set_text(ConnectField::Username, "fred");
    state.set_text(ConnectField::RealName, "Fred\nDev");
    assert_eq!(state.problem(), Some(Problem::UnsendableIdentity));
    state.set_text(ConnectField::RealName, "Fred Dev");
    assert_eq!(state.problem(), None, "a real name may have spaces");
}

#[test]
fn a_blank_username_and_real_name_fall_back_to_the_nick() {
    let mut profile = Profile::blank("n");
    profile.host = "irc.example".into();
    profile.nick = "fred".into();
    profile.username = "".into();
    profile.realname = "".into();
    assert_eq!(profile.problem(), None);
    profile.username = "fr ed".into();
    assert_eq!(
        profile.problem(),
        Some(Problem::UnsendableIdentity),
        "a username that is set is checked, a blank one is not"
    );
}

#[test]
fn an_autojoin_channel_with_a_nul_cannot_be_sent() {
    let mut profile = Profile::blank("n");
    profile.host = "irc.example".into();
    profile.nick = "fred".into();
    profile.autojoin = "#good #bad\0name".into();
    assert_eq!(profile.problem(), Some(Problem::UnsendableChannel));
    profile.autojoin = "#good, omarchy".into();
    assert_eq!(profile.problem(), None);
}

#[test]
fn autojoin_adds_hash_and_drops_keys_after_a_channel() {
    assert_eq!(parse_autojoin("omarchy lab", "#&"), ["#omarchy", "#lab"]);
    assert_eq!(parse_autojoin("a, b", "#&"), ["#a", "#b"]);
    assert_eq!(parse_autojoin("#a secret #b", "#&"), ["#a", "#b"]);
    assert_eq!(parse_autojoin("#a  ,, &b", "#&"), ["#a", "&b"]);
    assert!(parse_autojoin("  ,  ", "#&").is_empty());
}

#[test]
fn autojoin_uses_the_networks_chantypes() {
    assert_eq!(parse_autojoin("!x", "!"), ["!x"]);
    assert_eq!(parse_autojoin("#x", "!"), ["##x"]);
}

#[test]
fn normalized_trims_text_and_drops_secrets() {
    let mut profile = Profile::blank("n");
    profile.name = "  libera ".into();
    profile.host = " irc.libera.chat ".into();
    profile.port = " 6667 ".into();
    profile.nick = " fred ".into();
    profile.autojoin = "omarchy,  lab".into();
    profile.password = "hunter2".to_owned().into();
    profile.nickserv_password = "secret".to_owned().into();
    let saved = profile.normalized();
    assert_eq!(saved.name, "libera");
    assert_eq!(saved.host, "irc.libera.chat");
    assert_eq!(saved.port, "6667");
    assert_eq!(saved.nick, "fred");
    assert_eq!(saved.autojoin, "#omarchy #lab");
    assert!(saved.password.is_empty());
    assert!(saved.nickserv_password.is_empty());
}

#[test]
fn editing_makes_the_draft_dirty_and_typing_it_back_clears_it() {
    let (_, mut state) = seeded();
    state.set_text(ConnectField::Name, "libera");
    assert!(state.dirty());
    state.set_text(ConnectField::Name, "");
    assert!(!state.dirty());
}

#[test]
fn surrounding_spaces_are_not_an_edit() {
    let (_, mut state) = seeded();
    state.set_text(ConnectField::Host, "irc.example ");
    assert!(!state.dirty(), "Qt compares the normalized profiles");
}

#[test]
fn secrets_do_not_make_the_draft_dirty() {
    let (_, mut state) = seeded();
    state.set_text(ConnectField::ServerPassword, "hunter2");
    assert!(!state.dirty());
    assert_eq!(state.text(ConnectField::ServerPassword), "hunter2");
}

#[test]
fn every_text_field_reads_back_what_was_typed() {
    let (_, mut state) = seeded();
    let fields = [
        (ConnectField::Name, "n"),
        (ConnectField::Host, "h"),
        (ConnectField::Port, "7000"),
        (ConnectField::Nick, "nick"),
        (ConnectField::Username, "user"),
        (ConnectField::RealName, "real"),
        (ConnectField::Autojoin, "#x"),
        (ConnectField::ServerPassword, "p"),
        (ConnectField::NickServPassword, "q"),
    ];
    for (field, value) in fields {
        state.set_text(field, value);
        assert_eq!(state.text(field), value);
    }
    state.set_tls(false);
    state.set_connect_on_startup(true);
    assert!(!state.tls());
    assert!(state.connect_on_startup());
}

#[test]
fn the_stored_port_round_trips_and_a_bad_port_is_a_problem() {
    let mut state = ConnectState::from_stored(&[stored("network-1", "irc.libera.chat", "fred")]);
    assert_eq!(state.text(ConnectField::Port), "6697");
    state.set_text(ConnectField::Port, "abc");
    assert_eq!(state.problem(), Some(Problem::InvalidPort));
}
