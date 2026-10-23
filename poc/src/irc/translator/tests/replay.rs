//! Batch replay: history lines, targets batches and bouncer queries.

use super::*;

#[test]
fn history_batch_keeps_the_server_time_and_falls_back_to_now() {
    let now = reducer_timestamp();
    let event = history_of(
        &batch("lena", None, &[":lena!u@h PRIVMSG omairc :hi"]),
        None,
    );
    assert_eq!(event.lines.len(), 1);
    let line = &event.lines[0];
    assert_eq!(line.author, "lena");
    assert_eq!(line.body, "hi");
    assert!(line.server_time.is_none());
    assert_eq!(
        line.timestamp, now,
        "no time tag falls back to the wall clock"
    );

    let event = history_of(
        &batch(
            "lena",
            None,
            &["@time=2024-03-09T16:00:00.620Z :lena!u@h PRIVMSG omairc :hi"],
        ),
        None,
    );
    let line = &event.lines[0];
    let want =
        Utc.with_ymd_and_hms(2024, 3, 9, 16, 0, 0).unwrap() + chrono::Duration::milliseconds(620);
    assert_eq!(line.server_time, Some(want));
    assert_eq!(line.timestamp, want, "the timestamp uses the tag");

    assert!(
        translate_history(
            NETWORK_A,
            "omairc",
            &features(),
            &HistoryBatch::default(),
            now,
            None
        )
        .is_none(),
        "an empty target must not translate"
    );
}

#[test]
fn targets_batches_do_not_translate_into_lines() {
    let targets = HistoryBatch {
        target: "lena".to_owned(),
        kind: Some(HistoryKind::Targets),
        ..HistoryBatch::default()
    };
    assert!(translate_history(
        NETWORK_A,
        "omairc",
        &features(),
        &targets,
        reducer_timestamp(),
        None
    )
    .is_none());
}

#[test]
fn bouncer_query_keeps_lines_under_a_previous_nick_but_nothing_else() {
    let event = history_of(
        &batch(
            "lena",
            Some(HistoryKind::BouncerPlayback),
            &[
                ":oldnick!u@h PRIVMSG lena :mine",
                ":lena!u@h PRIVMSG oldnick :before",
                ":stranger!u@h PRIVMSG oldnick :elsewhere",
                ":lena!u@h PRIVMSG #other :channel",
                ":NickServ!s@services PRIVMSG lena :hi",
            ],
        ),
        None,
    );
    assert_eq!(event.lines.len(), 2);
    assert_eq!(event.lines[0].author, "oldnick");
    assert_eq!(event.lines[0].body, "mine");
    assert_eq!(event.lines[1].author, "lena");
    assert_eq!(event.lines[1].body, "before");
}

#[test]
fn chat_history_keeps_only_the_current_nick() {
    let event = history_of(
        &batch("lena", None, &[":oldnick!u@h PRIVMSG lena :mine"]),
        None,
    );
    assert!(event.lines.is_empty());
}

#[test]
fn channel_history_keeps_joins_as_event_lines_and_chat_as_chat() {
    let event = history_of(
        &batch(
            "#omarchy",
            None,
            &[
                ":alice!u@h JOIN :#omarchy",
                ":alice!u@h PRIVMSG #omarchy :from history",
            ],
        ),
        None,
    );
    assert_eq!(event.lines.len(), 2);
    assert_eq!(event.lines[0].author, "");
    assert_eq!(event.lines[0].kind, MessageKindTag::Event);
    assert_eq!(event.lines[0].body, "alice joined");
    assert_eq!(event.lines[1].body, "from history");
}
