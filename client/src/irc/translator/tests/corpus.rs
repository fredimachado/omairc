//! Corpus frames from testdata, as the framer hands them to the translator.

use super::*;

#[test]
fn corpus_frames_translate_to_the_expected_events() {
    // 01: a classic line. The corpus file carries CRLF, which the framer strips.
    let classic = corpus_frame(include_bytes!(
        "../../testdata/corpus/01-classic-privmsg.txt"
    ));
    let events = translate(
        NETWORK_A,
        "omairc",
        &features(),
        &classic,
        reducer_timestamp(),
        None,
    );
    let message = message_of(&events);
    assert_eq!(message.conversation.normalized_target, "#c");
    assert_eq!(message.author, "n");
    assert_eq!(message.body, "hello world");
    assert_eq!(message.msgid, Id::default());

    // 02: a tagged line. msgid survives and the +typing tag adds no event.
    let tagged = corpus_frame(include_bytes!(
        "../../testdata/corpus/02-tagged-privmsg.txt"
    ));
    let events = translate(
        NETWORK_A,
        "omairc",
        &features(),
        &tagged,
        reducer_timestamp(),
        None,
    );
    assert_eq!(
        events.len(),
        1,
        "+typing on a PRIVMSG is not a typing event"
    );
    let message = message_of(&events);
    assert_eq!(message.msgid, Id::new("abc"));
    assert_eq!(message.body, "tagged");

    // 03: the Latin-1 body decodes byte for byte as the scalar it names.
    let latin1 = corpus_frame(include_bytes!(
        "../../testdata/corpus/03-latin1-e9-body.txt"
    ));
    let events = translate(
        NETWORK_A,
        "omairc",
        &features(),
        &latin1,
        reducer_timestamp(),
        None,
    );
    assert_eq!(message_of(&events).body, "caf\u{e9}");
}

#[test]
fn corpus_tagged_line_replays_as_chat_with_its_msgid() {
    let tagged = corpus_frame(include_bytes!(
        "../../testdata/corpus/02-tagged-privmsg.txt"
    ));
    let history = history_of(
        &HistoryBatch {
            target: "#c".to_owned(),
            lines: vec![tagged],
            ..HistoryBatch::default()
        },
        None,
    );
    assert_eq!(history.lines.len(), 1);
    assert_eq!(history.lines[0].kind, MessageKindTag::Chat);
    assert_eq!(history.lines[0].msgid, Id::new("abc"));
}
