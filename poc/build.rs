//! Build script. Reads `../version.pri`, the repository's single VERSION source,
//! and exports it to the crate as `OMAIRC_VERSION` (see `src/version.rs`). The
//! parser is shared with the app's test build through `src/version_pri.rs`.

#[path = "src/version_pri.rs"]
mod version_pri;

use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let pri = manifest.join("..").join("version.pri");
    println!("cargo:rerun-if-changed={}", pri.display());
    let body = std::fs::read_to_string(&pri)
        .unwrap_or_else(|err| panic!("cannot read {}: {err}", pri.display()));
    let version = version_pri::parse_version(&body)
        .unwrap_or_else(|| panic!("{} has no VERSION = value", pri.display()));
    println!("cargo:rustc-env=OMAIRC_VERSION={version}");
}
