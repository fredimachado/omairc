//! The identity the transcript, the identity footer and the direct-message header read
//! off the controller over the real demo: a nick's account beside its name, the network's
//! own account, and the peer header a direct message shows. The seed names its accounts
//! (`fredm`, `kaidev`, `teoval`) and its real names (`Anna Vale`), and it logs `ivy` out.

use super::fixture::{Fixture, OMARCHY};

#[test]
fn a_nick_carries_its_display_account_into_the_transcript() {
    let fixture = Fixture::seeded();
    let facts = fixture.controller.nick_facts(OMARCHY);
    let account = |nick: &str| {
        facts
            .get(nick)
            .map(|facts| facts.account.clone())
            .unwrap_or_default()
    };
    assert_eq!(
        account("fred"),
        "fredm",
        "our own account shows beside our nick"
    );
    assert_eq!(
        account("kai"),
        "kaidev",
        "an account from the account-tag shows"
    );
    assert_eq!(
        account("teo"),
        "teoval",
        "an account from account-notify shows"
    );
    assert_eq!(account("ivy"), "", "a logged-out nick shows no account");
    assert_eq!(account("anna"), "", "a nick with no account shows none");
}

#[test]
fn the_network_reads_the_display_account_of_a_nick() {
    let fixture = Fixture::seeded();
    assert_eq!(fixture.controller.display_account(OMARCHY, "fred"), "fredm");
    assert_eq!(fixture.controller.display_account(OMARCHY, "ivy"), "");
    assert_eq!(fixture.controller.display_account(OMARCHY, ""), "");
}

#[test]
fn a_channel_has_no_peer_header_and_its_topic_is_the_second_line() {
    let fixture = Fixture::seeded();
    assert!(fixture.controller.is_channel());
    assert!(
        fixture.controller.peer_header().is_none(),
        "a channel names no peer"
    );
    assert!(
        !fixture.controller.topic().is_empty(),
        "a channel's second line is its topic"
    );
    assert!(
        fixture.controller.has_away_presence(),
        "the demo network sends away-notify"
    );
}

#[test]
fn a_direct_message_header_reads_the_peers_presence_name_and_labels() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    assert!(fixture.controller.open_direct("anna"));

    let header = fixture
        .controller
        .peer_header()
        .expect("a direct message has a peer header");
    assert_eq!(
        header.presence, "online",
        "anna shares our channel and is not away"
    );
    assert_eq!(
        header.realname, "Anna Vale",
        "the WHO real name is the subtitle"
    );
    assert!(
        header.labels.is_empty(),
        "anna has no account, operator or bot label"
    );
    assert!(
        fixture.controller.topic().is_empty(),
        "a direct message has no topic line, so the header takes the real name"
    );
}

#[test]
fn an_away_peer_reads_away_and_shows_its_account_as_a_label() {
    let mut fixture = Fixture::seeded();
    assert!(fixture.controller.open_direct("teo"));

    let header = fixture.controller.peer_header().expect("a peer header");
    assert_eq!(header.presence, "away", "teo is away in a channel we share");
    assert_eq!(header.labels, ["teoval"], "the account is the one label");
    assert!(header.realname.is_empty(), "teo has no real name to show");
}

#[test]
fn a_peer_we_share_nothing_with_is_offline_and_has_an_empty_subtitle() {
    let mut fixture = Fixture::seeded();
    assert!(fixture.controller.open_direct("newcomer"));

    let header = fixture.controller.peer_header().expect("a peer header");
    assert_eq!(
        header.presence, "offline",
        "no shared channel proves nothing"
    );
    assert!(
        header.realname.is_empty(),
        "no real name means an empty subtitle"
    );
    assert!(header.labels.is_empty());
}

#[test]
fn status_has_no_peer_header() {
    let mut fixture = Fixture::seeded();
    fixture.controller.open_status(OMARCHY);
    assert!(fixture.controller.shows_status());
    assert!(fixture.controller.peer_header().is_none());
}
