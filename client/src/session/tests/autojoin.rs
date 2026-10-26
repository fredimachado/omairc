//! keyed JOINs, the stored key
//! that survives a reconnect, and the key a 475 drops.

use std::collections::BTreeMap;

use super::batch::history_config;
use super::fixture::{joins, Fixture};
use crate::irc::jointarget::IrcJoinTarget;
use crate::irc::serverfeatures::IrcServerFeatures;

fn keyed(channel: &str, key: &str) -> IrcJoinTarget {
    IrcJoinTarget::make(channel, Some(key), &IrcServerFeatures::new())
        .unwrap_or_else(|| panic!("keyed target {channel} must build"))
}

fn bare(channel: &str) -> IrcJoinTarget {
    IrcJoinTarget::make(channel, None, &IrcServerFeatures::new())
        .unwrap_or_else(|| panic!("bare target {channel} must build"))
}

#[test]
fn keyed_autojoin_survives_reconnect_then_part_forgets() {
    let fx = Fixture::new(history_config());
    fx.register_with_welcome();
    assert!(
        fx.session.join(&keyed("#private", "hunter2")),
        "the keyed join must succeed"
    );
    assert_eq!(fx.last_frame(), "JOIN #private hunter2\r\n");
    fx.inject(":omairc!u@h JOIN :#private\r\n");

    let before = fx.frames().len();
    fx.reconnect_registered();
    assert_eq!(
        joins(&fx.written_since(before)),
        vec!["JOIN #private hunter2\r\n"]
    );
    fx.inject(":omairc!u@h JOIN :#private\r\n");

    assert!(fx.session.part("#private"), "part must succeed");
    fx.inject(":omairc!u@h PART #private\r\n");
    let before = fx.frames().len();
    fx.reconnect_registered();
    assert!(
        joins(&fx.written_since(before)).is_empty(),
        "a PART forgets the channel and its key"
    );
}

#[test]
fn bare_join_does_not_wipe_stored_key() {
    let fx = Fixture::new(history_config());
    fx.register_with_welcome();
    assert!(fx.session.join(&keyed("#private", "hunter2")));
    fx.inject(":omairc!u@h JOIN :#private\r\n");
    assert!(
        fx.session.join(&bare("#private")),
        "the bare join must succeed"
    );
    assert_eq!(fx.last_frame(), "JOIN #private\r\n");
    fx.inject(":omairc!u@h JOIN :#private\r\n");
    let before = fx.frames().len();
    fx.reconnect_registered();
    assert_eq!(
        joins(&fx.written_since(before)),
        vec!["JOIN #private hunter2\r\n"]
    );
}

#[test]
fn join_with_new_key_updates_after_success() {
    let fx = Fixture::new(history_config());
    fx.register_with_welcome();
    assert!(fx.session.join(&keyed("#private", "hunter2")));
    fx.inject(":omairc!u@h JOIN :#private\r\n");
    assert!(
        fx.session.join(&keyed("#private", "newkey")),
        "the second keyed join must succeed"
    );
    assert_eq!(fx.last_frame(), "JOIN #private newkey\r\n");
    fx.inject(":omairc!u@h JOIN :#private\r\n");
    let before = fx.frames().len();
    fx.reconnect_registered();
    assert_eq!(
        joins(&fx.written_since(before)),
        vec!["JOIN #private newkey\r\n"]
    );
}

#[test]
fn welcome_sends_stored_channel_key() {
    let mut config = history_config();
    config.autojoin_channels = vec!["#omarchy".to_owned(), "#private".to_owned()];
    config.autojoin_keys = BTreeMap::from([("#private".to_owned(), "hunter2".to_owned())]);
    let fx = Fixture::new(config);
    fx.register_with_welcome();
    assert_eq!(
        joins(&fx.frames()),
        vec!["JOIN #omarchy\r\n", "JOIN #private hunter2\r\n"]
    );
}

#[test]
fn bad_channel_key_stays_without_retry() {
    let mut config = history_config();
    config.autojoin_channels = vec!["#secret".to_owned()];
    config.autojoin_keys = BTreeMap::from([("#secret".to_owned(), "hunter2".to_owned())]);
    let fx = Fixture::new(config);
    fx.register_with_welcome();
    assert_eq!(joins(&fx.frames()), vec!["JOIN #secret hunter2\r\n"]);

    let after_welcome = fx.frames().len();
    fx.inject(":server 475 omairc #secret :Cannot join channel (+k)\r\n");
    assert_eq!(
        fx.frames().len(),
        after_welcome,
        "a bad channel key must not be retried"
    );
    assert!(
        fx.messages().iter().any(|message| message.command == "475"),
        "475 must reach the translator"
    );

    let before = fx.frames().len();
    fx.reconnect_registered();
    assert_eq!(joins(&fx.written_since(before)), vec!["JOIN #secret\r\n"]);
}
