//! One-shot import of Qt's Windows registry settings into the product INI.
//! On Windows, Qt's `QSettings::NativeFormat` stores main settings under
//! `HKCU\Software\omairc\omairc` (subkey = group). Product mode copies that data
//! once into `%LOCALAPPDATA%\omairc\omairc.conf`. The TUI keeps its settings in
//! that same file, so it can already exist: the import then merges. A value the
//! file already holds wins, a string list gains the registry's missing items
//! and everything else is added. The `migration/qtRegistryImported` marker
//! records that the import ran, so a network removed later does not come back.
//! The registry is never modified, so a Qt client can still roll back.
//! Product startup calls [`import_if_needed`] after the file log is installed
//! (`migrate::run_at_startup`). Tests drive the same mapping through
//! [`RegistryReader`] with an in-memory tree.

use std::collections::BTreeMap;
use std::path::Path;

use super::settings::{Settings, Status};

/// Registry path under `HKEY_CURRENT_USER` for the installed Qt client's settings.
pub const QT_REGISTRY_SUBKEY: &str = r"Software\omairc\omairc";

/// The group and key of the marker a finished import leaves in the INI.
pub const MIGRATION_GROUP: &str = "migration";
pub const REGISTRY_IMPORTED_KEY: &str = "qtRegistryImported";

/// QMetaType id of `QStringList`, the type a `@Variant(...)` string list carries.
const QSTRINGLIST_TYPE_ID: u32 = 11;

/// Outcome of [`import_if_needed`] or [`import_from_reader`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportOutcome {
    /// Not product+Windows, the import already ran, or nothing to do.
    Skipped,
    /// The Qt registry key is absent.
    NoRegistry,
    /// Wrote the INI from the registry (or a fake reader). `values` counts the
    /// values added or merged; ones the INI already held are not counted.
    Imported { values: usize },
    /// Could not read the registry or write the INI.
    Failed(&'static str),
}

/// A value as Qt's Windows registry backend stores it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegistryValue {
    /// `REG_SZ` / `REG_EXPAND_SZ` (and UTF-16 `REG_BINARY` text).
    Sz(String),
    Dword(u32),
    Qword(u64),
    MultiSz(Vec<String>),
}

/// Contents of one registry key (one QSettings group).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RegistryKeyContents {
    pub values: Vec<(String, RegistryValue)>,
    pub subkeys: Vec<String>,
}

/// Read-only walk of a registry key tree. The Windows product build opens
/// [`QT_REGISTRY_SUBKEY`]; tests supply an in-memory fake.
pub trait RegistryReader {
    /// Whether the Qt settings root key exists.
    fn root_exists(&self) -> bool;

    /// Values and direct child subkeys of `group` (`""` is the root). Subkey and
    /// value names are QSettings path segments (forward slashes), already
    /// unescaped from Qt's registry encoding.
    fn list(&self, group: &str) -> Result<RegistryKeyContents, ()>;
}

/// Whether the INI at `settings_path` records a finished import.
pub fn already_imported(settings_path: &Path) -> bool {
    settings_path.is_file()
        && Settings::open(settings_path).bool_value(MIGRATION_GROUP, REGISTRY_IMPORTED_KEY, false)
}

/// Product Windows startup entry. Called from `migrate::run_at_startup` once the
/// file log is installed. Elsewhere (non-Windows, `dev` feature, or an import that
/// already ran) this is a no-op.
pub fn import_if_needed(settings_path: &Path) -> ImportOutcome {
    if already_imported(settings_path) {
        return ImportOutcome::Skipped;
    }
    #[cfg(all(windows, not(feature = "dev")))]
    {
        import_from_reader(&windows_reader::WinRegistryReader, settings_path)
    }
    #[cfg(any(not(windows), feature = "dev"))]
    {
        let _ = settings_path;
        ImportOutcome::Skipped
    }
}

/// Walks `reader` into the [`Settings`] file at `settings_path`, merging into what
/// the file already holds, and records the marker. Leaves the registry (or fake
/// tree) untouched. A malformed or read-only file is left alone and reported.
pub fn import_from_reader(reader: &dyn RegistryReader, settings_path: &Path) -> ImportOutcome {
    if !reader.root_exists() {
        return ImportOutcome::NoRegistry;
    }
    let mut settings = Settings::open(settings_path);
    let mut values = 0usize;
    if walk_into(reader, "", &mut settings, &mut values).is_err() {
        return ImportOutcome::Failed("could not read the Qt registry settings");
    }
    settings.set_bool(MIGRATION_GROUP, REGISTRY_IMPORTED_KEY, true);
    match settings.sync() {
        Status::Written => {
            log::warn!(
                "Imported {values} Qt registry setting(s) into {}",
                settings_path.display()
            );
            ImportOutcome::Imported { values }
        }
        Status::FormatError => ImportOutcome::Failed("settings file is malformed; left unchanged"),
        Status::AccessError => ImportOutcome::Failed("could not write the settings file"),
        Status::Absent => ImportOutcome::Failed("settings write reported absent"),
    }
}

fn walk_into(
    reader: &dyn RegistryReader,
    group: &str,
    settings: &mut Settings,
    values: &mut usize,
) -> Result<(), ()> {
    let contents = reader.list(group)?;
    for (key, value) in contents.values {
        if key.is_empty() {
            continue;
        }
        if merge_value(settings, group, &key, &value) {
            *values += 1;
        }
    }
    for subkey in contents.subkeys {
        if subkey.is_empty() {
            continue;
        }
        let child = if group.is_empty() {
            subkey
        } else {
            format!("{group}/{subkey}")
        };
        walk_into(reader, &child, settings, values)?;
    }
    Ok(())
}

/// Adds one registry value unless the INI already holds that key. A string list
/// the INI already holds keeps its order and gains the registry's missing items.
/// Returns whether the INI changed.
fn merge_value(settings: &mut Settings, group: &str, key: &str, value: &RegistryValue) -> bool {
    let value = qt_list_value(value);
    if !settings.contains(group, key) {
        apply_value(settings, group, key, &value);
        return true;
    }
    let RegistryValue::MultiSz(items) = &value else {
        return false;
    };
    let mut merged = settings.string_list(group, key).unwrap_or_default();
    let before = merged.len();
    for item in items {
        if !merged.contains(item) {
            merged.push(item.clone());
        }
    }
    if merged.len() == before {
        return false;
    }
    settings.set_string_list(group, key, &merged);
    true
}

/// The value as Qt reads it back when it is a string list. Qt writes a list item
/// that starts with `@` as `@@…`, and a list holding an empty item as UTF-16
/// `@Variant(...)` text in a `REG_BINARY`. Other values pass through.
fn qt_list_value(value: &RegistryValue) -> RegistryValue {
    match value {
        RegistryValue::MultiSz(items) => RegistryValue::MultiSz(
            items
                .iter()
                .map(|item| item.strip_prefix('@').filter(|rest| rest.starts_with('@')).unwrap_or(item))
                .map(str::to_owned)
                .collect(),
        ),
        RegistryValue::Sz(text) => match decode_variant_string_list(text) {
            Some(items) => RegistryValue::MultiSz(items),
            None => value.clone(),
        },
        other => other.clone(),
    }
}

/// Decodes `@Variant(...)` text holding a `QStringList`, as `QSettings` writes it:
/// each byte of a `QDataStream` (version `Qt_4_0`) is one character. The stream is
/// the type id, the item count, then each `QString` as a byte length (`0xFFFFFFFF`
/// for a null string) and UTF-16BE. `None` for any other variant.
fn decode_variant_string_list(text: &str) -> Option<Vec<String>> {
    let body = text.strip_prefix("@Variant(")?.strip_suffix(')')?;
    let bytes = body
        .chars()
        .map(|ch| u8::try_from(u32::from(ch)).ok())
        .collect::<Option<Vec<u8>>>()?;
    let mut stream = bytes.as_slice();
    fn next_u32(stream: &mut &[u8]) -> Option<u32> {
        let (head, rest) = stream.split_first_chunk::<4>()?;
        *stream = rest;
        Some(u32::from_be_bytes(*head))
    }
    if next_u32(&mut stream)? != QSTRINGLIST_TYPE_ID {
        return None;
    }
    let count = next_u32(&mut stream)?;
    let mut items = Vec::new();
    for _ in 0..count {
        let length = next_u32(&mut stream)?;
        if length == u32::MAX {
            items.push(String::new());
            continue;
        }
        let length = usize::try_from(length).ok()?;
        if length % 2 != 0 || stream.len() < length {
            return None;
        }
        let (raw, rest) = stream.split_at(length);
        stream = rest;
        let units: Vec<u16> = raw
            .chunks_exact(2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect();
        items.push(String::from_utf16(&units).ok()?);
    }
    stream.is_empty().then_some(items)
}

fn apply_value(settings: &mut Settings, group: &str, key: &str, value: &RegistryValue) {
    match value {
        RegistryValue::Dword(n) => {
            // Qt stores bool/int/uint as REG_DWORD and reads them back as signed int.
            let signed = i32::from_le_bytes(n.to_le_bytes());
            settings.set_int(group, key, i64::from(signed));
        }
        RegistryValue::Qword(n) => {
            let signed = i64::from_le_bytes(n.to_le_bytes());
            settings.set_int(group, key, signed);
        }
        RegistryValue::MultiSz(items) => {
            settings.set_string_list(group, key, items);
        }
        RegistryValue::Sz(text) => apply_sz(settings, group, key, text),
    }
}

/// Maps a REG_SZ (or UTF-16 REG_BINARY) string the way Qt's `stringToVariant`
/// INI writer would: `@Invalid`, `@@…` plain text, `@Rect` / `@ByteArray`
/// other `@…` variants verbatim, else plain text.
fn apply_sz(settings: &mut Settings, group: &str, key: &str, text: &str) {
    if text == "@Invalid()" {
        settings.set_string_list(group, key, &[]);
    } else if let Some(rest) = text.strip_prefix("@@") {
        settings.set_value(group, key, &format!("@{rest}"));
    } else if text.starts_with('@') {
        settings.set_variant(group, key, text);
    } else {
        settings.set_value(group, key, text);
    }
}

/// Qt swaps `/` and `\` when writing registry key and value names.
fn unescape_registry_name(name: &str) -> String {
    name.chars()
        .map(|ch| match ch {
            '/' => '\\',
            '\\' => '/',
            other => other,
        })
        .collect()
}

/// In-memory registry tree for tests. Keys are QSettings group paths (`""`
/// `"networks/freenode"`); value names are unescaped.
#[derive(Clone, Debug, Default)]
pub struct FakeRegistry {
    nodes: BTreeMap<String, BTreeMap<String, RegistryValue>>,
}

impl FakeRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts `value` at `group`/`key`, creating the group node as needed.
    pub fn insert(&mut self, group: &str, key: &str, value: RegistryValue) {
        self.nodes
            .entry(group.to_string())
            .or_default()
            .insert(key.to_string(), value);
    }
}

impl RegistryReader for FakeRegistry {
    fn root_exists(&self) -> bool {
        // A Qt install that wrote anything has at least the root key; an empty
        // fake still "exists" so tests can assert NoRegistry separately.
        true
    }

    fn list(&self, group: &str) -> Result<RegistryKeyContents, ()> {
        let values = self
            .nodes
            .get(group)
            .map(|map| {
                map.iter()
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let prefix = if group.is_empty() {
            String::new()
        } else {
            format!("{group}/")
        };
        let mut subkeys = Vec::new();
        for path in self.nodes.keys() {
            let Some(rest) = path.strip_prefix(&prefix) else {
                continue;
            };
            if rest.is_empty() {
                continue;
            }
            let child = match rest.find('/') {
                Some(slash) => &rest[..slash],
                None => rest,
            };
            if !child.is_empty() && !subkeys.iter().any(|seen: &String| seen == child) {
                subkeys.push(child.to_string());
            }
        }
        subkeys.sort();
        Ok(RegistryKeyContents { values, subkeys })
    }
}

#[cfg(all(windows, not(feature = "dev")))]
mod windows_reader {
    use super::{
        unescape_registry_name, RegistryKeyContents, RegistryReader, RegistryValue,
        QT_REGISTRY_SUBKEY,
    };
    use std::os::windows::ffi::OsStrExt;
    use std::ffi::OsStr;

    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{ERROR_MORE_DATA, ERROR_NO_MORE_ITEMS, ERROR_SUCCESS};
    use windows::Win32::System::Registry::{
        RegCloseKey, RegEnumKeyExW, RegEnumValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER,
        KEY_READ, REG_BINARY, REG_DWORD, REG_DWORD_BIG_ENDIAN, REG_EXPAND_SZ, REG_MULTI_SZ,
        REG_QWORD, REG_SZ, REG_VALUE_TYPE,
    };

    pub(super) struct WinRegistryReader;

    impl RegistryReader for WinRegistryReader {
        fn root_exists(&self) -> bool {
            open_key("").is_ok()
        }

        fn list(&self, group: &str) -> Result<RegistryKeyContents, ()> {
            let key = open_key(group)?;
            let result = (|| {
                let values = enum_values(key)?;
                let subkeys = enum_subkeys(key)?;
                Ok(RegistryKeyContents { values, subkeys })
            })();
            let _ = unsafe { RegCloseKey(key) };
            result
        }
    }

    fn open_key(group: &str) -> Result<HKEY, ()> {
        let path = if group.is_empty() {
            QT_REGISTRY_SUBKEY.to_string()
        } else {
            // Walked names are unescaped; join with '\' for RegOpenKeyEx.
            // Re-escape each segment so a literal '/' in a name becomes '\'.
            let escaped: Vec<String> = group
                .split('/')
                .map(|segment| {
                    segment
                        .chars()
                        .map(|ch| match ch {
                            '/' => '\\',
                            '\\' => '/',
                            other => other,
                        })
                        .collect()
                })
                .collect();
            format!(r"{QT_REGISTRY_SUBKEY}\{}", escaped.join(r"\"))
        };
        let wide = wide(&path);
        let mut handle = HKEY::default();
        let status = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(wide.as_ptr()),
                None,
                KEY_READ,
                &mut handle,
            )
        };
        if status == ERROR_SUCCESS {
            Ok(handle)
        } else {
            Err(())
        }
    }

    fn enum_subkeys(key: HKEY) -> Result<Vec<String>, ()> {
        let mut names = Vec::new();
        let mut index = 0u32;
        loop {
            let mut name = vec![0u16; 256];
            let mut name_len = name.len() as u32;
            let status = unsafe {
                RegEnumKeyExW(
                    key,
                    index,
                    Some(windows::core::PWSTR(name.as_mut_ptr())),
                    &mut name_len,
                    None,
                    None,
                    None,
                    None,
                )
            };
            if status == ERROR_NO_MORE_ITEMS {
                break;
            }
            if status != ERROR_SUCCESS {
                return Err(());
            }
            let raw = String::from_utf16_lossy(&name[..name_len as usize]);
            names.push(unescape_registry_name(&raw));
            index += 1;
        }
        names.sort();
        Ok(names)
    }

    fn enum_values(key: HKEY) -> Result<Vec<(String, RegistryValue)>, ()> {
        let mut values = Vec::new();
        let mut index = 0u32;
        loop {
            let mut name = vec![0u16; 16_384];
            let mut name_len = name.len() as u32;
            let mut ty = 0u32;
            let mut data_len = 0u32;
            let status = unsafe {
                RegEnumValueW(
                    key,
                    index,
                    Some(windows::core::PWSTR(name.as_mut_ptr())),
                    &mut name_len,
                    None,
                    Some(&mut ty),
                    None,
                    Some(&mut data_len),
                )
            };
            if status == ERROR_NO_MORE_ITEMS {
                break;
            }
            // NULL data buffer: SUCCESS or MORE_DATA both report the byte size.
            if status != ERROR_SUCCESS && status != ERROR_MORE_DATA {
                return Err(());
            }
            let mut data = vec![0u8; data_len as usize];
            let mut name_len2 = name.len() as u32;
            let mut ty2 = 0u32;
            let mut data_len2 = data_len;
            let status = unsafe {
                RegEnumValueW(
                    key,
                    index,
                    Some(windows::core::PWSTR(name.as_mut_ptr())),
                    &mut name_len2,
                    None,
                    Some(&mut ty2),
                    Some(data.as_mut_ptr()),
                    Some(&mut data_len2),
                )
            };
            if status != ERROR_SUCCESS {
                return Err(());
            }
            data.truncate(data_len2 as usize);
            let raw_name = String::from_utf16_lossy(&name[..name_len2 as usize]);
            let value_name = unescape_registry_name(&raw_name);
            if let Some(parsed) = decode_value(REG_VALUE_TYPE(ty2), &data) {
                values.push((value_name, parsed));
            }
            index += 1;
        }
        Ok(values)
    }

    fn decode_value(ty: REG_VALUE_TYPE, data: &[u8]) -> Option<RegistryValue> {
        match ty {
            REG_SZ | REG_EXPAND_SZ => Some(RegistryValue::Sz(utf16_zstring(data))),
            REG_MULTI_SZ => Some(RegistryValue::MultiSz(utf16_multi_sz(data))),
            REG_DWORD | REG_DWORD_BIG_ENDIAN if data.len() >= 4 => {
                let mut bytes = [0u8; 4];
                bytes.copy_from_slice(&data[..4]);
                let n = if ty == REG_DWORD_BIG_ENDIAN {
                    u32::from_be_bytes(bytes)
                } else {
                    u32::from_le_bytes(bytes)
                };
                Some(RegistryValue::Dword(n))
            }
            REG_QWORD if data.len() >= 8 => {
                let mut bytes = [0u8; 8];
                bytes.copy_from_slice(&data[..8]);
                Some(RegistryValue::Qword(u64::from_le_bytes(bytes)))
            }
            REG_BINARY => {
                // Qt reads REG_BINARY as UTF-16 text of length size/2, then
                // stringToVariant — used for string lists that cannot be MULTI_SZ.
                let units = data.len() / 2;
                let mut utf16 = Vec::with_capacity(units);
                for chunk in data.chunks_exact(2).take(units) {
                    utf16.push(u16::from_le_bytes([chunk[0], chunk[1]]));
                }
                while utf16.last() == Some(&0) {
                    utf16.pop();
                }
                Some(RegistryValue::Sz(String::from_utf16_lossy(&utf16)))
            }
            _ => None,
        }
    }

    fn utf16_zstring(data: &[u8]) -> String {
        let units: Vec<u16> = data
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .take_while(|&unit| unit != 0)
            .collect();
        String::from_utf16_lossy(&units)
    }

    fn utf16_multi_sz(data: &[u8]) -> Vec<String> {
        let units: Vec<u16> = data
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();
        let mut items = Vec::new();
        let mut start = 0usize;
        for (i, &unit) in units.iter().enumerate() {
            if unit == 0 {
                if start == i {
                    break;
                }
                items.push(String::from_utf16_lossy(&units[start..i]));
                start = i + 1;
            }
        }
        items
    }

    fn wide(value: &str) -> Vec<u16> {
        OsStr::new(value).encode_wide().chain([0]).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::irc::storagepath::storage_segment;
    use crate::session::sts::StsStore;
    use crate::storage::migrate::{probe_qt_migration, QtInstallLayout, QtMigrationProbe};
    use crate::storage::opendirect::OPEN_DIRECTS_GROUP;
    use crate::storage::paths::{self, Env, Platform};
    use crate::storage::scrollplace::SCROLL_PLACES_GROUP;
    use crate::storage::stspolicies::IniStsStore;
    use crate::storage::Backing;
    use crate::util::testdir::TempDir;
    use chrono::{TimeZone, Utc};
    use std::fs;

    fn fixture_registry() -> FakeRegistry {
        let mut reg = FakeRegistry::new();
        reg.insert(
            "networks/freenode",
            "host",
            RegistryValue::Sz("chat.freenode.net".into()),
        );
        reg.insert("networks/freenode", "port", RegistryValue::Dword(6697));
        reg.insert("networks/freenode", "tls", RegistryValue::Dword(1));
        reg.insert(
            "networks/freenode",
            "nick",
            RegistryValue::Sz("alice".into()),
        );
        reg.insert(
            "networks/freenode",
            "autojoin",
            RegistryValue::MultiSz(vec!["#a".into(), "#b".into()]),
        );
        reg.insert(
            &format!("{OPEN_DIRECTS_GROUP}/freenode"),
            "targets",
            RegistryValue::MultiSz(vec!["anna".into(), "dax".into()]),
        );
        reg.insert(
            &format!("{SCROLL_PLACES_GROUP}/freenode"),
            "places",
            RegistryValue::Sz(r##"[{"follow":true,"target":"#omarchy"}]"##.into()),
        );
        reg.insert(
            "window",
            "geometry",
            RegistryValue::Sz("@Rect(0 0 1180 760)".into()),
        );
        reg.insert(
            "window",
            "icon",
            RegistryValue::Sz("@ByteArray(AQID)".into()),
        );
        reg.insert("window", "atText", RegistryValue::Sz("@@literal".into()));
        reg.insert("window", "big", RegistryValue::Qword(1_000_000_000_000));
        reg
    }

    #[test]
    fn maps_registry_types_into_ini() {
        let dir = TempDir::new("reg-import-types");
        let path = dir.path().join("omairc.conf");
        let outcome = import_from_reader(&fixture_registry(), &path);
        assert_eq!(
            outcome,
            ImportOutcome::Imported { values: 11 },
            "{outcome:?}"
        );

        let settings = Settings::open(&path);
        assert_eq!(
            settings.value("networks/freenode", "host"),
            "chat.freenode.net"
        );
        assert_eq!(settings.uint_value("networks/freenode", "port", 0), 6697);
        assert!(settings.bool_value("networks/freenode", "tls", false));
        assert_eq!(
            settings.string_list("networks/freenode", "autojoin"),
            Some(vec!["#a".into(), "#b".into()])
        );
        assert_eq!(
            settings.value("window", "geometry"),
            "@Rect(0 0 1180 760)"
        );
        assert_eq!(settings.value("window", "icon"), "@ByteArray(AQID)");
        assert_eq!(settings.value("window", "atText"), "@literal");
        assert_eq!(settings.int_value("window", "big", 0), 1_000_000_000_000);

        let text = fs::read_to_string(&path).expect("ini");
        assert!(text.contains("geometry=@Rect(0 0 1180 760)\n"), "{text}");
        assert!(text.contains("icon=@ByteArray(AQID)\n"), "{text}");
        assert!(!text.contains("@@Rect"), "{text}");
    }

    #[test]
    fn imported_ini_retains_migration_fixture() {
        let dir = TempDir::new("reg-import-probe");
        let env = Env {
            local_app_data: Some(dir.path().join("lad")),
            temp_dir: dir.path().join("tmp"),
            ..Env::default()
        };
        let settings_path = paths::settings_path_for(&env, Platform::Windows)
            .expect("windows settings path");
        let sts = paths::qt_sts_path_for(&env, Platform::Windows).expect("sts");
        let transcript_root = paths::qt_transcript_root_for(&env, Platform::Windows);

        assert_eq!(
            import_from_reader(&fixture_registry(), &settings_path),
            ImportOutcome::Imported { values: 11 }
        );

        fs::create_dir_all(sts.parent().expect("parent")).expect("config");
        IniStsStore::new(sts.clone()).store(
            "irc.example",
            6697,
            60,
            Utc.timestamp_opt(1_000, 0).single().unwrap(),
        );

        let transcript = transcript_root
            .join(storage_segment("freenode", None))
            .join(storage_segment("#omarchy", None));
        fs::create_dir_all(transcript.parent().expect("parent")).expect("logs");
        fs::write(&transcript, "{\"kind\":\"msg\",\"body\":\"hi\"}\n").expect("transcript");

        let layout = QtInstallLayout {
            settings: Some(settings_path.clone()),
            sts,
            transcript_root,
        };
        let probe = probe_qt_migration(&layout);
        assert!(
            probe.retains_fixture(),
            "imported registry lost Qt data: {probe:?}"
        );
        assert!(probe.settings_ok);
        assert!(probe.network_ids > 0);
        assert!(probe.open_direct_targets > 0);
        assert!(probe.scroll_place_rows > 0);
    }

    #[test]
    fn missing_registry_is_no_registry() {
        struct Absent;
        impl RegistryReader for Absent {
            fn root_exists(&self) -> bool {
                false
            }
            fn list(&self, _: &str) -> Result<RegistryKeyContents, ()> {
                Err(())
            }
        }
        let dir = TempDir::new("reg-import-absent");
        let path = dir.path().join("omairc.conf");
        assert_eq!(
            import_from_reader(&Absent, &path),
            ImportOutcome::NoRegistry
        );
        assert!(!path.exists());
    }

    #[test]
    fn a_finished_import_skips_import_if_needed() {
        let dir = TempDir::new("reg-import-skip");
        let path = dir.path().join("omairc.conf");
        fs::write(&path, b"[migration]\nqtRegistryImported=true\n").unwrap();
        assert!(already_imported(&path));
        assert_eq!(import_if_needed(&path), ImportOutcome::Skipped);
    }

    #[test]
    fn an_ini_without_the_marker_is_not_imported_yet() {
        let dir = TempDir::new("reg-import-unmarked");
        let path = dir.path().join("omairc.conf");
        assert!(!already_imported(&path), "no file");
        fs::write(&path, b"[preferences]\ndirects=true\n").unwrap();
        assert!(!already_imported(&path), "a TUI file without the marker");
    }

    #[test]
    fn the_import_merges_into_the_tui_settings_file() {
        let dir = TempDir::new("reg-import-merge");
        let path = dir.path().join("omairc.conf");
        // What the TUI leaves in %LOCALAPPDATA%\omairc\omairc.conf on Windows.
        let mut tui = Settings::open(&path);
        tui.set_value("networks/tui1", "host", "irc.libera.chat");
        tui.set_string_list("preferences", "networkOrder", &["tui1".into()]);
        tui.set_bool("preferences", "reopenDirectMessages", false);
        tui.set_string_list("highlights", "words", &["fred".into()]);
        assert_eq!(tui.sync(), Status::Written);

        let mut reg = fixture_registry();
        reg.insert(
            "preferences",
            "networkOrder",
            RegistryValue::MultiSz(vec!["freenode".into(), "tui1".into()]),
        );
        reg.insert("preferences", "reopenDirectMessages", RegistryValue::Sz("true".into()));
        reg.insert("preferences", "loadPeerAvatars", RegistryValue::Sz("false".into()));
        reg.insert(
            "highlights",
            "words",
            RegistryValue::MultiSz(vec!["omarchy".into(), "fred".into()]),
        );
        assert!(matches!(
            import_from_reader(&reg, &path),
            ImportOutcome::Imported { .. }
        ));

        let settings = Settings::open(&path);
        assert_eq!(settings.value("networks/tui1", "host"), "irc.libera.chat");
        assert_eq!(settings.value("networks/freenode", "host"), "chat.freenode.net");
        assert_eq!(
            settings.string_list("preferences", "networkOrder"),
            Some(vec!["tui1".into(), "freenode".into()]),
            "the TUI order first, then the Qt networks it lacked"
        );
        assert!(
            !settings.bool_value("preferences", "reopenDirectMessages", true),
            "a value the file already holds wins"
        );
        assert!(!settings.bool_value("preferences", "loadPeerAvatars", true));
        assert_eq!(
            settings.string_list("highlights", "words"),
            Some(vec!["fred".into(), "omarchy".into()])
        );
        assert!(already_imported(&path));
    }

    #[test]
    fn a_second_import_does_not_bring_back_a_removed_network() {
        let dir = TempDir::new("reg-import-once");
        let path = dir.path().join("omairc.conf");
        assert!(matches!(
            import_from_reader(&fixture_registry(), &path),
            ImportOutcome::Imported { .. }
        ));
        assert_eq!(
            Backing::Disk(path.clone()).edit(|settings| settings.remove_group("networks/freenode")),
            Status::Written
        );
        assert_eq!(import_if_needed(&path), ImportOutcome::Skipped);
        assert!(Settings::open(&path).child_groups("networks").is_empty());
    }

    #[test]
    fn a_malformed_ini_is_left_alone() {
        let dir = TempDir::new("reg-import-malformed");
        let path = dir.path().join("omairc.conf");
        fs::write(&path, b"[unclosed\nkey=x\n").unwrap();
        assert!(matches!(
            import_from_reader(&fixture_registry(), &path),
            ImportOutcome::Failed(_)
        ));
        assert_eq!(fs::read(&path).unwrap(), b"[unclosed\nkey=x\n");
    }

    /// `@Variant(...)` text as `QSettings::variantToString` writes a `QStringList`:
    /// a `Qt_4_0` `QDataStream`, one character per byte.
    fn variant_text(items: &[Option<&str>]) -> String {
        let mut bytes = Vec::new();
        bytes.extend(QSTRINGLIST_TYPE_ID.to_be_bytes());
        bytes.extend((items.len() as u32).to_be_bytes());
        for item in items {
            match item {
                None => bytes.extend(u32::MAX.to_be_bytes()),
                Some(text) => {
                    let units: Vec<u8> = text.encode_utf16().flat_map(u16::to_be_bytes).collect();
                    bytes.extend((units.len() as u32).to_be_bytes());
                    bytes.extend(units);
                }
            }
        }
        let body: String = bytes.into_iter().map(char::from).collect();
        format!("@Variant({body})")
    }

    #[test]
    fn a_binary_string_list_with_an_empty_item_imports_as_a_list() {
        let dir = TempDir::new("reg-import-variant");
        let path = dir.path().join("omairc.conf");
        let mut reg = FakeRegistry::new();
        reg.insert(
            "networks/freenode",
            "autojoinKeyValues",
            RegistryValue::Sz(variant_text(&[Some("sekrit"), Some(""), None, Some("é")])),
        );
        reg.insert("window", "color", RegistryValue::Sz("@Variant(\u{0}\u{0}\u{0}\u{43})".into()));
        reg.insert(
            "networks/freenode",
            "autojoin",
            RegistryValue::MultiSz(vec!["@@op".into(), "#plain".into()]),
        );
        import_from_reader(&reg, &path);

        let settings = Settings::open(&path);
        assert_eq!(
            settings.string_list("networks/freenode", "autojoinKeyValues"),
            Some(vec!["sekrit".into(), String::new(), String::new(), "é".into()])
        );
        assert_eq!(
            settings.string_list("networks/freenode", "autojoin"),
            Some(vec!["@op".into(), "#plain".into()]),
            "Qt escapes a leading @ in a list item as @@"
        );
        assert_eq!(
            settings.value("window", "color"),
            "@Variant(\u{0}\u{0}\u{0}\u{43})",
            "a variant that is not a string list stays verbatim"
        );
    }

    #[test]
    fn variant_decoding_refuses_other_shapes() {
        assert_eq!(decode_variant_string_list("@Rect(0 0 1 1)"), None);
        assert_eq!(decode_variant_string_list("@Variant(\u{0}\u{0}\u{0}\u{b})"), None, "truncated");
        assert_eq!(
            decode_variant_string_list(&format!("{})", variant_text(&[Some("a")]).trim_end_matches(')').to_owned() + "x")),
            None,
            "trailing bytes"
        );
        assert_eq!(decode_variant_string_list(&variant_text(&[])), Some(Vec::new()));
    }

    #[cfg(any(not(windows), feature = "dev"))]
    #[test]
    fn import_if_needed_is_skipped_without_product_windows() {
        let dir = TempDir::new("reg-import-noop");
        let path = dir.path().join("missing.conf");
        // With `dev` or on non-Windows, do not invent an INI from the registry.
        assert_eq!(import_if_needed(&path), ImportOutcome::Skipped);
        assert!(!path.exists());
    }

    #[test]
    fn unescape_swaps_slashes_like_qt() {
        assert_eq!(unescape_registry_name(r"a\b"), "a/b");
        assert_eq!(unescape_registry_name("a/b"), r"a\b");
    }

    #[test]
    fn probe_helper_still_requires_settings_ok() {
        let probe = QtMigrationProbe {
            settings_ok: false,
            network_ids: 1,
            open_direct_networks: 1,
            open_direct_targets: 1,
            scroll_place_networks: 1,
            scroll_place_rows: 1,
            sts_policies: 1,
            transcript_files: 1,
        };
        assert!(!probe.retains_fixture());
    }
}
