//! Replays the wire corpus files (the `include_bytes!` entries below) through
//! `IrcFramer` and `parse`. Each file is replayed whole, in halves and byte by
//! byte. A file with a fault is replayed whole only, because a fault's byte count
//! depends on where the chunk boundaries fall. `tests/irc_corpus.rs` checks that
//! this registry names every file in the corpus directory, since this module
//! cannot read the directory itself.

use super::framer::{IrcFrameFault, IrcFramer};
use super::message::{IrcError, IrcMessage, IrcPrefix, IrcTag};
use super::parser::parse;

/// One expected frame and the faults the framer must report for its file.
struct CorpusCase {
    file_name: &'static str,
    bytes: &'static [u8],
    frames: Vec<IrcMessage>,
    faults: Vec<ExpectedFault>,
}

struct ExpectedFault {
    error: IrcError,
    byte_count: usize,
    preview: &'static [u8],
}

#[derive(Clone, Copy, Debug)]
enum Chunking {
    Whole,
    Halves,
    Bytes,
}

fn tag(name: &str, value: Option<&str>) -> IrcTag {
    IrcTag {
        name: name.to_owned(),
        value: value.map(|text| text.as_bytes().to_vec()),
    }
}

fn prefix(raw: &str, nick: &str, user: &str, host: &str) -> IrcPrefix {
    IrcPrefix {
        raw: raw.as_bytes().to_vec(),
        nick: nick.as_bytes().to_vec(),
        user: user.as_bytes().to_vec(),
        host: host.as_bytes().to_vec(),
    }
}

fn privmsg(tags: Vec<IrcTag>, source: IrcPrefix, params: &[&[u8]]) -> IrcMessage {
    IrcMessage {
        tags,
        prefix: Some(source),
        command: "PRIVMSG".to_owned(),
        params: params.iter().map(|param| param.to_vec()).collect(),
    }
}

/// The corpus registry, mirroring `corpusCases` in the Go test and the C++ table.
fn corpus_cases() -> Vec<CorpusCase> {
    vec![
        CorpusCase {
            file_name: "01-classic-privmsg.txt",
            bytes: include_bytes!("testdata/corpus/01-classic-privmsg.txt"),
            frames: vec![privmsg(
                Vec::new(),
                prefix("n!u@h", "n", "u", "h"),
                &[b"#c", b"hello world"],
            )],
            faults: Vec::new(),
        },
        CorpusCase {
            file_name: "02-tagged-privmsg.txt",
            bytes: include_bytes!("testdata/corpus/02-tagged-privmsg.txt"),
            frames: vec![privmsg(
                vec![tag("msgid", Some("abc")), tag("+typing", Some("active"))],
                prefix("n!u@h", "n", "u", "h"),
                &[b"#c", b"tagged"],
            )],
            faults: Vec::new(),
        },
        CorpusCase {
            file_name: "03-latin1-e9-body.txt",
            bytes: include_bytes!("testdata/corpus/03-latin1-e9-body.txt"),
            frames: vec![privmsg(
                Vec::new(),
                prefix("n!u@h", "n", "u", "h"),
                &[b"#c", b"caf\xe9"],
            )],
            faults: Vec::new(),
        },
        CorpusCase {
            file_name: "04-overlong.txt",
            bytes: include_bytes!("testdata/corpus/04-overlong.txt"),
            frames: vec![IrcMessage {
                command: "X".repeat(513),
                ..IrcMessage::default()
            }],
            faults: Vec::new(),
        },
        CorpusCase {
            file_name: "05-nul.txt",
            bytes: include_bytes!("testdata/corpus/05-nul.txt"),
            frames: Vec::new(),
            faults: vec![ExpectedFault {
                error: IrcError::InvalidCharacter,
                byte_count: 18,
                preview: b"PRIVMSG #c :hel\0lo",
            }],
        },
    ]
}

/// Feeds `case` through a fresh framer in `chunking` and checks every frame and fault.
fn replay(case: &CorpusCase, chunking: Chunking) {
    let label = format!("{}:{chunking:?}", case.file_name);
    let raw = case.bytes;
    let mut framer = IrcFramer::new();
    let mut frames: Vec<Vec<u8>> = Vec::new();
    let mut faults: Vec<IrcFrameFault> = Vec::new();
    let mut feed = |chunk: &[u8]| {
        let result = framer.feed(chunk);
        frames.extend(result.frames);
        faults.extend(result.faults);
    };

    match chunking {
        Chunking::Whole => feed(raw),
        Chunking::Halves => {
            let half = raw.len() / 2;
            feed(&raw[..half]);
            feed(&raw[half..]);
        }
        Chunking::Bytes => {
            for index in 0..raw.len() {
                feed(&raw[index..index + 1]);
            }
        }
    }

    assert_eq!(frames.len(), case.frames.len(), "{label}: frame count, got {frames:?}");
    assert_eq!(faults.len(), case.faults.len(), "{label}: fault count, got {faults:?}");

    for (index, expected) in case.faults.iter().enumerate() {
        let fault = &faults[index];
        assert_eq!(fault.error, expected.error, "{label}: fault {index} error");
        assert_eq!(fault.byte_count, expected.byte_count, "{label}: fault {index} byte count");
        assert_eq!(fault.preview, expected.preview, "{label}: fault {index} preview");
    }

    for (index, expected) in case.frames.iter().enumerate() {
        let message = parse(&frames[index])
            .unwrap_or_else(|error| panic!("{label}: frame {index} failed to parse: {error}"));
        assert_eq!(&message, expected, "{label}: frame {index}");
    }
}

#[test]
fn every_corpus_file_replays_in_every_chunking() {
    for case in corpus_cases() {
        let chunkings: &[Chunking] = if case.faults.is_empty() {
            &[Chunking::Whole, Chunking::Halves, Chunking::Bytes]
        } else {
            &[Chunking::Whole]
        };
        for &chunking in chunkings {
            replay(&case, chunking);
        }
    }
}
