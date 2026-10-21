//! Port of `tui/internal/session/batch_ledger_test.go`: batches that straddle a
//! PART, a JOIN, or a capability that never came.

use super::batch::{bodies, history_config, register_with_history, since};
use super::fixture::Fixture;
use crate::session::events::HistoryKind;

#[test]
fn leftover_history_batch_after_part_does_not_emit() {
    let fx = Fixture::new(history_config());
    register_with_history(&fx, true);
    fx.inject(
        ":irc.host BATCH +stale chathistory #omarchy\r\n\
         @batch=stale :alice!u@h PRIVMSG #omarchy :old\r\n\
         :omairc!u@h PART :#omarchy\r\n\
         :omairc!u@h JOIN :#omarchy\r\n\
         :irc.host BATCH -stale\r\n",
    );
    assert!(fx.batches().is_empty());
}

#[test]
fn delayed_history_batch_after_part_does_not_emit() {
    let fx = Fixture::new(history_config());
    register_with_history(&fx, true);
    fx.inject(
        ":omairc!u@h PART :#omarchy\r\n\
         :irc.host BATCH +stale chathistory #omarchy\r\n\
         @batch=stale :alice!u@h PRIVMSG #omarchy :old\r\n\
         :omairc!u@h JOIN :#omarchy\r\n\
         :irc.host BATCH -stale\r\n",
    );
    assert!(fx.batches().is_empty());
    assert!(
        fx.session.history_pending(),
        "history must be pending after the rejoin"
    );

    fx.inject(
        ":irc.host BATCH +hx chathistory #omarchy\r\n\
         @batch=hx :alice!u@h PRIVMSG #omarchy :fresh\r\n\
         :irc.host BATCH -hx\r\n",
    );
    let batches = fx.batches();
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0].target, "#omarchy");
    assert_eq!(batches[0].lines.len(), 1);
    assert!(
        !fx.session.history_pending(),
        "history must no longer be pending"
    );
}

#[test]
fn leftover_tagged_history_line_after_part_is_swallowed() {
    let fx = Fixture::new(history_config());
    register_with_history(&fx, true);
    let start = fx.messages().len();
    fx.inject(
        ":irc.host BATCH +stale chathistory #omarchy\r\n\
         @batch=stale :alice!u@h PRIVMSG #omarchy :old\r\n\
         :omairc!u@h PART :#omarchy\r\n\
         @batch=stale :alice!u@h PRIVMSG #omarchy :late\r\n\
         :omairc!u@h JOIN :#omarchy\r\n\
         :irc.host BATCH -stale\r\n",
    );
    assert!(bodies(&since(&fx, start)).is_empty(), "no PRIVMSG may pass");
    assert!(fx.batches().is_empty());
}

#[test]
fn chat_history_batch_with_batch_alone_is_ignored() {
    let fx = Fixture::new(history_config());
    fx.connect_tls();
    fx.inject(
        ":server CAP omairc LS :batch\r\n\
         :server CAP omairc ACK :batch\r\n\
         :server 001 omairc :Welcome\r\n\
         :omairc!u@h JOIN :#omarchy\r\n",
    );
    assert!(
        !fx.session.history_pending(),
        "history must not be pending without chathistory"
    );
    let start = fx.messages().len();
    fx.inject(
        ":irc.host BATCH +hx chathistory #omarchy\r\n\
         @batch=hx :alice!u@h PRIVMSG #omarchy :older\r\n\
         :irc.host BATCH -hx\r\n\
         :bob!u@h PRIVMSG #omarchy :after\r\n",
    );
    assert!(fx.batches().is_empty());
    assert_eq!(bodies(&since(&fx, start)), vec!["after"]);
}

#[test]
fn self_join_keeps_open_playback_and_drops_stale_chat_history() {
    let fx = Fixture::new(history_config());
    fx.connect_tls();
    fx.inject(
        ":server CAP omairc LS :batch chathistory znc.in/playback\r\n\
         :server CAP omairc ACK :batch chathistory znc.in/playback\r\n\
         :server 001 omairc :Welcome\r\n\
         :znc.in BATCH +pb znc.in/playback #omarchy\r\n\
         @batch=pb :lena!u@h PRIVMSG #omarchy :buffered\r\n\
         :omairc!u@h JOIN :#omarchy\r\n\
         :znc.in BATCH -pb\r\n",
    );
    let batches = fx.batches();
    assert_eq!(batches.len(), 1);
    let batch = &batches[0];
    assert_eq!(batch.kind, Some(HistoryKind::BouncerPlayback));
    assert_eq!(batch.target, "#omarchy");
    assert_eq!(batch.lines.len(), 1);
    assert_eq!(
        String::from_utf8_lossy(batch.lines[0].params.last().unwrap()),
        "buffered"
    );

    fx.inject(
        ":irc.host BATCH +hx chathistory #lab\r\n\
         @batch=hx :alice!u@h PRIVMSG #lab :stale\r\n\
         :omairc!u@h JOIN :#lab\r\n\
         :irc.host BATCH -hx\r\n",
    );
    assert_eq!(fx.batches().len(), 1);

    fx.inject(
        ":omairc!u@h JOIN :#parted\r\n\
         :znc.in BATCH +gone znc.in/playback #parted\r\n\
         @batch=gone :lena!u@h PRIVMSG #parted :lost\r\n\
         :omairc!u@h PART :#parted\r\n\
         :znc.in BATCH -gone\r\n",
    );
    assert_eq!(fx.batches().len(), 1);
}
