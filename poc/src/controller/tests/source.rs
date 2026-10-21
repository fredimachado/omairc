//! The controller's production source stays free of gpui, sockets, threads, processes
//! and the filesystem. The UI thread owns the controller, and a transport delivers
//! into it, so none of those belong here. A source scan keeps that true.

use std::fs;
use std::path::{Path, PathBuf};

/// Every production `.rs` file under `dir`, skipping the test tree.
fn production_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(dir).expect("the controller directory exists");
    for entry in entries {
        let path = entry.expect("a directory entry").path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == "tests") {
                continue;
            }
            production_sources(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn controller_source_names_no_gpui_socket_thread_process_or_filesystem() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/controller");
    let mut files = Vec::new();
    production_sources(&root, &mut files);
    assert!(files.len() > 10, "the scan found the controller: {files:?}");

    let banned = [
        "gpui::",
        "use gpui",
        "std::net",
        "std::thread",
        "std::sync::mpsc",
        "std::process",
        "std::fs",
    ];
    for file in files {
        let text = fs::read_to_string(&file).expect("a readable source file");
        for word in banned {
            assert!(
                !text.contains(word),
                "{} names {word}; the controller stays portable",
                file.display()
            );
        }
    }
}
