//! Build script. Exports `OMAIRC_VERSION` (see `src/version.rs`): prefers the
//! `OMAIRC_BUILD_VERSION` env when packaging/CI set it, otherwise reads
//! `../version.pri`, the repository's single VERSION source. The parser is
//! shared with the app's test build through `src/version_pri.rs`.
//!
//! On a Windows target it also embeds `data/icons/omairc.ico` and a version
//! resource in the exe. The icon is resource 1: gpui's window class loads
//! `MAKEINTRESOURCE(1)` for its icon, so the title bar and taskbar show the
//! mark without any window-side change.

#[path = "src/version_pri.rs"]
mod version_pri;

use std::path::{Path, PathBuf};

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let pri = manifest.join("..").join("version.pri");
    println!("cargo:rerun-if-changed={}", pri.display());
    println!("cargo:rerun-if-env-changed=OMAIRC_BUILD_VERSION");
    // Packaging and CI pass OMAIRC_BUILD_VERSION (display version). Otherwise
    // read VERSION from version.pri, the repository's single release source.
    let version = match std::env::var("OMAIRC_BUILD_VERSION") {
        Ok(value) if !value.is_empty() => value,
        _ => {
            let body = std::fs::read_to_string(&pri)
                .unwrap_or_else(|err| panic!("cannot read {}: {err}", pri.display()));
            version_pri::parse_version(&body)
                .map(str::to_owned)
                .unwrap_or_else(|| panic!("{} has no VERSION = value", pri.display()))
        }
    };
    println!("cargo:rustc-env=OMAIRC_VERSION={version}");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let icon = manifest
            .join("..")
            .join("data")
            .join("icons")
            .join("omairc.ico");
        println!("cargo:rerun-if-changed={}", icon.display());
        embed_windows_resources(&icon, &version);
    }
}

/// Compiles the icon and version resource with `rc.exe` (found in the Windows SDK
/// by `embed-resource`) and links it into the binaries. The script lives in
/// OUT_DIR and names the icon by absolute path, so the repository file is used
/// in place rather than copied.
#[cfg(windows)]
fn embed_windows_resources(icon: &Path, version: &str) {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let script = out_dir.join("omairc.rc");
    std::fs::write(&script, resource_script(icon, version))
        .unwrap_or_else(|err| panic!("cannot write {}: {err}", script.display()));
    embed_resource::compile(&script, embed_resource::NONE)
        .manifest_required()
        .unwrap_or_else(|err| panic!("cannot embed the Windows resources: {err}"));
}

/// Cross-compiling the exe from a non-Windows host has no `rc.exe`, so the exe
/// keeps the default icon. Native Windows builds always take the branch above.
#[cfg(not(windows))]
fn embed_windows_resources(_icon: &Path, _version: &str) {
    println!(
        "cargo:warning=omairc: the Windows icon is not embedded when cross-compiling"
    );
}

/// The `.rc` text: the icon as resource 1, then a VERSIONINFO block whose file and
/// product versions come from `version.pri`.
#[cfg(windows)]
fn resource_script(icon: &Path, version: &str) -> String {
    let icon = icon.display().to_string().replace('\\', "/");
    let [major, minor, patch, build] = numeric_quad(version);
    format!(
        r#"1 ICON "{icon}"

1 VERSIONINFO
 FILEVERSION {major},{minor},{patch},{build}
 PRODUCTVERSION {major},{minor},{patch},{build}
 FILEFLAGSMASK 0x3fL
 FILEFLAGS 0x0L
 FILEOS 0x40004L
 FILETYPE 0x1L
 FILESUBTYPE 0x0L
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "040904b0"
        BEGIN
            VALUE "FileDescription", "Omairc"
            VALUE "FileVersion", "{version}"
            VALUE "InternalName", "omairc"
            VALUE "OriginalFilename", "omairc.exe"
            VALUE "ProductName", "Omairc"
            VALUE "ProductVersion", "{version}"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x409, 1200
    END
END
"#
    )
}

/// The four numbers of a VERSIONINFO `FILEVERSION`. Each dot-separated part
/// contributes its leading digits, so `1.10.3` gives `1,10,3,0` and `1.0.7alpha`
/// gives `1,0,7,0`.
#[cfg(windows)]
fn numeric_quad(version: &str) -> [u32; 4] {
    let mut quad = [0u32; 4];
    for (slot, part) in quad.iter_mut().zip(version.split('.')) {
        let digits: String = part.chars().take_while(char::is_ascii_digit).collect();
        *slot = digits.parse().unwrap_or(0);
    }
    quad
}
