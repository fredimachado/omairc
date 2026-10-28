//! Bottom-bar alignment and file-drag frame gates.

use gpui::{px, BorrowAppContext, FileDropEvent, Point};

use crate::model::theme::Theme;
use crate::model::Style;
use crate::ui::center::drag_frame_probe;
use crate::ui::BOTTOM_BAR_HEIGHT;
use crate::util::{scaled_px, style, OmaircStyleGlobal};

use super::fixture::{self, OMARCHY};

#[gpui::test]
fn bottom_bar_rules_share_one_row(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    assert_bottom_bars_align(&mut ui, 1.0);

    set_text_scale(&mut ui, 1.5);
    assert_bottom_bars_align(&mut ui, 1.5);
}

#[gpui::test]
fn composer_shell_is_centred_in_the_strip(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    assert_shell_centred(&mut ui);

    set_text_scale(&mut ui, 1.5);
    assert_shell_centred(&mut ui);
}

#[gpui::test]
fn file_drag_frame_mounts_only_when_a_host_is_offered(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    assert!(
        ui.cx.debug_bounds("center-file-drag-frame").is_none(),
        "demo seed has no FILEHOST, so the frame stays unmounted"
    );
    assert!(
        !ui.shell.read_with(ui.cx, |shell, _| shell.file_offered()),
        "shell file_offered matches the unmounted frame"
    );

    offer_file_host(&mut ui);
    let frame = ui
        .cx
        .debug_bounds("center-file-drag-frame")
        .expect("FILEHOST mounts the absolute drag frame");
    let strip = ui
        .cx
        .debug_bounds("composer-strip")
        .expect("composer strip");
    assert!(
        frame.origin.y <= strip.origin.y,
        "frame covers the centre column down through the composer"
    );
    assert!(
        ui.shell.read_with(ui.cx, |shell, _| shell.file_offered()),
        "shell keeps the file-offer gate open with a host and no Find/Connect"
    );

    // Find mode closes the same file-offer gate Connect does. gpui 0.2.2 keeps
    // prior debug_bounds entries across frames, so the gate is checked on the
    // shared Shell::file_offered method render uses rather than on the selector
    // disappearing.
    ui.keys("ctrl-f");
    assert!(
        ui.shell.read_with(ui.cx, |shell, _| shell.find.is_some()),
        "Ctrl+F enters find"
    );
    assert!(
        !ui.shell.read_with(ui.cx, |shell, _| shell.file_offered()),
        "Find closes the file-offer gate that mounts the frame"
    );
}

#[gpui::test]
fn file_drag_frame_accents_only_over_the_centre_column(cx: &mut gpui::TestAppContext) {
    // The frame's accent comes from group_drag_over, which gpui only applies when
    // the frame has a hitbox. A paint-time drag_over probe on the frame notes the
    // same gate (fixture cannot read computed border colours from the scene).
    let mut ui = fixture::open(cx);
    offer_file_host(&mut ui);
    let strip = ui
        .cx
        .debug_bounds("composer-strip")
        .expect("composer strip");
    let footer = ui
        .cx
        .debug_bounds("identity-footer")
        .expect("identity footer");
    let over_centre = Point::new(
        strip.origin.x + strip.size.width / 2.,
        strip.origin.y - px(40.),
    );
    let over_sidebar = Point::new(
        footer.origin.x + footer.size.width / 2.,
        footer.origin.y + footer.size.height / 2.,
    );

    drag_frame_probe::reset();
    ui.cx.simulate_event(FileDropEvent::Entered {
        position: over_centre,
        paths: gpui::ExternalPaths::default(),
    });
    ui.cx.simulate_event(FileDropEvent::Pending {
        position: over_centre,
    });
    ui.settle();
    assert!(
        ui.cx.update(|_, cx| cx.has_active_drag()),
        "Entered starts an ExternalPaths drag"
    );
    assert!(
        drag_frame_probe::active(),
        "frame drag-over (and group_drag_over accent) must run while the drag is over the centre"
    );

    drag_frame_probe::reset();
    ui.cx.simulate_event(FileDropEvent::Pending {
        position: over_sidebar,
    });
    ui.settle();
    assert!(
        ui.cx.update(|_, cx| cx.has_active_drag()),
        "drag stays active over the sidebar"
    );
    assert!(
        !drag_frame_probe::active(),
        "frame must not stay accented once the drag leaves the centre column"
    );

    // ExternalPaths's paths field is crate-private, so Submit carries an empty
    // list; the column's on_drop still runs and clears the active drag.
    ui.cx.simulate_event(FileDropEvent::Submit {
        position: over_centre,
    });
    ui.settle();
    assert!(
        !ui.cx.update(|_, cx| cx.has_active_drag()),
        "Submit clears the active drag after the column's on_drop runs"
    );
}

fn assert_bottom_bars_align(ui: &mut fixture::SeededShell<'_>, text_scale: f32) {
    let expected_h = ui.cx.update(|_, cx| {
        let app_style = style(cx);
        assert!(
            (app_style.text_scale - text_scale).abs() < f32::EPSILON,
            "expected text scale {text_scale}, got {}",
            app_style.text_scale
        );
        scaled_px(&app_style, BOTTOM_BAR_HEIGHT)
    });

    let footer = ui
        .cx
        .debug_bounds("identity-footer")
        .expect("identity footer bounds");
    let strip = ui
        .cx
        .debug_bounds("composer-strip")
        .expect("composer strip bounds");

    assert_eq!(
        footer.origin.y, strip.origin.y,
        "footer and composer top rules must share one row at scale {text_scale}"
    );
    assert_eq!(
        footer.size.height, expected_h,
        "footer height at scale {text_scale}"
    );
    assert_eq!(
        strip.size.height, expected_h,
        "composer strip height at scale {text_scale}"
    );
}

fn assert_shell_centred(ui: &mut fixture::SeededShell<'_>) {
    let strip = ui
        .cx
        .debug_bounds("composer-strip")
        .expect("composer strip");
    let shell = ui
        .cx
        .debug_bounds("composer-shell")
        .expect("composer shell");
    let above = shell.origin.y - strip.origin.y;
    let below = (strip.origin.y + strip.size.height) - (shell.origin.y + shell.size.height);
    let delta = f32::from(above) - f32::from(below);
    assert!(
        delta.abs() < 0.01,
        "shell gaps must match: above={above:?} below={below:?}"
    );
}

fn set_text_scale(ui: &mut fixture::SeededShell<'_>, scale: f32) {
    ui.cx.update(|_, cx| {
        cx.update_global::<OmaircStyleGlobal, _>(|global, _| {
            let palette = global.style.color_palette();
            global.style = Style::with_color_palette(Theme::defaults_dark(), scale, palette);
        });
    });
    ui.settle();
}

fn offer_file_host(ui: &mut fixture::SeededShell<'_>) {
    ui.inject(
        OMARCHY,
        ":server 005 fred soju.im/FILEHOST=https://uploads.example/upload :are supported by this server",
    );
    assert!(
        ui.shell
            .read_with(ui.cx, |shell, _| shell.controller.file_host().is_some()),
        "FILEHOST must open the upload gate"
    );
}
