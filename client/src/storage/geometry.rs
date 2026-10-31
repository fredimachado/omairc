//! Window geometry, the pure model behind the saved normal (restore) rectangle
//! and maximized flag. The Qt client stores the rectangle as a `QRect` under
//! `window/geometry` and the maximized flag under `window/maximized`. The GPUI
//! desktop client writes the same keys under `[window]` in `<state>/window.ini`.
//! The INI holds the rectangle as `@Rect(x y width height)`.
//! The gpui glue that reads and applies the live window lives in
//! `desktop::windowstate`. This module has no toolkit types.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use super::settings::{file_not_writable, probe_ini, IniProbe, Settings, Status};

const GROUP: &str = "window";
const KEY_GEOMETRY: &str = "geometry";
const KEY_MAXIMIZED: &str = "maximized";

/// A window's restore rectangle and whether it was maximized (or fullscreen).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowGeometry {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub maximized: bool,
}

impl WindowGeometry {
    pub fn new(x: i32, y: i32, width: i32, height: i32, maximized: bool) -> Self {
        Self {
            x,
            y,
            width,
            height,
            maximized,
        }
    }

    /// Qt's `QRect::isValid`: a rectangle needs at least one pixel each way.
    pub fn is_valid(&self) -> bool {
        self.width >= 1 && self.height >= 1
    }

    /// The stored geometry, or `None` when none is stored or the stored rectangle is
    /// not valid. Qt ignores an invalid rectangle the same way.
    pub fn load(settings: &Settings) -> Option<Self> {
        let (x, y, width, height) = parse_rect(&settings.value(GROUP, KEY_GEOMETRY))?;
        let geometry = Self::new(
            x,
            y,
            width,
            height,
            settings.bool_value(GROUP, KEY_MAXIMIZED, false),
        );
        geometry.is_valid().then_some(geometry)
    }

    fn save(&self, settings: &mut Settings) {
        settings.set_variant(
            GROUP,
            KEY_GEOMETRY,
            &format_rect(self.x, self.y, self.width, self.height),
        );
        settings.set_bool(GROUP, KEY_MAXIMIZED, self.maximized);
    }
}

/// The `@Rect(x y width height)` text Qt writes for a `QRect`.
pub fn format_rect(x: i32, y: i32, width: i32, height: i32) -> String {
    format!("@Rect({x} {y} {width} {height})")
}

/// Parses `@Rect(x y width height)`, or `None` for any other text.
pub fn parse_rect(text: &str) -> Option<(i32, i32, i32, i32)> {
    let inner = text.strip_prefix("@Rect(")?.strip_suffix(')')?;
    let mut numbers = inner.split_whitespace().map(str::parse::<i32>);
    let x = numbers.next()?.ok()?;
    let y = numbers.next()?.ok()?;
    let width = numbers.next()?.ok()?;
    let height = numbers.next()?.ok()?;
    if numbers.next().is_some() {
        return None;
    }
    Some((x, y, width, height))
}

/// Remembers the geometry last written, so bounds events that change nothing skip
/// the disk. A failed write is not recorded, so the next event tries again.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GeometryTracker {
    last_saved: Option<WindowGeometry>,
}

impl GeometryTracker {
    /// Starts from the geometry already on disk, so a launch that changes nothing
    /// writes nothing.
    pub fn new(stored: Option<WindowGeometry>) -> Self {
        Self { last_saved: stored }
    }

    /// Whether `next` differs from the last geometry written.
    pub fn differs(&self, next: WindowGeometry) -> bool {
        self.last_saved != Some(next)
    }

    pub fn record(&mut self, saved: WindowGeometry) {
        self.last_saved = Some(saved);
    }
}

/// Reads geometry from `window_path` when it exists. When `window.ini` is missing,
/// reads the legacy `[window]` group from `conf_path` on each launch until the first
/// successful save creates `window.ini`. Never writes to `conf_path`.
pub fn load_stored(window_path: &Path, conf_path: Option<&Path>) -> Option<WindowGeometry> {
    if window_path.exists() {
        return load_from(window_path);
    }
    conf_path.and_then(|path| WindowGeometry::load(&Settings::open(path)))
}

/// Writes `geometry` into `window_path`, keeping every other key in that file when
/// it is well-formed. A malformed `window.ini` is hard-linked to `window.ini.corrupt`
/// (then `.corrupt.1`, …) and replaced with a clean file. Returns the status.
pub fn save_to(window_path: &Path, geometry: &WindowGeometry) -> Status {
    let mut settings = Settings::open(window_path);
    geometry.save(&mut settings);
    match settings.sync() {
        Status::FormatError if probe_ini(window_path) == IniProbe::Malformed => {
            recover_malformed_window_ini(window_path, geometry)
        }
        other => other,
    }
}

const MAX_WINDOW_CORRUPT_BACKUPS: u32 = 10_000;

fn recover_malformed_window_ini(window_path: &Path, geometry: &WindowGeometry) -> Status {
    if file_not_writable(window_path) {
        return Status::AccessError;
    }
    let encoded = encoded_geometry(geometry);
    match recover_malformed_ini_file(window_path, &encoded) {
        Ok(()) => Status::Written,
        Err(_) => Status::FormatError,
    }
}

fn encoded_geometry(geometry: &WindowGeometry) -> Vec<u8> {
    let mut settings = Settings::in_memory();
    geometry.save(&mut settings);
    settings.encode()
}

/// Temp sibling, hard-link backup, rename — same shape as STS malformed recovery.
fn recover_malformed_ini_file(path: &Path, data: &[u8]) -> io::Result<()> {
    let target = super::atomicwrite::atomic_write_target(path)?;
    super::atomicwrite::ensure_atomic_write_parent(&target)?;
    let temp = write_window_temp(&target.destination, data)?;
    let Some(backup) = link_window_corrupt_backup(&target.destination) else {
        let _ = fs::remove_file(&temp);
        return Err(io::Error::other("window.ini corrupt backup failed"));
    };
    match fs::rename(&temp, &target.destination) {
        Ok(()) => {
            restrict_backup_permissions(&backup);
            Ok(())
        }
        Err(err) => {
            let _ = fs::remove_file(&temp);
            let _ = fs::remove_file(&backup);
            Err(err)
        }
    }
}

fn write_window_temp(path: &Path, data: &[u8]) -> io::Result<PathBuf> {
    let dir = super::atomicwrite::atomic_write_dir(path).unwrap_or_else(|| PathBuf::from("."));
    let mut last_error = None;
    for n in 0..16u32 {
        let candidate = dir.join(format!(
            ".omairc-window-{}-{n}.tmp",
            std::process::id()
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(mut file) => {
                let write_result = (|| -> io::Result<()> {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        file.set_permissions(fs::Permissions::from_mode(0o600))?;
                    }
                    file.write_all(data)?;
                    file.sync_all()
                })();
                drop(file);
                match write_result {
                    Ok(()) => return Ok(candidate),
                    Err(err) => {
                        let _ = fs::remove_file(&candidate);
                        return Err(err);
                    }
                }
            }
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => last_error = Some(err),
            Err(err) => return Err(err),
        }
    }
    Err(last_error.unwrap_or_else(|| io::Error::other("no free window temp name")))
}

fn link_window_corrupt_backup(path: &Path) -> Option<PathBuf> {
    let parent = super::atomicwrite::atomic_write_dir(path)?;
    let name = super::atomicwrite::atomic_write_base(path);
    let name = name.to_string_lossy();
    for n in 0..MAX_WINDOW_CORRUPT_BACKUPS {
        let candidate = if n == 0 {
            parent.join(format!("{name}.corrupt"))
        } else {
            parent.join(format!("{name}.corrupt.{n}"))
        };
        match fs::hard_link(path, &candidate) {
            Ok(()) => return Some(candidate),
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(_) => match copy_window_exclusive(path, &candidate) {
                Ok(()) => return Some(candidate),
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(_) => return None,
            },
        }
    }
    None
}

fn copy_window_exclusive(src: &Path, dst: &Path) -> io::Result<()> {
    let data = fs::read(src)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dst)?;
    let write_result = (|| -> io::Result<()> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(0o600))?;
        }
        file.write_all(&data)?;
        file.sync_all()
    })();
    drop(file);
    match write_result {
        Ok(()) => Ok(()),
        Err(err) => {
            let _ = fs::remove_file(dst);
            Err(err)
        }
    }
}

#[cfg(unix)]
fn restrict_backup_permissions(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restrict_backup_permissions(_path: &Path) {}

/// Reads the stored geometry at `path`, or `None` when it has none. A malformed file
/// is read as empty, which is what the Qt client does too.
pub fn load_from(path: &Path) -> Option<WindowGeometry> {
    WindowGeometry::load(&Settings::open(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::paths::{self, Env, Platform, WINDOW_FILE};
    use crate::storage::settings::{probe_ini, IniProbe};
    use crate::util::testdir::TempDir;
    use std::fs;
    use std::path::PathBuf;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn parses_qrect_text() {
        assert_eq!(parse_rect("@Rect(0 0 1180 760)"), Some((0, 0, 1180, 760)));
        assert_eq!(parse_rect("@Rect(-8 -8 900 600)"), Some((-8, -8, 900, 600)));
        assert_eq!(parse_rect("@Rect(1 2 3)"), None);
        assert_eq!(parse_rect("@Rect(1 2 3 4 5)"), None);
        assert_eq!(parse_rect("@Rect(a b c d)"), None);
        assert_eq!(parse_rect("Rect(1 2 3 4)"), None);
        assert_eq!(parse_rect(""), None);
        assert_eq!(format_rect(0, 0, 1180, 760), "@Rect(0 0 1180 760)");
    }

    #[test]
    fn invalid_rectangles_are_not_valid() {
        assert!(WindowGeometry::new(0, 0, 1, 1, false).is_valid());
        assert!(!WindowGeometry::new(0, 0, 0, 760, false).is_valid());
        assert!(!WindowGeometry::new(0, 0, 1180, -1, false).is_valid());
    }

    #[test]
    fn save_then_load_round_trips_the_maximized_flag() {
        let dir = TempDir::new("geometry-round-trip-f862");
        let path = dir.path().join(WINDOW_FILE);
        let saved = WindowGeometry::new(120, 64, 1024, 700, true);

        assert_eq!(save_to(&path, &saved), Status::Written);
        assert_eq!(load_from(&path), Some(saved));

        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("@Rect(120 64 1024 700)"), "{text}");
        assert!(text.contains("maximized=true"), "{text}");
    }

    #[test]
    fn loads_a_geometry_qt_wrote() {
        let dir = TempDir::new("geometry-qt-f862");
        let path = dir.path().join(WINDOW_FILE);
        fs::write(
            &path,
            "[window]\ngeometry=@Rect(10 20 300 400)\nmaximized=true\n",
        )
        .unwrap();
        assert_eq!(
            load_from(&path),
            Some(WindowGeometry::new(10, 20, 300, 400, true))
        );
    }

    #[test]
    fn saving_keeps_unrelated_keys_in_window_ini() {
        let dir = TempDir::new("geometry-keeps-keys-f862");
        let path = dir.path().join(WINDOW_FILE);
        let mut other = Settings::open(&path);
        other.set_bool("preferences", "directs", true);
        assert_eq!(other.sync(), Status::Written);

        assert_eq!(
            save_to(&path, &WindowGeometry::new(0, 0, 800, 600, false)),
            Status::Written
        );
        assert!(Settings::open(&path).bool_value("preferences", "directs", false));
    }

    #[test]
    fn missing_or_invalid_geometry_loads_as_none() {
        let dir = TempDir::new("geometry-missing-f862");
        let path = dir.path().join(WINDOW_FILE);
        assert_eq!(load_from(&path), None);

        let mut settings = Settings::open(&path);
        settings.set_value("window", "geometry", "@Rect(0 0 0 600)");
        assert_eq!(settings.sync(), Status::Written);
        assert_eq!(load_from(&path), None);
    }

    #[test]
    fn malformed_window_ini_is_backed_up_and_rewritten() {
        let dir = TempDir::new("geometry-malformed-window-f862");
        let path = dir.path().join(WINDOW_FILE);
        let corrupt = b"[window\ngeometry=@Rect(0 0 800 600)\n";
        fs::write(&path, corrupt).unwrap();
        let saved = WindowGeometry::new(0, 0, 900, 700, false);

        assert_eq!(save_to(&path, &saved), Status::Written);
        assert_eq!(load_from(&path), Some(saved));
        assert_eq!(fs::read(path.with_file_name("window.ini.corrupt")).unwrap(), corrupt);
        assert_eq!(probe_ini(&path), IniProbe::Ok);
    }

    #[test]
    fn malformed_omairc_conf_is_not_overwritten() {
        let dir = TempDir::new("geometry-malformed-conf-f862");
        let conf = dir.path().join("omairc.conf");
        let corrupt = b"[preferences\nbad=1\n";
        fs::write(&conf, corrupt).unwrap();

        let mut settings = Settings::open(&conf);
        settings.set_bool("preferences", "directs", true);
        assert_eq!(settings.sync(), Status::FormatError);
        assert_eq!(fs::read(&conf).unwrap(), corrupt);
        assert_eq!(probe_ini(&conf), IniProbe::Malformed);
    }

    #[cfg(unix)]
    #[test]
    #[allow(clippy::permissions_set_readonly_false)]
    fn read_only_malformed_window_ini_is_left_alone() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TempDir::new("geometry-readonly-malformed-f862");
        let path = dir.path().join(WINDOW_FILE);
        let corrupt = b"[window\ngeometry=@Rect(0 0 800 600)\n";
        fs::write(&path, corrupt).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).unwrap();

        if fs::OpenOptions::new().write(true).open(&path).is_ok() {
            fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
            return;
        }

        assert_eq!(
            save_to(&path, &WindowGeometry::new(0, 0, 900, 700, false)),
            Status::AccessError
        );
        assert_eq!(fs::read(&path).unwrap(), corrupt);
        assert!(!dir.path().join("window.ini.corrupt").exists());

        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn malformed_window_ini_through_symlink_backs_up_target_and_keeps_link() {
        use std::os::unix::fs::symlink;

        let dir = TempDir::new("geometry-symlink-recover-3dba");
        let real_ini = dir.path().join("real").join(WINDOW_FILE);
        fs::create_dir_all(real_ini.parent().unwrap()).unwrap();
        let corrupt = b"[window\ngeometry=@Rect(0 0 800 600)\n";
        fs::write(&real_ini, corrupt).unwrap();
        let cfg_link = dir.path().join("cfg");
        fs::create_dir_all(dir.path().join("real").join("cfg")).unwrap();
        symlink(dir.path().join("real").join("cfg"), &cfg_link).unwrap();
        let link = cfg_link.join(WINDOW_FILE);
        symlink(format!("../{WINDOW_FILE}"), &link).unwrap();
        let saved = WindowGeometry::new(0, 0, 900, 700, false);

        assert_eq!(save_to(&link, &saved), Status::Written);
        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(
            fs::read(real_ini.with_file_name("window.ini.corrupt")).unwrap(),
            corrupt
        );
        assert_eq!(load_from(&real_ini), Some(saved));
    }

    #[test]
    fn failed_rename_removes_the_corrupt_backup() {
        let dir = TempDir::new("geometry-rename-fail-f862");
        let path = dir.path().join(WINDOW_FILE);
        let corrupt = b"[window\nbad=1\n";
        fs::write(&path, corrupt).unwrap();
        let data = encoded_geometry(&WindowGeometry::new(0, 0, 900, 700, false));
        let temp = write_window_temp(&path, &data).expect("temp");
        let backup = link_window_corrupt_backup(&path).expect("backup");
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(fs::rename(&temp, &path).is_err());
        let _ = fs::remove_file(&temp);
        let _ = fs::remove_file(&backup);
        assert!(!backup.exists());
        assert!(path.is_dir());

        assert!(recover_malformed_ini_file(&path, &data).is_err());
        assert!(!dir.path().join("window.ini.corrupt").exists());
    }

    #[test]
    fn tracker_skips_unchanged_geometry() {
        let first = WindowGeometry::new(10, 10, 900, 700, false);
        let moved = WindowGeometry::new(20, 10, 900, 700, false);
        let mut tracker = GeometryTracker::default();

        assert!(tracker.differs(first));
        tracker.record(first);
        assert!(!tracker.differs(first));
        assert!(tracker.differs(moved));

        let seeded = GeometryTracker::new(Some(first));
        assert!(!seeded.differs(first));
    }

    fn set_mode(path: &Path, mode: u32) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
        }
        #[cfg(not(unix))]
        {
            let _ = (path, mode);
        }
    }

    fn sleep_for_mtime() {
        thread::sleep(Duration::from_millis(1100));
    }

    #[test]
    fn geometry_save_leaves_conf_bytes_and_mtime_alone() {
        let dir = TempDir::new("geometry-conf-untouched-f862");
        let conf = dir.path().join("omairc.conf");
        let window = dir.path().join(WINDOW_FILE);
        fs::write(
            &conf,
            "[preferences]\ndirects=true\n[window]\ngeometry=@Rect(1 2 3 4)\n",
        )
        .unwrap();
        set_mode(&conf, 0o644);
        sleep_for_mtime();
        let before_bytes = fs::read(&conf).unwrap();
        let before_mtime = fs::metadata(&conf).unwrap().modified().unwrap();

        assert_eq!(
            save_to(&window, &WindowGeometry::new(40, 50, 900, 700, false)),
            Status::Written
        );
        assert_eq!(fs::read(&conf).unwrap(), before_bytes);
        assert_eq!(fs::metadata(&conf).unwrap().modified().unwrap(), before_mtime);
        assert!(window.exists());

        sleep_for_mtime();
        assert_eq!(
            save_to(&window, &WindowGeometry::new(40, 50, 900, 700, true)),
            Status::Written
        );
        assert_eq!(fs::read(&conf).unwrap(), before_bytes);
        assert_eq!(fs::metadata(&conf).unwrap().modified().unwrap(), before_mtime);

        sleep_for_mtime();
        assert_eq!(
            save_to(&window, &WindowGeometry::new(40, 50, 900, 700, true)),
            Status::Written
        );
        assert_eq!(fs::read(&conf).unwrap(), before_bytes);
        assert_eq!(fs::metadata(&conf).unwrap().modified().unwrap(), before_mtime);
    }

    #[test]
    fn window_ini_honours_xdg_state_home_and_dev_home() {
        let home = PathBuf::from("home-f862");
        let xdg = TempDir::new("geometry-xdg-f862");
        let env = Env {
            xdg_state_home: Some(xdg.path().to_path_buf()),
            home: Some(home.clone()),
            temp_dir: PathBuf::from("tmp"),
            ..Env::default()
        };
        let window = paths::window_path_for(&env, Platform::Linux);
        assert_eq!(save_to(&window, &WindowGeometry::new(1, 2, 800, 600, false)), Status::Written);
        assert!(window.starts_with(xdg.path()));
        assert_eq!(load_from(&window).unwrap().width, 800);

        let dev = TempDir::new("geometry-dev-f862");
        let dev_env = Env {
            dev_home: Some(dev.path().to_path_buf()),
            xdg_state_home: Some(PathBuf::from("ignored")),
            home: Some(home),
            temp_dir: PathBuf::from("tmp"),
            ..Env::default()
        };
        let dev_window = paths::window_path_for(&dev_env, Platform::Linux);
        assert_eq!(
            save_to(&dev_window, &WindowGeometry::new(3, 4, 700, 500, true)),
            Status::Written
        );
        assert_eq!(
            dev_window,
            dev.path().join("state").join(WINDOW_FILE)
        );
        assert!(load_from(&dev_window).unwrap().maximized);
    }

    #[test]
    fn conf_fallback_restores_once_and_writes_window_ini() {
        let dir = TempDir::new("geometry-fallback-f862");
        let conf = dir.path().join("omairc.conf");
        let window = dir.path().join(WINDOW_FILE);
        fs::write(
            &conf,
            "[window]\ngeometry=@Rect(100 100 900 650)\nmaximized=false\n",
        )
        .unwrap();
        set_mode(&conf, 0o600);
        let conf_bytes = fs::read(&conf).unwrap();

        let restored = load_stored(&window, Some(&conf)).expect("fallback geometry");
        assert_eq!(restored.width, 900);
        assert_eq!(restored.height, 650);
        assert_eq!(restored.x, 100);
        assert_eq!(fs::read(&conf).unwrap(), conf_bytes);
        assert!(!window.exists());

        assert_eq!(
            save_to(&window, &WindowGeometry::new(100, 100, 900, 650, false)),
            Status::Written
        );
        assert_eq!(fs::read(&conf).unwrap(), conf_bytes);
        assert!(window.exists());
        assert_eq!(load_from(&window), Some(restored));
    }

    #[test]
    fn new_window_ini_is_owner_only() {
        let dir = TempDir::new("geometry-mode-f862");
        let window = dir.path().join(WINDOW_FILE);
        assert_eq!(
            save_to(&window, &WindowGeometry::new(0, 0, 800, 600, false)),
            Status::Written
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&window).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
    }

    #[test]
    fn geometry_save_must_target_window_ini_not_conf() {
        let dir = TempDir::new("geometry-mutation-guard-f862");
        let conf = dir.path().join("omairc.conf");
        let window = dir.path().join(WINDOW_FILE);
        fs::write(&conf, "[preferences]\nkeep=yes\n").unwrap();
        sleep_for_mtime();
        let before = fs::read(&conf).unwrap();

        assert_eq!(
            save_to(&window, &WindowGeometry::new(10, 20, 900, 700, false)),
            Status::Written
        );
        assert_eq!(fs::read(&conf).unwrap(), before);
        assert!(window.exists());
        assert!(
            !fs::read_to_string(&conf).unwrap().contains("[window]"),
            "geometry save must not add a [window] group to omairc.conf"
        );
    }
}
