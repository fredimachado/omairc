//! The `read --unread` cursor store: one JSON file per network, or per network and target, holding the
//! newest line the CLI returned. Desktop-only (no Go twin). Cursors live under
//! `<state>/<APP_FOLDER>/cli-cursors` ([`crate::identity::APP_FOLDER`]), so a default build never
//! shares a cursor with an installed Qt client. Qt's migration from older layouts does not apply to
//! a folder this build creates.
//!
//! A target's file name comes from its name folded by the network's CASEMAPPING, so `#Omarchy` and
//! `#omarchy` share one cursor. Without a known CASEMAPPING the fold is ASCII, as Qt's is.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::controller::cli::CliCursor;
use crate::irc::casemapping::{IrcCaseMapping, IrcCaseMappingKind};
use crate::storage::paths::{self, Env, Platform};

/// Overrides the cursor folder. Qt uses the same name, and its tests set it.
pub const ROOT_VAR: &str = "OMAIRC_CURSOR_ROOT";
/// The folder under the client state directory.
const FOLDER: &str = "cli-cursors";
/// A file-name segment longer than this is replaced by a hash of its text.
const MAX_SEGMENT_BYTES: usize = 96;

/// Where the cursors live. The UI thread owns one and reads and writes it on each `read --unread`.
pub struct CursorStore {
    root: PathBuf,
}

impl CursorStore {
    /// The store at the default folder, or at `OMAIRC_CURSOR_ROOT` when that is set.
    pub fn from_env() -> Self {
        Self::at(default_root())
    }

    pub fn at(root: PathBuf) -> Self {
        Self { root }
    }

    /// The cursor a target holds, or `None` when none was saved or the file cannot be read.
    pub fn load(
        &self,
        network_id: &str,
        target: &str,
        mapping: IrcCaseMapping,
        known: bool,
    ) -> Option<CliCursor> {
        let text = fs::read_to_string(self.path_for(network_id, target, mapping, known)).ok()?;
        let object: Value = serde_json::from_str(&text).ok()?;
        let stamp = object.get("timestamp")?.as_str()?;
        let timestamp = DateTime::parse_from_rfc3339(stamp).ok()?.with_timezone(&Utc);
        let msgid = object
            .get("msgid")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let sequence = object.get("sequence").and_then(Value::as_i64).unwrap_or(0);
        Some(CliCursor {
            timestamp,
            msgid,
            sequence,
        })
    }

    /// Records `cursor` for a target. Written through a temporary file, so a reader never sees half of
    /// a cursor.
    pub fn save(
        &self,
        network_id: &str,
        target: &str,
        mapping: IrcCaseMapping,
        known: bool,
        cursor: &CliCursor,
    ) -> io::Result<()> {
        let path = self.path_for(network_id, target, mapping, known);
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let body = json!({
            "msgid": cursor.msgid,
            "sequence": cursor.sequence,
            "target": target,
            "timestamp": cursor.timestamp.to_rfc3339_opts(SecondsFormat::Millis, true),
        });
        let temporary = path.with_extension("json.tmp");
        write_private(&temporary, format!("{body}\n").as_bytes())?;
        fs::rename(&temporary, &path)
    }

    /// The file for a network, or for a target on it. An untargeted read keeps its cursor at the
    /// network's own file, as Qt does.
    fn path_for(&self, network_id: &str, target: &str, mapping: IrcCaseMapping, known: bool) -> PathBuf {
        if target.is_empty() {
            return self.root.join(format!("{}.json", segment(network_id)));
        }
        let folded = fold_mapping(mapping, known).normalize(target);
        self.root
            .join(segment(network_id))
            .join(format!("{}.json", segment(&folded)))
    }
}

/// The CASEMAPPING a target folds by: the network's, when the server advertised one, else ASCII.
fn fold_mapping(mapping: IrcCaseMapping, known: bool) -> IrcCaseMapping {
    if known {
        mapping
    } else {
        IrcCaseMapping::new(IrcCaseMappingKind::Ascii)
    }
}

/// `$OMAIRC_CURSOR_ROOT`, else `<APP_FOLDER>/cli-cursors` under the platform state directory.
pub fn default_root() -> PathBuf {
    if let Some(root) = std::env::var_os(ROOT_VAR).filter(|value| !value.is_empty()) {
        return PathBuf::from(root);
    }
    paths::state_dir_for(&Env::from_process(), Platform::current()).join(FOLDER)
}

/// A file-name segment: the text with every byte outside `[A-Za-z0-9_-]` percent-encoded. A segment
/// that would be too long becomes `h-` and the hash of its text, so it stays unique.
fn segment(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_' {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    if encoded.len() <= MAX_SEGMENT_BYTES {
        return encoded;
    }
    let digest = Sha256::digest(text.as_bytes());
    let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("h-{hex}")
}

/// Writes a cursor file readable by its owner only. Windows keeps the profile's own permissions.
fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;
    file.write_all(bytes)?;
    file.flush()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    /// A fresh folder under the temp directory, removed when the guard drops.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(tag: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or_default();
            let path = std::env::temp_dir().join(format!(
                "{}-cursor-{tag}-{}-{nanos}",
                crate::identity::APP_FOLDER,
                std::process::id()
            ));
            Self(path)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn cursor(msgid: &str, sequence: i64) -> CliCursor {
        CliCursor {
            timestamp: Utc.with_ymd_and_hms(2026, 9, 12, 12, 0, 0).unwrap(),
            msgid: msgid.to_owned(),
            sequence,
        }
    }

    fn ascii() -> IrcCaseMapping {
        IrcCaseMapping::new(IrcCaseMappingKind::Ascii)
    }

    #[test]
    fn a_saved_cursor_loads_back_per_target() {
        let scratch = Scratch::new("round");
        let store = CursorStore::at(scratch.0.clone());
        assert_eq!(store.load("net", "#omarchy", ascii(), true), None);
        store.save("net", "#omarchy", ascii(), true, &cursor("m1", 4)).expect("save");
        assert_eq!(store.load("net", "#omarchy", ascii(), true), Some(cursor("m1", 4)));
        assert_eq!(store.load("net", "#other", ascii(), true), None);
        assert!(scratch.0.join("net").join("%23omarchy.json").exists());
    }

    #[test]
    fn an_untargeted_cursor_lives_at_the_network_file() {
        let scratch = Scratch::new("network");
        let store = CursorStore::at(scratch.0.clone());
        store.save("net", "", ascii(), true, &cursor("n1", 1)).expect("save");
        assert_eq!(store.load("net", "", ascii(), true), Some(cursor("n1", 1)));
        assert!(scratch.0.join("net.json").exists());
    }

    #[test]
    fn targets_that_differ_only_in_case_share_one_cursor() {
        let scratch = Scratch::new("casing");
        let store = CursorStore::at(scratch.0.clone());
        store.save("net", "#Omarchy", ascii(), true, &cursor("m2", 9)).expect("save");
        assert_eq!(store.load("net", "#omarchy", ascii(), true), Some(cursor("m2", 9)));
    }

    #[test]
    fn an_unreadable_file_is_no_cursor() {
        let scratch = Scratch::new("corrupt");
        let store = CursorStore::at(scratch.0.clone());
        let path = scratch.0.join("net").join("%23omarchy.json");
        fs::create_dir_all(path.parent().expect("parent")).expect("dir");
        fs::write(&path, "not json").expect("write");
        assert_eq!(store.load("net", "#omarchy", ascii(), true), None);
        fs::write(&path, r#"{"timestamp":"never"}"#).expect("write");
        assert_eq!(store.load("net", "#omarchy", ascii(), true), None);
    }

    #[test]
    fn long_targets_get_a_hashed_segment() {
        let long = "#".repeat(200);
        let name = segment(&long);
        assert!(name.starts_with("h-"));
        assert!(name.len() < 80);
        assert_ne!(name, segment(&"#".repeat(201)));
    }

    #[test]
    fn a_saved_cursor_is_private_on_unix() {
        let scratch = Scratch::new("perms");
        let store = CursorStore::at(scratch.0.clone());
        store.save("net", "#c", ascii(), true, &cursor("m3", 1)).expect("save");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(scratch.0.join("net").join("%23c.json"))
                .expect("metadata")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }
}
