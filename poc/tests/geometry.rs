//! Window close must persist geometry and still launch a ready installer. GPUI
//! stores one `on_window_should_close` callback, so the PoC chains both effects in
//! `windowstate::attach` instead of registering them separately in `main.rs`.

mod common;

use common::{code_only, crate_root, read_code};

#[test]
fn main_chains_geometry_save_with_update_launch_on_close() {
    let main = read_code(&crate_root().join("src/main.rs"));
    let windowstate = read_code(&crate_root().join("src/desktop/windowstate.rs"));

    assert!(
        main.contains("windowstate::attach")
            && main.contains("launch_update_on_close"),
        "main.rs must wire geometry attach and the update launch together"
    );
    let attach = main
        .find("windowstate::attach")
        .expect("windowstate::attach in main.rs");
    let before_attach = &main[..attach];
    assert!(
        !before_attach.contains("launch_update_on_close"),
        "main.rs must not register update launch before attach replaces the close handler"
    );
    assert!(
        windowstate.contains("run_close_handlers")
            && windowstate.contains("keeper.save(window_bounds)"),
        "windowstate must save geometry before running the chained close hook"
    );
}

#[test]
fn geometry_close_handler_source_orders_save_before_also() {
    let windowstate = code_only(
        &std::fs::read_to_string(crate_root().join("src/desktop/windowstate.rs"))
            .expect("windowstate.rs"),
    );
    let run_close = windowstate
        .split("pub fn run_close_handlers")
        .nth(1)
        .and_then(|tail| tail.split("pub fn attach").next())
        .expect("run_close_handlers");
    let save = run_close
        .find("keeper.save(window_bounds)")
        .expect("geometry save in run_close_handlers");
    let also = run_close
        .find("also()")
        .expect("also hook in run_close_handlers");
    assert!(
        save < also,
        "geometry must be saved before the chained close hook runs"
    );
}
