//! Demo mode must not persist window geometry, and geometry must stay in window.ini.

mod common;

use common::{crate_root, read_code};

#[test]
fn demo_server_skips_geometry_keeper() {
    let app = read_code(&crate_root().join("src/app.rs"));
    assert!(
        app.contains("(!demo_server).then(|| GeometryKeeper::new(window_path, stored))"),
        "demo mode must not construct a geometry keeper"
    );
    assert!(
        app.contains("geometry::load_stored(&window_path, settings_path.as_deref())"),
        "launch must load geometry from window.ini with a one-time conf fallback"
    );
    assert!(
        app.contains("paths::window_path()"),
        "geometry must use the state-dir window.ini path"
    );
}
