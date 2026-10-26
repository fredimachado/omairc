//! The corpus registry gate. `src/irc/corpus.rs` replays every file in
//! `src/irc/testdata/corpus/`, and it names each file in an `include_bytes!`.
//! This test checks that the two sets match, as `TestCorpusRegistryMatchesDirectory`
//! does in the Go suite. `src/irc/` cannot read the directory itself, because the
//! portability gate in `tests/irc_portability.rs` forbids `std::fs` there, so the
//! read happens here.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

/// The path prefix every `include_bytes!` in the registry shares.
const REGISTRY_PREFIX: &str = "testdata/corpus/";

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn directory_corpus_files() -> BTreeSet<String> {
    let directory = crate_root().join("src").join("irc").join(REGISTRY_PREFIX);
    let entries = fs::read_dir(&directory).unwrap_or_else(|err| panic!("read {}: {err}", directory.display()));
    entries
        .map(|entry| entry.expect("directory entry").path())
        .filter(|path| path.is_file() && path.extension().is_some_and(|ext| ext == "txt"))
        .map(|path| path.file_name().expect("file name").to_string_lossy().into_owned())
        .collect()
}

/// The file names the registry includes, in order of appearance.
fn registered_corpus_files(source: &str) -> Vec<String> {
    source
        .match_indices(REGISTRY_PREFIX)
        .map(|(start, _)| {
            let rest = &source[start + REGISTRY_PREFIX.len()..];
            let end = rest.find('"').expect("the registry path closes with a quote");
            rest[..end].to_owned()
        })
        .collect()
}

#[test]
fn corpus_registry_matches_directory() {
    let source_path = crate_root().join("src").join("irc").join("corpus.rs");
    let source = fs::read_to_string(&source_path).unwrap_or_else(|err| panic!("read {}: {err}", source_path.display()));

    let registered = registered_corpus_files(&source);
    let unique: BTreeSet<&String> = registered.iter().collect();
    assert_eq!(unique.len(), registered.len(), "duplicate corpus registry entries: {registered:?}");

    let registered: BTreeSet<String> = registered.into_iter().collect();
    let actual = directory_corpus_files();
    assert!(!actual.is_empty(), "no corpus files found");
    assert_eq!(actual, registered, "src/irc/testdata/corpus/ files vs the registry in src/irc/corpus.rs");
}
