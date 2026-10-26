//! The live window's geometry: where it opens, and how it is saved on move, resize
//! and close. The shell handlers track the normal rectangle and persist on
//! destruction. The pure model is `storage::geometry`; this module only maps
//! gpui's `WindowBounds` onto it.
//!
//! The normal rectangle is saved on close and is not clamped to a display.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use gpui::{point, px, size, App, Bounds, Entity, Window, WindowBounds};

use crate::storage::geometry::{self, GeometryTracker, WindowGeometry};
use crate::storage::settings::Status;
use crate::util::OmaircStyleGlobal;

/// The size of a first launch, with no stored geometry.
const DEFAULT_SIZE: (f32, f32) = (1180.0, 760.0);

/// Where the window opens: the stored rectangle (maximized when it was), or the
/// default size centred on screen.
pub fn initial_bounds(stored: Option<WindowGeometry>, cx: &App) -> WindowBounds {
    match stored {
        Some(geometry) => restore_bounds(geometry),
        None => {
            let text_scale = cx.global::<OmaircStyleGlobal>().style.text_scale;
            let default = size(
                px(DEFAULT_SIZE.0 * text_scale),
                px(DEFAULT_SIZE.1 * text_scale),
            );
            WindowBounds::Windowed(Bounds::centered(None, default, cx))
        }
    }
}

fn restore_bounds(geometry: WindowGeometry) -> WindowBounds {
    let bounds = Bounds {
        origin: point(px(geometry.x as f32), px(geometry.y as f32)),
        size: size(px(geometry.width as f32), px(geometry.height as f32)),
    };
    if geometry.maximized {
        WindowBounds::Maximized(bounds)
    } else {
        WindowBounds::Windowed(bounds)
    }
}

/// The geometry a window reports: its restore rectangle and whether it is maximized.
/// Fullscreen counts as maximized, as it does in Qt.
pub fn geometry_of(window_bounds: WindowBounds) -> WindowGeometry {
    let (bounds, maximized) = match window_bounds {
        WindowBounds::Windowed(bounds) => (bounds, false),
        WindowBounds::Maximized(bounds) | WindowBounds::Fullscreen(bounds) => (bounds, true),
    };
    WindowGeometry::new(
        f32::from(bounds.origin.x).round() as i32,
        f32::from(bounds.origin.y).round() as i32,
        f32::from(bounds.size.width).round() as i32,
        f32::from(bounds.size.height).round() as i32,
        maximized,
    )
}

/// Saves the window's geometry when it changes. Clones share one tracker, so the
/// bounds observer and the close handler see the same last save.
#[derive(Clone)]
pub struct GeometryKeeper {
    inner: Rc<RefCell<Keeper>>,
}

struct Keeper {
    path: PathBuf,
    tracker: GeometryTracker,
}

impl GeometryKeeper {
    /// `stored` is the geometry already on disk, so a launch that changes nothing
    /// writes nothing.
    pub fn new(path: PathBuf, stored: Option<WindowGeometry>) -> Self {
        Self {
            inner: Rc::new(RefCell::new(Keeper {
                path,
                tracker: GeometryTracker::new(stored),
            })),
        }
    }

    /// Saves `window_bounds` if it differs from the last save. A rectangle that is not
    /// valid is skipped, since a minimised window can report one. A failed write is
    /// logged once and not retried for the same rectangle.
    pub fn save(&self, window_bounds: WindowBounds) {
        let geometry = geometry_of(window_bounds);
        if !geometry.is_valid() {
            return;
        }
        let mut keeper = self.inner.borrow_mut();
        if !keeper.tracker.differs(geometry) {
            return;
        }
        let status = geometry::save_to(&keeper.path, &geometry);
        if status != Status::Written {
            log::warn!("window geometry not saved: {status:?}");
        }
        keeper.tracker.record(geometry);
    }
}

/// Saves geometry and runs `also` when the window closes. GPUI keeps one close
/// callback, so every close side effect must go through here.
pub fn run_close_handlers(
    keeper: &GeometryKeeper,
    window_bounds: WindowBounds,
    also: impl FnOnce(),
) -> bool {
    keeper.save(window_bounds);
    also();
    true
}

/// Saves the window's geometry on close, and on every move or resize that `entity`
/// observes. `on_close` runs after the geometry save (Qt `onClosing` order:
/// remember geometry, then launch a ready installer). Call once after the window
/// is open.
pub fn attach<T: 'static>(
    keeper: &GeometryKeeper,
    entity: &Entity<T>,
    window: &mut Window,
    cx: &mut App,
    on_close: impl Fn(&mut Window, &mut App) + 'static,
) {
    let on_close_keeper = keeper.clone();
    window.on_window_should_close(cx, move |window, cx| {
        run_close_handlers(&on_close_keeper, window.window_bounds(), || on_close(window, cx))
    });

    let on_bounds = keeper.clone();
    entity.update(cx, |_, cx| {
        cx.observe_window_bounds(window, move |_, window, _| {
            on_bounds.save(window.window_bounds());
        })
        .detach();
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::settings::{probe_ini, IniProbe};
    use crate::util::testdir::TempDir;
    use gpui::Pixels;

    fn bounds(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
        Bounds {
            origin: point(px(x), px(y)),
            size: size(px(w), px(h)),
        }
    }

    #[test]
    fn maps_window_bounds_onto_the_stored_geometry() {
        assert_eq!(
            geometry_of(WindowBounds::Windowed(bounds(10.4, 20.6, 1180.0, 760.0))),
            WindowGeometry::new(10, 21, 1180, 760, false)
        );
        assert_eq!(
            geometry_of(WindowBounds::Maximized(bounds(0.0, 0.0, 900.0, 600.0))),
            WindowGeometry::new(0, 0, 900, 600, true)
        );
        assert!(geometry_of(WindowBounds::Fullscreen(bounds(0.0, 0.0, 900.0, 600.0))).maximized);
    }

    #[test]
    fn keeper_saves_only_changes_and_skips_invalid_rectangles() {
        let dir = TempDir::new("windowstate-keeper");
        let path = dir.path().join("omairc.conf");
        let keeper = GeometryKeeper::new(path.clone(), None);

        keeper.save(WindowBounds::Windowed(bounds(0.0, 0.0, 0.0, 0.0)));
        assert!(!path.exists(), "an invalid rectangle must not be written");

        keeper.save(WindowBounds::Windowed(bounds(40.0, 50.0, 900.0, 700.0)));
        let first = geometry::load_from(&path).expect("saved geometry");
        assert_eq!(first, WindowGeometry::new(40, 50, 900, 700, false));

        keeper.save(WindowBounds::Maximized(bounds(40.0, 50.0, 900.0, 700.0)));
        let maximized = geometry::load_from(&path).expect("saved geometry");
        assert!(maximized.maximized);
        assert_eq!(probe_ini(&path), IniProbe::Ok);
    }

    #[test]
    fn geometry_close_saves_and_runs_the_also_hook() {
        let dir = TempDir::new("windowstate-close");
        let path = dir.path().join("omairc.conf");
        let keeper = GeometryKeeper::new(path.clone(), None);
        let bounds = WindowBounds::Windowed(bounds(40.0, 50.0, 900.0, 700.0));
        let also_ran = std::cell::Cell::new(false);

        assert!(run_close_handlers(&keeper, bounds, || also_ran.set(true)));
        assert!(path.exists(), "close must persist geometry");
        assert!(also_ran.get(), "close must run the chained hook");
    }

    #[test]
    fn geometry_close_skipping_the_hook_still_saves() {
        let dir = TempDir::new("windowstate-close-save-only");
        let path = dir.path().join("omairc.conf");
        let keeper = GeometryKeeper::new(path.clone(), None);
        let bounds = WindowBounds::Windowed(bounds(40.0, 50.0, 900.0, 700.0));

        assert!(run_close_handlers(&keeper, bounds, || {}));
        assert!(path.exists());
    }

    #[test]
    fn restore_reopens_the_stored_rectangle_in_its_state() {
        let maximized = restore_bounds(WindowGeometry::new(5, 6, 700, 500, true));
        let WindowBounds::Maximized(restored) = maximized else {
            panic!("a maximized launch restores as maximized");
        };
        assert_eq!(f32::from(restored.origin.x), 5.0);
        assert_eq!(f32::from(restored.size.height), 500.0);

        let windowed = restore_bounds(WindowGeometry::new(1, 2, 300, 200, false));
        assert_eq!(
            windowed,
            WindowBounds::Windowed(bounds(1.0, 2.0, 300.0, 200.0))
        );
    }
}
