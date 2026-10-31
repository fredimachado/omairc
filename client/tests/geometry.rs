//! Window close must persist geometry and still launch a ready installer. GPUI
//! stores one `on_window_should_close` callback, so the client chains both effects in
//! `windowstate::attach` instead of registering them separately in `app.rs`.

mod common;

use common::{code_only, crate_root, read_code};

#[test]
fn main_chains_geometry_save_with_update_launch_on_close() {
    let app = read_code(&crate_root().join("src/app.rs"));

    assert!(
        app.contains("windowstate::attach") && app.contains("launch_update_on_close"),
        "app.rs must wire geometry attach and the update launch together"
    );
    assert!(
        !app.contains("cx.on_app_quit"),
        "Ctrl+Q routing belongs in windowstate::attach, not app.rs"
    );
    let attach = app
        .find("windowstate::attach")
        .expect("windowstate::attach in app.rs");
    let before_attach = &app[..attach];
    assert!(
        !before_attach.contains("launch_update_on_close"),
        "app.rs must not register update launch before attach replaces the close handler"
    );
    let attach_tail = &app[attach..];
    assert!(
        attach_tail.contains("launch_update_on_close"),
        "app.rs must pass launch_update_on_close through attach's close hook"
    );
}

#[test]
fn geometry_close_handler_source_orders_save_before_also() {
    let windowstate = code_only(
        &std::fs::read_to_string(crate_root().join("src/desktop/windowstate.rs"))
            .expect("windowstate.rs"),
    );
    let run_close = windowstate
        .split("impl WindowCloseWork")
        .nth(1)
        .and_then(|tail| tail.split("pub fn run_close_handlers").next())
        .expect("WindowCloseWork::run");
    let save = run_close
        .find("keeper.save(window_bounds)")
        .expect("geometry save in close work");
    let also = run_close.find("also()").expect("also hook in close work");
    assert!(
        save < also,
        "geometry must be saved before the chained close hook runs"
    );
}
