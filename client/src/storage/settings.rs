//! `IrcProfileStore` (`tui/internal/storage/profilestore.go`). The file is the INI that
//! Qt's `QSettings` writes: the escaping, the `[section]` layout, the `@Invalid`
//! sentinel and the `@@` escape all match, so a file written here reads back in the
//! Qt client and the TUI.
//! Writes fail closed. A malformed or unreadable file is never overwritten, so a
//! corrupt store is not rebuilt from memory and silently loses its contents.
//! The stores in this folder (profiles, open directs, lists, preferences, scroll places)
//! read and write through this API.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::atomicwrite::{write_atomic, write_atomic_preserve_mode};

#[path = "settings_snapshot.rs"]
mod settings_snapshot;

use self::settings_snapshot::encoded_matches_disk;

/// The result of a fail-closed probe of the raw bytes (`probeSettingsIni`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IniProbe {
    /// The file is missing, or a readable, well-formed INI.
    Ok,
    /// The raw bytes are not a valid INI. Writing would drop data silently.
    Malformed,
    /// The file exists but cannot be read. Writing would rebuild it from a partial view.
    Unreadable,
}

/// The outcome of a write (`IrcProfileStore::Status`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Written,
    /// There was nothing to write or remove: no network id, or no such entry.
    Absent,
    /// The file could not be read or written.
    AccessError,
    /// The file is not a well-formed INI. Settings leaves it alone; the STS store
    /// may back it up and rewrite.
    FormatError,
}

/// One persisted value. QSettings stores a plain string, a string list, or an
/// invalid QVariant written as `@Invalid`. A one-item list is written without a
/// comma, so it reads back as a string, exactly as in QSettings.
/// `Variant` is any other QVariant encoding (`@Rect(...)` and the like). It reads as
/// text, but it is written verbatim: a plain string that starts with `@` is written
/// with an extra `@`, and a `Variant` must not be.
#[derive(Clone, Debug, PartialEq, Eq)]
enum IniValue {
    Text(String),
    Variant(String),
    List(Vec<String>),
    Invalid,
}

/// A QSettings-compatible key/value store. A group is a `/`-joined path such as
/// `networks/abc`. Group and key names are case-sensitive.
#[derive(Clone, Debug)]
pub struct Settings {
    path: PathBuf,
    values: BTreeMap<String, IniValue>,
    dirty: bool,
    on_disk: Option<Vec<u8>>,
    preserve_file_mode: bool,
}

impl Settings {
    /// In-memory store for encoding without reading or writing a path.
    pub(in crate::storage) fn in_memory() -> Self {
        settings_snapshot::in_memory()
    }

    /// The stored value for `group`/`key`, or an empty string when the key is absent.
    pub fn value(&self, group: &str, key: &str) -> String {
        self.text(group, key).unwrap_or_default().to_string()
    }

    /// Whether the key is present, whatever its value.
    pub fn contains(&self, group: &str, key: &str) -> bool {
        self.values.contains_key(&settings_key(group, key))
    }

    /// Parses the value as a QSettings bool: `true`/`1` or `false`/`0`, in any case.
    /// Returns `fallback` for an absent key or any other text.
    pub fn bool_value(&self, group: &str, key: &str, fallback: bool) -> bool {
        match self.text(group, key).map(str::to_lowercase).as_deref() {
            Some("true" | "1") => true,
            Some("false" | "0") => false,
            _ => fallback,
        }
    }

    /// Parses the value as a base-10 unsigned integer, or `fallback`.
    pub fn uint_value(&self, group: &str, key: &str, fallback: u64) -> u64 {
        self.text(group, key)
            .and_then(|text| text.trim().parse().ok())
            .unwrap_or(fallback)
    }

    /// Parses the value as a base-10 signed integer, or `fallback`.
    pub fn int_value(&self, group: &str, key: &str, fallback: i64) -> i64 {
        self.text(group, key)
            .and_then(|text| text.trim().parse().ok())
            .unwrap_or(fallback)
    }

    /// The stored string list, or `None` when the key is absent. An empty string and
    /// `@Invalid` both read as an empty list. A plain string is a one-item list.
    pub fn string_list(&self, group: &str, key: &str) -> Option<Vec<String>> {
        match self.values.get(&settings_key(group, key))? {
            IniValue::List(items) => Some(items.clone()),
            IniValue::Invalid => Some(Vec::new()),
            IniValue::Text(text) | IniValue::Variant(text) if text.is_empty() => Some(Vec::new()),
            IniValue::Text(text) | IniValue::Variant(text) => Some(vec![text.clone()]),
        }
    }

    pub fn set_value(&mut self, group: &str, key: &str, value: &str) {
        self.values
            .insert(settings_key(group, key), IniValue::Text(value.to_string()));
        self.dirty = true;
    }

    /// Stores an already-encoded QVariant such as `@Rect(0 0 800 600)`, written
    /// verbatim so Qt reads it back as that type. [`Settings::set_value`] is for plain
    /// strings, and it escapes a leading `@`.
    pub fn set_variant(&mut self, group: &str, key: &str, encoded: &str) {
        self.values.insert(
            settings_key(group, key),
            IniValue::Variant(encoded.to_string()),
        );
        self.dirty = true;
    }

    pub fn set_bool(&mut self, group: &str, key: &str, value: bool) {
        self.set_value(group, key, if value { "true" } else { "false" });
    }

    pub fn set_uint(&mut self, group: &str, key: &str, value: u64) {
        self.set_value(group, key, &value.to_string());
    }

    pub fn set_int(&mut self, group: &str, key: &str, value: i64) {
        self.set_value(group, key, &value.to_string());
    }

    /// Stores a string list. An empty list is written as `@Invalid`, which is
    /// QSettings' sentinel, so it reads back distinct from a list holding one "".
    pub fn set_string_list(&mut self, group: &str, key: &str, values: &[String]) {
        self.values
            .insert(settings_key(group, key), IniValue::List(values.to_vec()));
        self.dirty = true;
    }

    /// Removes one key. Removing an absent key is a no-op.
    pub fn remove_key(&mut self, group: &str, key: &str) {
        if self.values.remove(&settings_key(group, key)).is_some() {
            self.dirty = true;
        }
    }

    /// Removes a group and everything beneath it, as `QSettings::remove(group)`
    /// does. An empty group clears the whole store.
    pub fn remove_group(&mut self, group: &str) {
        let before = self.values.len();
        if group.is_empty() {
            self.values.clear();
        } else {
            let prefix = format!("{group}/");
            self.values
                .retain(|path, _| path != group && !path.starts_with(&prefix));
        }
        if self.values.len() != before {
            self.dirty = true;
        }
    }

    /// The direct child groups of `group`, sorted. Deeper groups collapse to their
    /// first segment, as `QSettings::childGroups` does.
    pub fn child_groups(&self, group: &str) -> Vec<String> {
        let prefix = if group.is_empty() {
            String::new()
        } else {
            format!("{group}/")
        };
        let mut seen = BTreeSet::new();
        for path in self.values.keys() {
            let Some(rest) = path.strip_prefix(&prefix) else {
                continue;
            };
            if let Some(slash) = rest.find('/') {
                seen.insert(rest[..slash].to_string());
            }
        }
        seen.into_iter().collect()
    }

    /// Marks the store dirty so the next [`Settings::sync`] writes even when no
    /// value changed. STS recovery uses this after renaming a malformed file so
    /// the already-loaded parseable map is rewritten cleanly.
    pub(super) fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Writes pending changes atomically. Refuses first when the file must not be
    /// overwritten, so a malformed or read-only file is never rebuilt.
    pub fn sync(&mut self) -> Status {
        let blocked = self.write_blocked();
        if blocked != Status::Written {
            return blocked;
        }
        if !self.dirty {
            return Status::Written;
        }
        let encoded = self.encode();
        if encoded_matches_disk(&self.path, self.on_disk.as_deref(), &encoded) {
            self.dirty = false;
            return Status::Written;
        }
        let write = if self.preserve_file_mode {
            write_atomic_preserve_mode(&self.path, &encoded)
        } else {
            write_atomic(&self.path, &encoded)
        };
        match write {
            Ok(()) => {
                self.on_disk = Some(encoded);
                self.dirty = false;
                Status::Written
            }
            Err(_) => Status::AccessError,
        }
    }

    /// Whether [`Settings::sync`] would refuse to write. A missing file is writable.
    pub fn write_blocked(&self) -> Status {
        match probe_ini(&self.path) {
            IniProbe::Malformed => return Status::FormatError,
            IniProbe::Unreadable => return Status::AccessError,
            IniProbe::Ok => {}
        }
        if file_not_writable(&self.path) {
            Status::AccessError
        } else {
            Status::Written
        }
    }

    fn text(&self, group: &str, key: &str) -> Option<&str> {
        match self.values.get(&settings_key(group, key))? {
            IniValue::Text(text) | IniValue::Variant(text) => Some(text.as_str()),
            IniValue::List(_) | IniValue::Invalid => None,
        }
    }

    /// Renders the whole store as QSettings INI. Sections and keys are sorted, so
    /// the same contents always produce the same bytes.
    pub(super) fn encode(&self) -> Vec<u8> {
        let mut sections: BTreeMap<String, Vec<(String, &IniValue)>> = BTreeMap::new();
        for (path, value) in &self.values {
            let (section, key) = split_section(path);
            sections
                .entry(section.to_string())
                .or_default()
                .push((key.to_string(), value));
        }

        let mut out = String::new();
        for (index, (section, mut entries)) in sections.into_iter().enumerate() {
            if index != 0 {
                out.push('\n');
            }
            out.push_str(&section_header(&section));
            out.push('\n');
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            for (key, value) in entries {
                out.push_str(&ini_escaped_key(&key));
                out.push('=');
                out.push_str(&encode_value(value));
                out.push('\n');
            }
        }
        out.into_bytes()
    }
}

fn settings_key(group: &str, key: &str) -> String {
    if group.is_empty() {
        key.to_string()
    } else {
        format!("{group}/{key}")
    }
}

/// Splits a full key path at its first `/`: the first segment is the `[section]`.
fn split_section(path: &str) -> (&str, &str) {
    match path.find('/') {
        Some(slash) => (&path[..slash], &path[slash + 1..]),
        None => ("", path),
    }
}

/// `[General]` for the top level. A real group named "general" is written as
/// `[%General]`, so it cannot be confused with the top level.
fn section_header(section: &str) -> String {
    let escaped = ini_escaped_key(section);
    if escaped.is_empty() {
        "[General]".to_string()
    } else if escaped.eq_ignore_ascii_case("general") {
        "[%General]".to_string()
    } else {
        format!("[{escaped}]")
    }
}

fn encode_value(value: &IniValue) -> String {
    match value {
        IniValue::List(items) => ini_escaped_string_list(items),
        IniValue::Invalid => "@Invalid()".to_string(),
        IniValue::Variant(encoded) => ini_escaped_string(encoded),
        // A string that starts with '@' gets one more '@', so it cannot be read
        // back as a QVariant such as `@Invalid` or `@Rect(...)`.
        IniValue::Text(text) if text.starts_with('@') => ini_escaped_string(&format!("@{text}")),
        IniValue::Text(text) => ini_escaped_string(text),
    }
}

/// Probes the raw bytes of `path`, the same fail-closed check the Qt client makes.
pub fn probe_ini(path: &Path) -> IniProbe {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return IniProbe::Ok,
        Err(_) => return IniProbe::Unreadable,
    };
    if metadata.is_dir() {
        return IniProbe::Unreadable;
    }
    match fs::read(path) {
        Ok(raw) if ini_bytes_are_malformed(&raw) => IniProbe::Malformed,
        Ok(_) => IniProbe::Ok,
        Err(_) => IniProbe::Unreadable,
    }
}

/// A file that exists and cannot accept a write. Read-only is checked first, then an
/// `O_WRONLY` open, which never truncates.
pub(crate) fn file_not_writable(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if metadata.permissions().readonly() {
        return true;
    }
    fs::OpenOptions::new().write(true).open(path).is_err()
}

#[path = "settings_ini.rs"]
mod settings_ini;

use settings_ini::{
    ini_bytes_are_malformed, ini_escaped_key, ini_escaped_string, ini_escaped_string_list,
    parse_settings_ini, parse_sts_ini,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::testdir::TempDir;

    fn settings_in(dir: &TempDir) -> PathBuf {
        dir.path().join("omairc").join("omairc.conf")
    }

    fn write_file(path: &Path, bytes: &[u8]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    #[test]
    fn string_round_trip() {
        let dir = TempDir::new("settings-strings");
        let path = settings_in(&dir);
        let mut settings = Settings::open(&path);
        let tricky = [
            ("spaces", "  leading and trailing  "),
            ("semi", "a;b"),
            ("equals", "a=b"),
            ("comma", "a,b"),
            ("hash", "a#b"),
            ("quote", r#"a"b"#),
            ("backslash", r"a\b"),
            ("newline", "a\nb"),
            ("tab", "a\tb"),
            ("unicode", "café-ünï-日本"),
            ("json", r#"[{"target":"a,b","ms":"1"}]"#),
            ("empty", ""),
        ];
        for (key, value) in tricky {
            settings.set_value("strings", key, value);
        }
        assert_eq!(settings.sync(), Status::Written);

        let reopened = Settings::open(&path);
        for (key, value) in tricky {
            assert_eq!(reopened.value("strings", key), value, "key {key}");
        }
        assert_eq!(reopened.value("strings", "absent"), "");
        assert_eq!(reopened.value("", "strings"), "");
    }

    #[test]
    fn scalar_round_trip_and_fallbacks() {
        let dir = TempDir::new("settings-scalars");
        let path = settings_in(&dir);
        let mut settings = Settings::open(&path);
        settings.set_bool("scalars", "on", true);
        settings.set_bool("scalars", "off", false);
        settings.set_uint("scalars", "port", 6697);
        settings.set_int("scalars", "icon", 3);
        assert_eq!(settings.sync(), Status::Written);

        let reopened = Settings::open(&path);
        assert!(reopened.bool_value("scalars", "on", false));
        assert!(!reopened.bool_value("scalars", "off", true));
        assert_eq!(reopened.uint_value("scalars", "port", 0), 6697);
        assert_eq!(reopened.int_value("scalars", "icon", 0), 3);

        assert!(reopened.bool_value("scalars", "missing", true));
        assert_eq!(reopened.uint_value("scalars", "on", 5), 5);
        assert_eq!(reopened.int_value("scalars", "missing", 9), 9);
    }

    #[test]
    fn bool_is_case_insensitive() {
        let dir = TempDir::new("settings-bools");
        let path = settings_in(&dir);
        let mut settings = Settings::open(&path);
        settings.set_value("b", "upper", "TRUE");
        settings.set_value("b", "digit", "1");
        settings.set_value("b", "nope", "yes");
        assert_eq!(settings.sync(), Status::Written);

        let reopened = Settings::open(&path);
        assert!(reopened.bool_value("b", "upper", false));
        assert!(reopened.bool_value("b", "digit", false));
        assert!(reopened.bool_value("b", "nope", true));
    }

    #[test]
    fn string_list_round_trip() {
        let dir = TempDir::new("settings-lists");
        let path = settings_in(&dir);
        let mut settings = Settings::open(&path);
        settings.set_string_list("lists", "autojoin", &["#a".into(), "#b".into()]);
        settings.set_string_list("lists", "commas", &["a,b".into(), r#"c"d"#.into()]);
        settings.set_string_list("lists", "single", &["freenode".into()]);
        settings.set_string_list("lists", "empty", &[]);
        settings.set_value("lists", "emptyString", "");
        assert_eq!(settings.sync(), Status::Written);

        let reopened = Settings::open(&path);
        let cases: [(&str, Vec<String>); 5] = [
            ("autojoin", vec!["#a".into(), "#b".into()]),
            ("commas", vec!["a,b".into(), r#"c"d"#.into()]),
            ("single", vec!["freenode".into()]),
            ("empty", vec![]),
            ("emptyString", vec![]),
        ];
        for (key, want) in cases {
            assert_eq!(reopened.string_list("lists", key), Some(want), "key {key}");
        }
        assert_eq!(reopened.string_list("lists", "absent"), None);
        assert_eq!(reopened.value("lists", "autojoin"), "");
    }

    #[test]
    fn nested_groups() {
        let dir = TempDir::new("settings-nested");
        let path = settings_in(&dir);
        let mut settings = Settings::open(&path);
        settings.set_value("networks/abc", "host", "chat.freenode.net");
        settings.set_value("networks/abc", "port", "6697");
        settings.set_value("networks/abc/deep", "host", "irc.deep.net");
        settings.set_value("networks/def", "host", "irc.oftc.net");
        settings.set_value("preferences", "directs", "true");
        settings.set_value("", "topLevel", "yes");

        assert_eq!(settings.child_groups(""), ["networks", "preferences"]);
        assert_eq!(settings.child_groups("networks"), ["abc", "def"]);
        assert_eq!(settings.child_groups("networks/abc"), ["deep"]);
        assert_eq!(settings.value("", "topLevel"), "yes");

        settings.remove_key("networks/abc", "port");
        assert_eq!(settings.value("networks/abc", "port"), "");
        assert_eq!(settings.value("networks/abc", "host"), "chat.freenode.net");
        settings.remove_key("networks/abc", "absent");

        settings.remove_group("networks/abc");
        assert_eq!(settings.value("networks/abc", "host"), "");
        assert_eq!(settings.value("networks/abc/deep", "host"), "");
        assert_eq!(settings.value("networks/def", "host"), "irc.oftc.net");

        assert_eq!(settings.sync(), Status::Written);
        let reopened = Settings::open(&path);
        assert_eq!(reopened.value("networks/def", "host"), "irc.oftc.net");
    }

    #[test]
    fn remove_group_with_empty_name_clears_everything() {
        let dir = TempDir::new("settings-clear");
        let mut settings = Settings::open(settings_in(&dir));
        settings.set_value("a", "x", "1");
        settings.set_value("", "top", "2");
        settings.remove_group("");
        assert_eq!(settings.value("a", "x"), "");
        assert_eq!(settings.value("", "top"), "");
    }

    #[test]
    fn sync_escaping_matches_qsettings() {
        let dir = TempDir::new("settings-escaping");
        let path = settings_in(&dir);
        let mut settings = Settings::open(&path);
        settings.set_value("tricky", "backslash", r"a\b");
        settings.set_value("tricky", "comma", "a,b");
        settings.set_value("tricky", "empty_str", "");
        settings.set_value("tricky", "equals", "a=b");
        settings.set_value("tricky", "hash", "a#b");
        settings.set_value("tricky", "lead", " x");
        settings.set_value("tricky", "newline", "a\nb");
        settings.set_value("tricky", "quote", r#"a"b"#);
        settings.set_value("tricky", "semi", "a;b");
        settings.set_value("tricky", "trail", "x ");
        settings.set_value("tricky", "unicode", "café日本");
        settings.set_string_list("tricky", "list", &["a,b".into(), r#"c"d"#.into()]);
        settings.set_string_list("tricky", "empty_list", &[]);
        assert_eq!(settings.sync(), Status::Written);

        let text = fs::read_to_string(&path).unwrap();
        // Expectations captured from Qt 6.8 QSettings(NativeFormat) output.
        let want_lines = [
            "[tricky]",
            r"backslash=a\\b",
            r#"comma="a,b""#,
            "empty_list=@Invalid()",
            "empty_str=",
            r#"equals="a=b""#,
            "hash=a#b",
            r#"lead=" x""#,
            r#"list="a,b", c\"d"#,
            r"newline=a\nb",
            r#"quote=a\"b"#,
            r#"semi="a;b""#,
            r#"trail="x ""#,
            "unicode=café日本",
        ];
        for line in want_lines {
            assert!(
                text.contains(&format!("{line}\n")),
                "missing QSettings line {line:?} in:\n{text}"
            );
        }

        let reopened = Settings::open(&path);
        assert_eq!(reopened.value("tricky", "empty_list"), "");
        assert_eq!(
            reopened.string_list("tricky", "list"),
            Some(vec!["a,b".into(), r#"c"d"#.into()])
        );
        assert_eq!(reopened.value("tricky", "unicode"), "café日本");
    }

    #[test]
    fn at_prefixed_text_is_not_read_as_a_sentinel() {
        let dir = TempDir::new("settings-at");
        let path = settings_in(&dir);
        let mut settings = Settings::open(&path);
        settings.set_value("at", "invalid", "@Invalid()");
        settings.set_value("at", "single", "@x");
        settings.set_value("at", "double", "@@x");
        assert_eq!(settings.sync(), Status::Written);

        let reopened = Settings::open(&path);
        assert_eq!(reopened.value("at", "invalid"), "@Invalid()");
        assert_eq!(reopened.value("at", "single"), "@x");
        assert_eq!(reopened.value("at", "double"), "@@x");
    }

    #[test]
    fn variant_values_are_written_verbatim_and_stay_so() {
        let dir = TempDir::new("settings-variant");
        let path = settings_in(&dir);
        let mut settings = Settings::open(&path);
        settings.set_variant("window", "geometry", "@Rect(1 2 3 4)");
        assert_eq!(settings.sync(), Status::Written);
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("geometry=@Rect(1 2 3 4)\n"), "{text}");

        // Reloading and saving another key must not add an '@' to the variant.
        let mut reopened = Settings::open(&path);
        assert_eq!(reopened.value("window", "geometry"), "@Rect(1 2 3 4)");
        reopened.set_bool("window", "maximized", true);
        assert_eq!(reopened.sync(), Status::Written);
        let again = fs::read_to_string(&path).unwrap();
        assert!(again.contains("geometry=@Rect(1 2 3 4)\n"), "{again}");
        assert!(!again.contains("@@"), "{again}");
    }

    #[test]
    fn reads_qsettings_fixtures() {
        // QSettings' canonical layout, and a hand-written nested section it also accepts.
        let fixtures = [
            (
                "canonical",
                concat!(
                    "[networks]\n",
                    "abc\\host=chat.freenode.net\n",
                    "abc\\port=6697\n",
                    "abc\\tls=true\n",
                    "abc\\autojoin=#a, #b\n",
                    r#"abc\playback="[{\"target\":\"a,b\",\"ms\":\"1\"}]""#,
                    "\n\n[preferences]\ndirects=true\nnetworkOrder=freenode\n",
                ),
            ),
            (
                "nested-section",
                concat!(
                    "[networks/abc]\n",
                    "host=chat.freenode.net\nport=6697\ntls=true\nautojoin=#a, #b\n",
                    r#"playback="[{\"target\":\"a,b\",\"ms\":\"1\"}]""#,
                    "\n\n[preferences]\ndirects=true\nnetworkOrder=freenode\n",
                ),
            ),
        ];

        for (name, fixture) in fixtures {
            let dir = TempDir::new("settings-fixture");
            let path = settings_in(&dir);
            write_file(&path, fixture.as_bytes());
            assert_eq!(probe_ini(&path), IniProbe::Ok, "{name}");

            let settings = Settings::open(&path);
            assert_eq!(
                settings.value("networks/abc", "host"),
                "chat.freenode.net",
                "{name}"
            );
            assert_eq!(
                settings.uint_value("networks/abc", "port", 0),
                6697,
                "{name}"
            );
            assert!(settings.bool_value("networks/abc", "tls", false), "{name}");
            assert_eq!(
                settings.string_list("networks/abc", "autojoin"),
                Some(vec!["#a".into(), "#b".into()]),
                "{name}"
            );
            assert_eq!(
                settings.value("networks/abc", "playback"),
                r#"[{"target":"a,b","ms":"1"}]"#,
                "{name}"
            );
            assert!(
                settings.bool_value("preferences", "directs", false),
                "{name}"
            );
            assert_eq!(
                settings.string_list("preferences", "networkOrder"),
                Some(vec!["freenode".into()]),
                "{name}"
            );
            assert_eq!(
                settings.child_groups(""),
                ["networks", "preferences"],
                "{name}"
            );
            assert_eq!(settings.child_groups("networks"), ["abc"], "{name}");
        }
    }

    #[test]
    fn probe_classifies_files() {
        let dir = TempDir::new("settings-probe");
        assert_eq!(probe_ini(&dir.path().join("missing.conf")), IniProbe::Ok);

        let valid = dir.path().join("valid.conf");
        write_file(&valid, b"[networks]\nabc\\host=example\n");
        assert_eq!(probe_ini(&valid), IniProbe::Ok);

        let comment = dir.path().join("comment.conf");
        write_file(&comment, b";just a comment\n");
        assert_eq!(probe_ini(&comment), IniProbe::Ok);

        let hash_comment = dir.path().join("hash-comment.conf");
        write_file(&hash_comment, b"# keep me\n   # note\n");
        assert_eq!(probe_ini(&hash_comment), IniProbe::Ok);

        let unclosed = dir.path().join("unclosed.conf");
        write_file(&unclosed, b"[unclosed\nkey=x\n");
        assert_eq!(probe_ini(&unclosed), IniProbe::Malformed);

        let no_equals = dir.path().join("noequals.conf");
        write_file(&no_equals, b"[group]\nkey\n");
        assert_eq!(probe_ini(&no_equals), IniProbe::Malformed);
    }

    #[cfg(unix)]
    #[test]
    fn probe_reports_unreadable_files() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TempDir::new("settings-unreadable");
        let path = dir.path().join("unreadable.conf");
        write_file(&path, b"[networks]\nabc\\host=example\n");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).unwrap();
        if fs::read(&path).is_ok() {
            // Running as root: the file stays readable, so there is nothing to probe.
            return;
        }
        assert_eq!(probe_ini(&path), IniProbe::Unreadable);
    }

    #[cfg(unix)]
    #[test]
    fn hash_full_line_comments_load_and_save() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TempDir::new("settings-hash-comment-236b");
        let path = settings_in(&dir);
        let original = concat!(
            "# keep me\n",
            "   # note\n",
            "[preferences]\n",
            "directs=false\n",
            "channel=#omarchy\n",
            "color=#ff0000\n",
        );
        write_file(&path, original.as_bytes());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();

        assert_eq!(probe_ini(&path), IniProbe::Ok);
        let mut settings = Settings::open(&path);
        assert_eq!(settings.write_blocked(), Status::Written);
        assert!(!settings.bool_value("preferences", "directs", true));
        assert_eq!(settings.value("preferences", "channel"), "#omarchy");
        assert_eq!(settings.value("preferences", "color"), "#ff0000");

        settings.set_bool("preferences", "directs", true);
        assert_eq!(settings.sync(), Status::Written);
        assert!(settings.bool_value("preferences", "directs", false));

        let after = fs::read_to_string(&path).unwrap();
        assert!(after.contains("directs=true\n"), "{after}");
        assert!(after.contains("channel=#omarchy\n"), "{after}");
        assert!(after.contains("color=#ff0000\n"), "{after}");
        assert!(
            !after.contains("# keep me") && !after.contains("# note"),
            "full-line # comments are not preserved on write: {after}"
        );
    }

    #[test]
    fn hash_comment_iterator_skips_special_bytes_in_comment() {
        let dir = TempDir::new("settings-hash-iterator-236b");
        let path = settings_in(&dir);
        let body = concat!(
            "# don't \"quote\n",
            "# a=b\n",
            "# x;y\n",
            "[preferences]\n",
            "directs=false\n",
            "note=ok\n",
        );
        write_file(&path, body.as_bytes());
        assert_eq!(probe_ini(&path), IniProbe::Ok);

        let settings = Settings::open(&path);
        assert!(!settings.bool_value("preferences", "directs", true));
        assert_eq!(settings.value("preferences", "note"), "ok");
    }

    #[test]
    fn sync_refuses_malformed_file() {
        let dir = TempDir::new("settings-malformed");
        let path = settings_in(&dir);
        let corrupt = b"[networks]\nabc\\host\n";
        write_file(&path, corrupt);

        let mut settings = Settings::open(&path);
        settings.set_value("networks/abc", "host", "irc.changed.example");
        assert_eq!(settings.sync(), Status::FormatError);
        assert_eq!(fs::read(&path).unwrap(), corrupt);
    }

    #[test]
    // Restores writability after the read-only probe; the false bit is intentional cleanup.
    #[allow(clippy::permissions_set_readonly_false)]
    fn sync_refuses_read_only_file() {
        let dir = TempDir::new("settings-readonly");
        let path = settings_in(&dir);
        let original = b"[networks]\nabc\\host=chat.freenode.net\n";
        write_file(&path, original);
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&path, permissions.clone()).unwrap();

        if fs::OpenOptions::new().write(true).open(&path).is_ok() {
            // Running as root: the read-only bit does not stop the write.
            permissions.set_readonly(false);
            fs::set_permissions(&path, permissions).unwrap();
            return;
        }

        let mut settings = Settings::open(&path);
        settings.set_value("networks/abc", "host", "irc.changed.example");
        assert_eq!(settings.sync(), Status::AccessError);
        assert_eq!(fs::read(&path).unwrap(), original);

        permissions.set_readonly(false);
        fs::set_permissions(&path, permissions).unwrap();
    }

    #[cfg(unix)]
    #[test]
    // Restores writability after the read-only probe; the false bit is intentional cleanup.
    #[allow(clippy::permissions_set_readonly_false)]
    fn sync_refuses_read_only_malformed_file_with_format_error() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TempDir::new("settings-readonly-malformed");
        let path = settings_in(&dir);
        let corrupt = b"[networks]\nabc\\host\n";
        write_file(&path, corrupt);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).unwrap();

        if fs::OpenOptions::new().write(true).open(&path).is_ok() {
            fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
            return;
        }

        let mut settings = Settings::open(&path);
        settings.set_value("networks/abc", "host", "irc.changed.example");
        assert_eq!(settings.sync(), Status::FormatError);
        assert_eq!(fs::read(&path).unwrap(), corrupt);

        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn sync_preserves_existing_file_mode() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TempDir::new("settings-preserve-mode");
        let path = settings_in(&dir);
        write_file(&path, b"[preferences]\ndirects=false\n");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();

        let mut settings = Settings::open(&path);
        settings.set_bool("preferences", "directs", true);
        assert_eq!(settings.sync(), Status::Written);

        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o644);
        assert!(settings.bool_value("preferences", "directs", false));
    }

    #[cfg(unix)]
    mod symlink_sync_3dba {
        use super::*;
        use std::os::unix::fs::{symlink, PermissionsExt};

        #[test]
        fn sync_through_symlink_updates_target_and_keeps_link() {
            let dir = TempDir::new("settings-symlink-3dba");
            let cfg_dir = dir.path().join("cfg").join("omairc");
            fs::create_dir_all(&cfg_dir).unwrap();
            let target = cfg_dir.join("real").join("omairc.conf");
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            write_file(&target, b"[preferences]\ndirects=false\n");
            fs::set_permissions(&target, fs::Permissions::from_mode(0o644)).unwrap();
            let link = cfg_dir.join("omairc.conf");
            symlink("real/omairc.conf", &link).unwrap();

            let mut settings = Settings::open(&link);
            settings.set_bool("preferences", "directs", true);
            assert_eq!(settings.sync(), Status::Written);

            assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
            assert_eq!(fs::read_link(&link).unwrap(), Path::new("real/omairc.conf"));
            assert_eq!(
                fs::read(&target).unwrap(),
                b"[preferences]\ndirects=true\n"
            );
            let mode = fs::metadata(&target).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o644);
        }

        #[test]
        fn sync_refuses_read_only_target_behind_symlink() {
            let dir = TempDir::new("settings-symlink-readonly-3dba");
            let cfg_dir = dir.path().join("cfg").join("omairc");
            fs::create_dir_all(&cfg_dir).unwrap();
            let target = cfg_dir.join("real").join("omairc.conf");
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            let original = b"[preferences]\ndirects=false\n";
            write_file(&target, original);
            fs::set_permissions(&target, fs::Permissions::from_mode(0o444)).unwrap();
            let link = cfg_dir.join("omairc.conf");
            symlink("real/omairc.conf", &link).unwrap();

            if fs::OpenOptions::new().write(true).open(&link).is_ok() {
                fs::set_permissions(&target, fs::Permissions::from_mode(0o644)).unwrap();
                return;
            }

            let mut settings = Settings::open(&link);
            settings.set_bool("preferences", "directs", true);
            assert_eq!(settings.sync(), Status::AccessError);
            assert_eq!(fs::read(&target).unwrap(), original);
            assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());

            fs::set_permissions(&target, fs::Permissions::from_mode(0o644)).unwrap();
        }

        #[test]
        fn sync_errors_when_dangling_target_directory_missing() {
            let dir = TempDir::new("settings-symlink-missing-dir-3dba");
            let cfg_dir = dir.path().join("cfg").join("omairc");
            fs::create_dir_all(&cfg_dir).unwrap();
            let link = cfg_dir.join("omairc.conf");
            symlink("missing/deep/omairc.conf", &link).unwrap();
            let missing_dir = cfg_dir.join("missing");

            let mut settings = Settings::open(&link);
            settings.set_bool("preferences", "directs", true);
            assert_eq!(settings.sync(), Status::AccessError);
            assert!(!missing_dir.exists());
        }

        #[test]
        fn sync_through_symlinked_data_directory() {
            let dir = TempDir::new("settings-symlink-data-dir-3dba");
            let data = dir.path().join("data");
            fs::create_dir_all(&data).unwrap();
            let target = data.join("omairc.conf");
            write_file(&target, b"[preferences]\ndirects=false\n");
            let dotfiles = dir.path().join("dotfiles");
            symlink(&data, &dotfiles).unwrap();
            fs::create_dir_all(dir.path().join("cfg")).unwrap();
            let link = dir.path().join("cfg").join("omairc.conf");
            symlink("../dotfiles/omairc.conf", &link).unwrap();

            let mut settings = Settings::open(&link);
            settings.set_bool("preferences", "directs", true);
            assert_eq!(settings.sync(), Status::Written);
            assert_eq!(
                fs::read(&target).unwrap(),
                b"[preferences]\ndirects=true\n"
            );
        }
    }

    #[test]
    fn sync_creates_missing_file_owner_only() {
        let dir = TempDir::new("settings-create");
        let path = settings_in(&dir);
        let mut settings = Settings::open(&path);
        settings.set_value("preferences", "directs", "true");
        assert_eq!(settings.sync(), Status::Written);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
        assert!(Settings::open(&path).bool_value("preferences", "directs", false));
    }

    #[test]
    fn sync_without_changes_does_not_create_file() {
        let dir = TempDir::new("settings-nochange");
        let path = settings_in(&dir);
        let mut settings = Settings::open(&path);
        assert_eq!(settings.sync(), Status::Written);
        assert!(!path.exists());
    }

    fn mark_dirty_for_test(settings: &mut Settings) {
        settings.set_value("_sync_test", "noop", "1");
        settings.remove_key("_sync_test", "noop");
    }

    #[test]
    fn sync_on_empty_file_leaves_disk_unchanged() {
        let dir = TempDir::new("settings-empty-file");
        let path = settings_in(&dir);
        write_file(&path, b"");
        let mut settings = Settings::open(&path);
        assert!(sync_leaves_disk_unchanged(&path, &mut settings, mark_dirty_for_test));
    }

    #[test]
    fn sync_on_comment_only_file_leaves_disk_unchanged() {
        let dir = TempDir::new("settings-comment-file");
        let path = settings_in(&dir);
        write_file(&path, b"; keep this comment\n");
        let mut settings = Settings::open(&path);
        assert!(sync_leaves_disk_unchanged(
            &path,
            &mut settings,
            mark_dirty_for_test,
        ));
        assert_eq!(
            fs::read(&path).unwrap(),
            b"; keep this comment\n",
            "comment-only bytes must survive a no-op sync"
        );
    }

    #[test]
    fn sync_repeat_assignment_preserves_file_bytes_and_mtime() {
        use std::thread;
        use std::time::Duration;

        let dir = TempDir::new("settings-idempotent");
        let path = settings_in(&dir);
        write_file(&path, b"[preferences]\ndirects=true\n");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        }

        let before_bytes = fs::read(&path).unwrap();
        let before_mtime = fs::metadata(&path).unwrap().modified().unwrap();
        thread::sleep(Duration::from_millis(1100));

        let mut settings = Settings::open(&path);
        settings.set_bool("preferences", "directs", true);
        assert_eq!(settings.sync(), Status::Written);
        assert_eq!(fs::read(&path).unwrap(), before_bytes);
        assert_eq!(
            fs::metadata(&path).unwrap().modified().unwrap(),
            before_mtime
        );

        settings.set_bool("preferences", "directs", false);
        assert_eq!(settings.sync(), Status::Written);
        assert_ne!(fs::read(&path).unwrap(), before_bytes);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o644);
        }
        assert!(!settings.bool_value("preferences", "directs", true));
    }

    fn sync_leaves_disk_unchanged(path: &Path, settings: &mut Settings, mutate: impl FnOnce(&mut Settings)) -> bool {
        use std::thread;
        use std::time::Duration;

        thread::sleep(Duration::from_millis(1100));
        let before_bytes = fs::read(path).expect("read");
        let before_mtime = fs::metadata(path).expect("stat").modified().expect("mtime");
        mutate(settings);
        assert_eq!(settings.sync(), Status::Written);
        let bytes_unchanged = fs::read(path).expect("read after") == before_bytes;
        let mtime_unchanged =
            fs::metadata(path).expect("stat after").modified().expect("mtime") == before_mtime;
        bytes_unchanged && mtime_unchanged
    }

    #[test]
    fn sync_string_list_after_reopen_skips_unchanged_bytes() {
        let dir = TempDir::new("settings-list-idempotent");
        let path = settings_in(&dir);
        write_file(
            &path,
            b"[preferences]\nnetworkOrder=@Invalid()\ncollapsed=@Invalid()\n",
        );
        let mut settings = Settings::open(&path);
        assert!(sync_leaves_disk_unchanged(&path, &mut settings, |settings| {
            settings.set_string_list("preferences", "networkOrder", &[]);
            settings.set_string_list("preferences", "collapsed", &[]);
        }));

        write_file(&path, b"[preferences]\nnetworkOrder=freenode\n");
        let mut settings = Settings::open(&path);
        assert!(sync_leaves_disk_unchanged(&path, &mut settings, |settings| {
            settings.set_string_list("preferences", "networkOrder", &["freenode".into()]);
        }));
    }

    #[test]
    fn encode_is_deterministic() {
        let dir = TempDir::new("settings-deterministic");
        let mut first = Settings::open(settings_in(&dir));
        first.set_value("z", "b", "2");
        first.set_value("z", "a", "1");
        first.set_value("a", "y", "2");
        first.set_value("a", "x", "1");

        let mut second = Settings::open(dir.path().join("second.conf"));
        second.set_value("a", "x", "1");
        second.set_value("a", "y", "2");
        second.set_value("z", "a", "1");
        second.set_value("z", "b", "2");

        assert_eq!(first.encode(), second.encode());
    }

    #[test]
    fn escaping_helpers() {
        let cases = [
            ("a;b", r#""a;b""#),
            ("a,b", r#""a,b""#),
            ("a=b", r#""a=b""#),
            (r"a\b", r"a\\b"),
            (r#"a"b"#, r#"a\"b"#),
            ("a\nb", r"a\nb"),
            ("a\tb", r"a\tb"),
            (" x", r#"" x""#),
            ("x ", r#""x ""#),
            ("a#b", "a#b"),
            ("", ""),
            ("café", "café"),
            ("trig\u{0}", r"trig\0"),
        ];
        for (input, want) in cases {
            assert_eq!(ini_escaped_string(input), want, "input {input:?}");
        }
        assert_eq!(ini_escaped_string_list(&[]), "@Invalid()");
        assert_eq!(ini_escaped_string_list(&["a".into(), "b".into()]), "a, b");
    }

    #[test]
    fn key_escaping_round_trips_slashes_and_unicode() {
        let key = "abc/dé日";
        assert_eq!(settings_ini::ini_unescaped_key(ini_escaped_key(key).as_bytes()), key);
        assert_eq!(ini_escaped_key("a/b"), r"a\b");
        assert_eq!(settings_ini::ini_unescaped_key(br"a\b"), "a/b");
    }
}
