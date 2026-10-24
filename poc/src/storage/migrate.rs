//! Locates an installed Qt client's settings and state for a future cutover. This build
//! still writes under [`crate::identity::APP_FOLDER`]; this module only maps and reads the
//! installed client's [`crate::identity::QT_APP_FOLDER`] tree.
//!
//! On Windows, Qt stores main settings in the registry (`HKCU\Software\omairc\omairc`);
//! [`QtInstallLayout::settings`] is `None` there until registry import lands. STS remains
//! an INI under `%LOCALAPPDATA%\omairc\sts`, and transcripts under
//! `%LOCALAPPDATA%\State\omairc\logs`.

use std::fs;
use std::path::{Path, PathBuf};

use super::opendirect::{OPEN_DIRECTS_GROUP, TARGETS_KEY};
use super::paths::{self, Env, Platform};
use super::scrollplace::ScrollPlaceFile;
use super::settings::{probe_ini, IniProbe, Settings};
use super::Backing;
/// The on-disk layout of an installed Qt client on this machine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QtInstallLayout {
    /// INI settings path on Linux/macOS. `None` on Windows (registry
    /// `HKCU\Software\omairc\omairc`).
    pub settings: Option<PathBuf>,
    pub sts: PathBuf,
    pub transcript_root: PathBuf,
}

impl QtInstallLayout {
    /// Resolves the Qt install paths from `env` and `platform`. Returns `None` when the
    /// platform config root is unknown (STS cannot be located).
    pub fn for_env(env: &Env, platform: Platform) -> Option<Self> {
        let sts = paths::qt_sts_path_for(env, platform)?;
        Some(Self {
            settings: paths::qt_settings_path_for(env, platform),
            sts,
            transcript_root: paths::qt_transcript_root_for(env, platform),
        })
    }

    /// Whether any file migration must preserve exists under this layout.
    pub fn has_user_data(&self) -> bool {
        self.settings
            .as_ref()
            .is_some_and(|path| path.is_file())
            || self.sts.is_file()
            || count_transcript_files(&self.transcript_root) > 0
    }
}

/// What a cutover would import from an installed Qt client. Counts are zero when a
/// subtree is absent; they must stay non-zero once a matching fixture is on disk.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QtMigrationProbe {
    pub settings_ok: bool,
    pub network_ids: usize,
    pub open_direct_networks: usize,
    pub open_direct_targets: usize,
    pub scroll_place_networks: usize,
    pub scroll_place_rows: usize,
    pub sts_policies: usize,
    pub transcript_files: usize,
}

impl QtMigrationProbe {
    /// True when every persisted subtree the Qt client uses is present and readable.
    pub fn retains_fixture(&self) -> bool {
        self.settings_ok
            && self.network_ids > 0
            && self.open_direct_targets > 0
            && self.scroll_place_rows > 0
            && self.sts_policies > 0
            && self.transcript_files > 0
    }
}

/// Reads `layout` through the same stores this build uses at runtime, without writing.
/// Windows registry settings are not probed here (cutover phase 2.3).
pub fn probe_qt_migration(layout: &QtInstallLayout) -> QtMigrationProbe {
    let Some(settings_path) = layout.settings.as_ref() else {
        return QtMigrationProbe {
            sts_policies: count_sts_groups(&layout.sts),
            transcript_files: count_transcript_files(&layout.transcript_root),
            ..QtMigrationProbe::default()
        };
    };
    let settings_ok = probe_ini(settings_path) == IniProbe::Ok && settings_path.is_file();
    let settings = Settings::open(settings_path);
    let network_ids = settings.child_groups("networks").len();
    let open_networks = settings.child_groups(OPEN_DIRECTS_GROUP);
    let open_direct_networks = open_networks.len();
    let open_direct_targets = open_networks
        .iter()
        .map(|network_id| {
            settings
                .string_list(&format!("{OPEN_DIRECTS_GROUP}/{network_id}"), TARGETS_KEY)
                .map(|list| list.len())
                .unwrap_or(0)
        })
        .sum();
    let scroll = ScrollPlaceFile::new(Backing::Disk(settings_path.clone()));
    let scroll_all = scroll.load_all();
    let scroll_place_networks = scroll_all.len();
    let scroll_place_rows = scroll_all.values().map(|rows| rows.len()).sum();
    let sts_policies = count_sts_groups(&layout.sts);
    let transcript_files = count_transcript_files(&layout.transcript_root);
    QtMigrationProbe {
        settings_ok,
        network_ids,
        open_direct_networks,
        open_direct_targets,
        scroll_place_networks,
        scroll_place_rows,
        sts_policies,
        transcript_files,
    }
}

fn count_sts_groups(path: &Path) -> usize {
    if !path.is_file() {
        return 0;
    }
    let raw = fs::read_to_string(path).unwrap_or_default();
    raw.lines()
        .filter(|line| line.starts_with('[') && line.ends_with(']'))
        .count()
}

fn count_transcript_files(root: &Path) -> usize {
    if !root.is_dir() {
        return 0;
    }
    let mut count = 0usize;
    walk_transcripts(root, &mut count);
    count
}

fn walk_transcripts(dir: &Path, count: &mut usize) {
    let entries = fs::read_dir(dir).ok();
    for entry in entries.into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_transcripts(&path, count);
        } else if path.is_file() {
            *count += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::irc::storagepath::storage_segment;
    use crate::session::sts::StsStore;
    use crate::storage::opendirect::{DISMISSED_DIRECTS_GROUP, OpenDirectStore};
    #[cfg(not(feature = "product"))]
    use crate::storage::paths::QT_APP_FOLDER;
    use crate::storage::paths::SETTINGS_FILE_MACOS;
    use crate::storage::scrollplace::PlaceRows;
    use crate::storage::profilestore::{Profile, ProfileStore};
    use crate::storage::scrollplace::{ScrollPlace, ScrollPlaceFile};
    use crate::storage::settings::Status;
    use crate::storage::stspolicies::IniStsStore;
    use crate::util::testdir::TempDir;
    use chrono::{TimeZone, Utc};

    const IRC_EXAMPLE_STS_GROUP: &str =
        "a84a91b2bc2ea526cb7d244872ddffe0770a9ca3737ee9ef355e9018a76767de";

    fn fixture_env(dir: &TempDir) -> Env {
        Env {
            xdg_config_home: Some(dir.path().join("config")),
            xdg_state_home: Some(dir.path().join("state")),
            temp_dir: dir.path().join("tmp"),
            ..Env::default()
        }
    }

    fn write_qt_fixture_tree(env: &Env, platform: Platform) -> QtInstallLayout {
        let layout = QtInstallLayout::for_env(env, platform).expect("layout");
        let settings = layout
            .settings
            .clone()
            .expect("INI fixture platforms need a settings path");
        fs::create_dir_all(settings.parent().expect("parent")).expect("config dir");
        fs::create_dir_all(&layout.transcript_root).expect("logs dir");

        let profile_store = ProfileStore::new(Backing::Disk(settings.clone()));
        let profile = {
            let mut profile = Profile::new("freenode");
            profile.host = "chat.freenode.net".into();
            profile.port = 6697;
            profile.tls = true;
            profile.nick = "alice".into();
            profile
        };
        assert_eq!(profile_store.save(&profile), Status::Written);

        let mut open = OpenDirectStore::new(&Backing::Disk(settings.clone()));
        use crate::irc::casemapping::{IrcCaseMapping, IrcCaseMappingKind};
        let mapping = IrcCaseMapping::new(IrcCaseMappingKind::Rfc1459);
        open.add("freenode", "anna", mapping);
        open.add("freenode", "dax", mapping);
        open.dismiss("freenode", "dax", mapping);

        let scroll = ScrollPlaceFile::new(Backing::Disk(settings));
        let rows: PlaceRows = vec![(
            "#omarchy".into(),
            ScrollPlace {
                follow_end: true,
                ..ScrollPlace::default()
            },
        )];
        scroll.save("freenode", &rows);

        IniStsStore::new(layout.sts.clone()).store(
            "irc.example",
            6697,
            60,
            Utc.timestamp_opt(1_000, 0).single().unwrap(),
        );

        let transcript = layout
            .transcript_root
            .join(storage_segment("freenode", None))
            .join(storage_segment("#omarchy", None));
        fs::create_dir_all(transcript.parent().expect("parent")).expect("network dir");
        fs::write(&transcript, "{\"kind\":\"msg\",\"body\":\"hi\"}\n").expect("transcript");

        layout
    }

    #[test]
    fn migration_fixture_linux_ini_tree_is_not_treated_as_empty() {
        let dir = TempDir::new("qt-migrate-linux");
        let env = fixture_env(&dir);
        let layout = write_qt_fixture_tree(&env, Platform::Linux);
        let probe = probe_qt_migration(&layout);
        assert!(
            probe.retains_fixture(),
            "probe dropped Qt data: {probe:?}"
        );
        assert!(layout.has_user_data());
        let ini = fs::read_to_string(layout.settings.as_ref().expect("settings"))
            .expect("settings");
        assert!(ini.contains("[networks]"), "{ini}");
        assert!(ini.contains("[openDirects]"), "{ini}");
        assert!(ini.contains("[scrollPlaces]"), "{ini}");
        assert!(fs::read_to_string(&layout.sts).expect("sts").contains(IRC_EXAMPLE_STS_GROUP));
    }

    fn macos_env(dir: &TempDir) -> Env {
        Env {
            home: Some(dir.path().join("home")),
            temp_dir: dir.path().join("tmp"),
            ..Env::default()
        }
    }

    #[test]
    fn migration_fixture_macos_ini_paths_match_qsettings_layout() {
        let dir = TempDir::new("qt-migrate-macos-paths");
        let env = macos_env(&dir);
        let layout = QtInstallLayout::for_env(&env, Platform::MacOs).expect("layout");
        let settings = layout.settings.as_ref().expect("macos ini settings");
        assert!(
            settings
                .components()
                .any(|part| part.as_os_str() == "Preferences"),
            "macOS Qt settings live under Library/Preferences: {}",
            settings.display()
        );
        assert_eq!(
            layout
                .settings
                .as_ref()
                .and_then(|path| path.file_name())
                .and_then(|name| name.to_str()),
            Some(SETTINGS_FILE_MACOS)
        );
        let written = write_qt_fixture_tree(&env, Platform::MacOs);
        let probe = probe_qt_migration(&written);
        assert!(probe.retains_fixture(), "probe dropped Qt data: {probe:?}");
    }

    #[test]
    fn migration_windows_layout_skips_ini_settings() {
        let dir = TempDir::new("qt-migrate-windows-paths");
        let env = Env {
            local_app_data: Some(dir.path().join("lad")),
            temp_dir: dir.path().join("tmp"),
            ..Env::default()
        };
        let layout = QtInstallLayout::for_env(&env, Platform::Windows).expect("layout");
        assert_eq!(layout.settings, None, "Windows Qt settings are the registry");
        assert_eq!(
            layout.sts,
            dir.path().join("lad").join(paths::QT_APP_FOLDER).join(paths::STS_FILE)
        );
        assert_eq!(
            layout.transcript_root,
            dir.path()
                .join("lad")
                .join("State")
                .join(paths::QT_APP_FOLDER)
                .join("logs")
        );
        fs::create_dir_all(layout.sts.parent().expect("parent")).expect("config");
        fs::write(&layout.sts, "[stub]\n").expect("sts");
        assert!(layout.has_user_data());
        let probe = probe_qt_migration(&layout);
        assert!(!probe.settings_ok);
        assert_eq!(probe.sts_policies, 1);
    }

    /// Without `product`, Qt migration paths must not share the PoC folder.
    #[cfg(not(feature = "product"))]
    #[test]
    fn migration_paths_stay_outside_the_poc_folder() {
        let dir = TempDir::new("qt-migrate-separate");
        let env = fixture_env(&dir);
        let layout = write_qt_fixture_tree(&env, Platform::Linux);
        let settings = layout.settings.as_ref().expect("linux settings");
        for path in [settings, &layout.sts, &layout.transcript_root] {
            assert!(
                path.components().any(|part| part.as_os_str() == QT_APP_FOLDER),
                "{} should stay under the Qt folder",
                path.display()
            );
            assert!(
                !path.components().any(|part| part.as_os_str() == paths::APP_FOLDER),
                "{} must not use the PoC folder",
                path.display()
            );
        }
        let poc_settings = paths::settings_path_for(&env, Platform::Linux);
        assert_ne!(layout.settings, poc_settings);
    }

    #[test]
    fn migration_does_not_rewrite_fixture_bytes() {
        let dir = TempDir::new("qt-migrate-readonly");
        let env = fixture_env(&dir);
        let layout = write_qt_fixture_tree(&env, Platform::Linux);
        let settings = layout.settings.as_ref().expect("settings");
        let before_settings = fs::read(settings).expect("settings");
        let before_sts = fs::read(&layout.sts).expect("sts");
        let before_logs = fs::read_dir(&layout.transcript_root)
            .expect("logs")
            .count();
        let _ = probe_qt_migration(&layout);
        assert_eq!(fs::read(settings).expect("settings"), before_settings);
        assert_eq!(fs::read(&layout.sts).expect("sts"), before_sts);
        assert_eq!(
            fs::read_dir(&layout.transcript_root).expect("logs").count(),
            before_logs
        );
    }

    #[test]
    fn migration_open_direct_groups_are_counted() {
        let dir = TempDir::new("qt-migrate-opendirect");
        let env = fixture_env(&dir);
        let layout = write_qt_fixture_tree(&env, Platform::Linux);
        let probe = probe_qt_migration(&layout);
        assert!(probe.open_direct_networks > 0, "{probe:?}");
        assert!(probe.open_direct_targets > 0, "{probe:?}");
        let ini = fs::read_to_string(layout.settings.as_ref().expect("settings"))
            .expect("settings");
        assert!(ini.contains(DISMISSED_DIRECTS_GROUP), "{ini}");
    }
}
