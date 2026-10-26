//! Window geometry, the pure model behind the saved normal (restore) rectangle
//! and maximized flag. The Qt client stores the rectangle as a `QRect` under
//! `window/geometry` and the maximized flag under `window/maximized`. The INI
//! file holds the rectangle as `@Rect(x y width height)`, so this module writes
//! the same keys and value text.
//! The gpui glue that reads and applies the live window lives in
//! `desktop::windowstate`. This module has no toolkit types.

use std::path::Path;

use super::settings::{Settings, Status};

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

    pub fn save(&self, settings: &mut Settings) {
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

/// Writes `geometry` into the INI at `path`, keeping every other key. Returns the
/// status. A malformed or read-only file is refused, not overwritten.
pub fn save_to(path: &Path, geometry: &WindowGeometry) -> Status {
    let mut settings = Settings::open(path);
    geometry.save(&mut settings);
    settings.sync()
}

/// Reads the stored geometry at `path`, or `None` when it has none. A malformed file
/// is read as empty, which is what the Qt client does too.
pub fn load_from(path: &Path) -> Option<WindowGeometry> {
    WindowGeometry::load(&Settings::open(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::settings::{probe_ini, IniProbe};
    use crate::util::testdir::TempDir;

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
        let dir = TempDir::new("geometry-round-trip");
        let path = dir.path().join("omairc").join("omairc.conf");
        let saved = WindowGeometry::new(120, 64, 1024, 700, true);

        assert_eq!(save_to(&path, &saved), Status::Written);
        assert_eq!(load_from(&path), Some(saved));

        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("@Rect(120 64 1024 700)"), "{text}");
        assert!(text.contains("maximized=true"), "{text}");
    }

    #[test]
    fn loads_a_geometry_qt_wrote() {
        let dir = TempDir::new("geometry-qt");
        let path = dir.path().join("omairc.conf");
        std::fs::write(
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
    fn saving_keeps_unrelated_keys() {
        let dir = TempDir::new("geometry-keeps-keys");
        let path = dir.path().join("omairc.conf");
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
        let dir = TempDir::new("geometry-missing");
        let path = dir.path().join("omairc.conf");
        assert_eq!(load_from(&path), None);

        let mut settings = Settings::open(&path);
        settings.set_value("window", "geometry", "@Rect(0 0 0 600)");
        assert_eq!(settings.sync(), Status::Written);
        assert_eq!(load_from(&path), None);
    }

    #[test]
    fn a_malformed_store_is_not_overwritten() {
        let dir = TempDir::new("geometry-malformed");
        let path = dir.path().join("omairc.conf");
        let corrupt = b"[window\ngeometry=@Rect(0 0 800 600)\n";
        std::fs::write(&path, corrupt).unwrap();

        assert_eq!(
            save_to(&path, &WindowGeometry::new(0, 0, 900, 700, false)),
            Status::FormatError
        );
        assert_eq!(std::fs::read(&path).unwrap(), corrupt);
        assert_eq!(probe_ini(&path), IniProbe::Malformed);
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
}
