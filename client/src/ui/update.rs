//! The Windows installer update's gpui wiring. The rules are in
//! `model::update_check`, and the file and process work is in `desktop::installer`.
//! This module starts each background job on the executor and applies its result to
//! the window's shared `UpdateCheck` entity. The About sheet and the footer observe
//! that entity. Qt equivalents: the About sheet's `activateUpdateStatus`, the footer's
//! `activateVersionControl`, and the window's `onClosing`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use gpui::{App, Entity};

use crate::desktop::httpfetch::UreqFetch;
use crate::desktop::installer::{
    default_updates_dir, download_setup, installer_matches, remove_installer, ProcessLauncher,
};
use crate::model::update_check::{
    check_latest, LaunchOutcome, StatusAction, UpdateCheck, LATEST_RELEASE_API_URL,
};
use crate::util::open_web_link;

/// How often the download reports its byte count to the status line.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(200);

/// Runs the status line's action. A release page opens in the browser, a
/// verified setup starts downloading, and a ready installer starts.
pub fn activate_status(update: &Entity<UpdateCheck>, cx: &mut App) {
    let action = update.read(cx).action();
    match action {
        StatusAction::None => {}
        StatusAction::OpenPage(url) => {
            open_web_link(cx, &url);
        }
        StatusAction::Download => start_download(update, cx),
        StatusAction::Restart => {
            launch_installer(update, cx);
        }
    }
}

/// Check for Updates. The request runs on the background executor, so the window
/// stays live. A check already running does nothing. A reply after the sheet
/// closed is ignored, because closing cancels the check.
pub fn start_check(update: &Entity<UpdateCheck>, cx: &mut App) {
    let (ticket, stale) = update.update(cx, |state, cx| {
        let stale = state.installer_path().map(Path::to_path_buf);
        let ticket = state.begin_check();
        cx.notify();
        (ticket, stale)
    });
    let Some(ticket) = ticket else {
        return;
    };
    // A new check forgets the downloaded installer, as Qt does, so its file goes too.
    if let Some(path) = stale {
        remove_installer(&path);
    }
    let current = update.read(cx).current().to_string();
    let weak = update.downgrade();
    cx.spawn(async move |cx| {
        let release = cx
            .background_executor()
            .spawn(async move { check_latest(&UreqFetch, &current, LATEST_RELEASE_API_URL) })
            .await;
        weak.update(cx, |state, cx| {
            state.finish_check(ticket, release);
            cx.notify();
        })
        .ok();
    })
    .detach();
}

/// Downloads the verified setup in the background. A timer reports the byte count
/// while it runs. A result the state no longer wants (a newer check started) is
/// removed from disk, as Qt discards a stale reply.
pub fn start_download(update: &Entity<UpdateCheck>, cx: &mut App) {
    let ticket = update.update(cx, |state, cx| {
        let ticket = state.start_download();
        cx.notify();
        ticket
    });
    let Some(ticket) = ticket else {
        return;
    };
    let directory = default_updates_dir();
    let written = Arc::new(AtomicU64::new(0));
    let finished = Arc::new(AtomicBool::new(false));
    let background = {
        let (written, finished, ticket) = (written.clone(), finished.clone(), ticket.clone());
        cx.background_executor().spawn(async move {
            let result = download_setup(
                &UreqFetch,
                &ticket.version,
                &ticket.setup,
                &directory,
                &mut |bytes| written.store(bytes, Ordering::Relaxed),
            );
            finished.store(true, Ordering::Release);
            result
        })
    };
    let weak = update.downgrade();
    cx.spawn(async move |cx| {
        while !finished.load(Ordering::Acquire) {
            cx.background_executor().timer(PROGRESS_INTERVAL).await;
            let bytes = written.load(Ordering::Relaxed);
            weak.update(cx, |state, cx| {
                if state.report_progress(&ticket, bytes) {
                    cx.notify();
                }
            })
            .ok();
        }
        let path: Option<PathBuf> = background.await.ok();
        let accepted = weak
            .update(cx, |state, cx| {
                let accepted = state.commit_download(&ticket, path.clone());
                cx.notify();
                accepted
            })
            .unwrap_or(false);
        if !accepted {
            if let Some(path) = path {
                remove_installer(&path);
            }
        }
    })
    .detach();
}

/// The footer's `Restart to update` and the status line's restart. Starts the
/// verified installer once per session. Qt `launchInstaller`.
pub fn launch_installer(update: &Entity<UpdateCheck>, cx: &mut App) -> Option<LaunchOutcome> {
    launch_with(update, cx, |state| {
        Some(state.launch_installer(&ProcessLauncher, &installer_matches))
    })
}

/// The first window close. Starts the installer if it is ready and nothing has
/// launched it yet. Qt `onClosing`.
pub fn launch_on_close(update: &Entity<UpdateCheck>, cx: &mut App) -> Option<LaunchOutcome> {
    launch_with(update, cx, |state| {
        state.launch_on_close(&ProcessLauncher, &installer_matches)
    })
}

/// Runs a launch on the shared state and notifies its observers. A verification
/// failure marks the download failed, and the file it names is removed here, since
/// the state only forgets its path.
fn launch_with(
    update: &Entity<UpdateCheck>,
    cx: &mut App,
    launch: impl FnOnce(&mut UpdateCheck) -> Option<LaunchOutcome>,
) -> Option<LaunchOutcome> {
    let stale = update.read(cx).installer_path().map(Path::to_path_buf);
    let outcome = update.update(cx, |state, cx| {
        let outcome = launch(state);
        cx.notify();
        outcome
    });
    if outcome == Some(LaunchOutcome::VerificationFailed) {
        if let Some(path) = stale {
            remove_installer(&path);
        }
    }
    outcome
}
