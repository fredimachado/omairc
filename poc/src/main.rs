#![cfg_attr(windows, windows_subsystem = "windows")]

mod assets;
mod controller;
mod demo;
mod desktop;
mod irc;
mod model;
mod net;
mod session;
mod storage;
mod ui;
mod util;
mod version;

use std::borrow::Cow;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

const BENCH_LOG: &str = "results/gpui-bench-stderr.txt";

use clap::Parser;
use gpui::{
    px, size, AnyWindowHandle, App, Application, Entity, WindowOptions, prelude::*,
};
use crate::desktop::systemtheme::SystemAppearance;
use crate::desktop::windowstate::{self, GeometryKeeper};
use crate::model::{Style, Theme, ThemeWatcher};
use crate::net::NetHub;
use crate::storage::{geometry, paths};
use ui::shell::{BenchState, Shell, CONTROLLER_TICK};

use crate::ui::input::bind_input_keys;
use crate::ui::shell::bind_shell_keys;
use crate::ui::transcript::TranscriptView;
use crate::util::OmaircStyleGlobal;

#[derive(Parser, Debug)]
#[command(
    name = "omairc-poc-gpui",
    about = "Omairc GPUI visual shell PoC",
    version = version::version()
)]
struct Args {
    #[arg(long)]
    theme: Option<PathBuf>,
    #[arg(long)]
    scale: Option<f32>,
    #[arg(long)]
    lines: Option<usize>,
    #[arg(long)]
    bench: bool,
    /// Run the seeded in-process demo server: the controller starts on its networks.
    #[arg(long)]
    demo_server: bool,
    /// Internal: started by a Windows toast activation (`LocalServer32` appends it).
    #[arg(long = "Embedding", hide = true)]
    embedding: bool,
}

/// Windows GUI-subsystem builds have no console. Attach to the parent's console
/// (cmd/PowerShell) so `--help`, `--version`, and stdout/stderr still show up
/// there. Skipped when stdout or stderr is already redirected to a pipe or file.
#[cfg(windows)]
mod console {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;

    type Handle = *mut c_void;
    const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
    const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
    const STD_ERROR_HANDLE: u32 = -12i32 as u32;
    const FILE_TYPE_DISK: u32 = 0x0001;
    const FILE_TYPE_PIPE: u32 = 0x0003;
    const GENERIC_READ_WRITE: u32 = 0xC000_0000;
    const FILE_SHARE_READ_WRITE: u32 = 0x0000_0003;
    const OPEN_EXISTING: u32 = 3;

    extern "system" {
        fn AttachConsole(process_id: u32) -> i32;
        fn GetStdHandle(which: u32) -> Handle;
        fn GetFileType(handle: Handle) -> u32;
        fn SetStdHandle(which: u32, handle: Handle) -> i32;
        fn CreateFileW(
            name: *const u16,
            access: u32,
            share: u32,
            security: *const c_void,
            disposition: u32,
            flags: u32,
            template: Handle,
        ) -> Handle;
    }

    fn is_invalid(handle: Handle) -> bool {
        handle.is_null() || handle as isize == -1
    }

    fn is_redirected(which: u32) -> bool {
        let handle = unsafe { GetStdHandle(which) };
        if is_invalid(handle) {
            return false;
        }
        matches!(
            unsafe { GetFileType(handle) },
            FILE_TYPE_DISK | FILE_TYPE_PIPE
        )
    }

    pub fn attach_parent_console() {
        if is_redirected(STD_OUTPUT_HANDLE) || is_redirected(STD_ERROR_HANDLE) {
            return;
        }
        if unsafe { AttachConsole(ATTACH_PARENT_PROCESS) } == 0 {
            return;
        }
        let conout: Vec<u16> = std::ffi::OsStr::new("CONOUT$")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let handle = unsafe {
            CreateFileW(
                conout.as_ptr(),
                GENERIC_READ_WRITE,
                FILE_SHARE_READ_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        if !is_invalid(handle) {
            unsafe {
                SetStdHandle(STD_OUTPUT_HANDLE, handle);
                SetStdHandle(STD_ERROR_HANDLE, handle);
            }
        }
    }
}

fn main() {
    #[cfg(windows)]
    console::attach_parent_console();

    // A control command talks to the running window and exits before clap or gpui start.
    let raw_args: Vec<String> = normalize_windows_args(std::env::args_os().skip(1));
    if desktop::cli::is_control_invocation(&raw_args) {
        std::process::exit(desktop::cli::run(&raw_args));
    }

    let args = Args::parse();
    let theme_path = args.theme.clone();
    let scale = args
        .scale
        .or_else(desktop::textscale::read_portal_text_scale)
        .unwrap_or(1.0);
    let cli_scale_override = args.scale.is_some();
    let lines = args.lines;
    let bench = args.bench;
    let demo_server = args.demo_server;
    let _embedding = args.embedding;

    // One window per user: a second launch raises the first and exits, as Qt's guard does.
    let ipc = match desktop::singleinstance::acquire_for_app() {
        Ok(desktop::singleinstance::Startup::Primary(server)) => Some(server),
        Ok(desktop::singleinstance::Startup::Unguarded) => None,
        Ok(desktop::singleinstance::Startup::Secondary) => return,
        Err(error) => {
            eprintln!("omairc-poc-gpui: {error}");
            std::process::exit(1);
        }
    };

    // Demo mode keeps no log file, as the Qt client does.
    if !demo_server {
        let _ = desktop::filelog::install(paths::log_path());
    }

    Application::new().with_assets(assets::OmaircAssets).run(move |cx: &mut App| {
        cx.text_system()
            .add_fonts(vec![Cow::Borrowed(
                include_bytes!("../../fonts/iAWriterMonoS-Regular.ttf") as &[u8],
            )])
            .expect("regular font");
        cx.text_system()
            .add_fonts(vec![Cow::Borrowed(
                include_bytes!("../../fonts/iAWriterMonoS-Bold.ttf") as &[u8],
            )])
            .expect("bold font");

        let system = SystemAppearance::from_window(cx.window_appearance());
        cx.set_global(system);
        let theme = Theme::load(theme_path.as_deref(), system.dark).expect("theme load");
        cx.set_global(OmaircStyleGlobal {
            style: Style::new(theme, scale),
        });

        bind_input_keys(cx);
        bind_shell_keys(cx);

        // Demo and bench runs restore the window but never save over the user's
        // geometry, as the Qt client's demo mode is ephemeral.
        let settings_path = paths::settings_path();
        let stored = settings_path.as_deref().and_then(geometry::load_from);
        let keeper = settings_path
            .filter(|_| !demo_server && !bench)
            .map(|path| GeometryKeeper::new(path, stored));
        let window_bounds = windowstate::initial_bounds(stored, cx);
        // The socket hub runs its net thread for a live launch. The demo's peers are in-process,
        // so the demo has none.
        let net = (!demo_server).then(|| Rc::new(NetHub::new()));
        let (activation_tx, activation_rx) = async_channel::unbounded();
        #[cfg(windows)]
        let pending_activation = desktop::notifier::activation_from_args(&raw_args);
        #[cfg(not(windows))]
        let pending_activation: Option<desktop::notifier::Activation> = None;
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(window_bounds),
                    window_min_size: Some(size(px(760.), px(540.))),
                    titlebar: Some(gpui::TitlebarOptions {
                        title: Some("Omairc GPUI PoC".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                |window, cx| {
                    let shell = cx.new(|cx| {
                        Shell::new(cx, lines, bench, demo_server, net.clone(), activation_tx.clone())
                    });
                    if let Some(activation) = pending_activation {
                        let _ = activation_tx.send_blocking(activation);
                    }
                    start_notification_activation(
                        shell.clone(),
                        window.window_handle(),
                        activation_rx,
                        cx,
                    );
                    if let Some(hub) = &net {
                        start_net_delivery(shell.clone(), hub, cx);
                    }
                    // The first window close starts a ready installer, as Qt's onClosing does.
                    let close_shell = shell.clone();
                    window.on_window_should_close(cx, move |_, cx| {
                        close_shell.update(cx, |shell, cx| shell.launch_update_on_close(cx));
                        true
                    });
                    let transcript = shell.read(cx).transcript.clone();
                    if let Some(keeper) = &keeper {
                        windowstate::attach(keeper, &shell, window, cx);
                    }
                    if let Some(server) = ipc {
                        desktop::ipchandler::serve(shell.clone(), window.window_handle(), server, cx);
                    }
                    if bench {
                        run_bench(shell.clone(), window.window_handle(), cx);
                    }
                    start_typing_pulse(shell.clone(), cx);
                    start_controller_tick(shell.clone(), cx);
                    start_caret_blink(shell.clone(), cx);
                    start_drag_autoscroll(transcript.clone(), cx);

                    if !cli_scale_override {
                        if let Some(portal_scale) =
                            desktop::textscale::start_portal_text_scale_watcher()
                        {
                            let shell_portal = shell.clone();
                            let transcript_portal = transcript.clone();
                            cx.spawn(async move |cx| {
                                while let Ok(scale) = portal_scale.recv().await {
                                    cx.update(|cx| {
                                        apply_portal_text_scale(
                                            scale,
                                            &shell_portal,
                                            &transcript_portal,
                                            cx,
                                        );
                                    })
                                    .ok();
                                }
                            })
                            .detach();
                        }
                    }

                    // The system light or dark mode can change while the app runs.
                    let appearance_path = theme_path.clone();
                    let shell_appearance = shell.clone();
                    let transcript_appearance = transcript.clone();
                    window
                        .observe_window_appearance(move |window, cx| {
                            cx.set_global(SystemAppearance::from_window(window.appearance()));
                            refresh_theme(
                                cx,
                                appearance_path.as_deref(),
                                &shell_appearance,
                                &transcript_appearance,
                            );
                        })
                        .detach();

                    let watch_path = theme_path.clone();
                    let shell_watch = shell.clone();
                    let transcript_watch = transcript;
                    let watcher =
                        ThemeWatcher::start(theme_path.as_deref()).expect("theme watcher");
                    cx.spawn(async move |cx| {
                        loop {
                            while watcher.receiver().try_recv().is_ok() {
                                let path = watch_path.clone();
                                cx.update(|cx| {
                                    refresh_theme(
                                        cx,
                                        path.as_deref(),
                                        &shell_watch,
                                        &transcript_watch,
                                    );
                                })
                                .ok();
                            }
                            cx.background_executor()
                                .timer(Duration::from_millis(250))
                                .await;
                        }
                    })
                    .detach();

                    shell
                },
            )
            .expect("open window");

        window
            .update(cx, |shell, window, cx| {
                shell.focus_on_launch(window, cx);
                cx.activate(true);
            })
            .ok();
    });
}

/// Re-reads the palette for the current system appearance and repaints the shell.
/// Runs on a `colors.toml` change and on a system light or dark switch.
fn apply_portal_text_scale(
    scale: f32,
    shell: &Entity<Shell>,
    transcript: &Entity<TranscriptView>,
    cx: &mut App,
) {
    cx.update_global::<OmaircStyleGlobal, _>(|global, _| {
        global.style.text_scale = scale;
    });
    transcript.update(cx, |t, cx| {
        t.invalidate_all();
        cx.notify();
    });
    shell.update(cx, |_, cx| cx.notify());
    cx.refresh_windows();
}

fn refresh_theme(
    cx: &mut App,
    theme_path: Option<&Path>,
    shell: &Entity<Shell>,
    transcript: &Entity<TranscriptView>,
) {
    let dark = cx.global::<SystemAppearance>().dark;
    let scale = cx.global::<OmaircStyleGlobal>().style.text_scale;
    let theme = Theme::load(theme_path, dark).unwrap_or_else(|_| Theme::defaults(dark));
    cx.update_global::<OmaircStyleGlobal, _>(|global, _| {
        global.style = Style::new(theme, scale);
    });
    transcript.update(cx, |t, cx| {
        t.invalidate_all();
        cx.notify();
    });
    shell.update(cx, |_, cx| cx.notify());
    cx.refresh_windows();
}

#[cfg(windows)]
fn normalize_windows_args(args: impl Iterator<Item = std::ffi::OsString>) -> Vec<String> {
    args.map(|arg| {
        let text = arg.to_string_lossy();
        if text == "-Embedding" {
            "--Embedding".to_owned()
        } else {
            text.into_owned()
        }
    })
    .collect()
}

#[cfg(not(windows))]
fn normalize_windows_args(args: impl Iterator<Item = std::ffi::OsString>) -> Vec<String> {
    args.map(|arg| arg.to_string_lossy().into_owned()).collect()
}

fn start_notification_activation(
    shell: Entity<Shell>,
    window: AnyWindowHandle,
    activation_rx: async_channel::Receiver<desktop::notifier::Activation>,
    cx: &mut App,
) {
    cx.spawn(async move |cx| {
        while let Ok(activation) = activation_rx.recv().await {
            cx.update_window(window, |_, window, cx| {
                shell.update(cx, |shell, cx| shell.activate_notification(activation, window, cx));
            })
            .ok();
        }
    })
    .detach();
}

fn start_typing_pulse(shell: Entity<Shell>, cx: &mut App) {
    cx.spawn(async move |cx| {
        loop {
            cx.background_executor()
                .timer(Duration::from_millis(320))
                .await;
            shell
                .update(cx, |shell, cx| {
                    shell.typing_phase = shell.typing_phase.wrapping_add(1);
                    // The transcript's typing dots pulse with the sidebar's.
                    let phase = shell.typing_phase;
                    shell
                        .transcript
                        .update(cx, |t, cx| t.set_typing_phase(phase, cx));
                    cx.notify();
                })
                .ok();
        }
    })
    .detach();
}

/// Drives the controller's timers and repaints what they change. The clock is polled
/// (`RealClock`), so the loop ticks on an interval. A tick with nothing due costs nothing.
fn start_controller_tick(shell: Entity<Shell>, cx: &mut App) {
    cx.spawn(async move |cx| {
        loop {
            cx.background_executor().timer(CONTROLLER_TICK).await;
            shell.update(cx, |shell, cx| shell.tick(cx)).ok();
        }
    })
    .detach();
}

/// Hands the net thread's results to the shell as each one arrives, so a reply lands without
/// waiting for the next tick. The tick keeps to the timers the clock runs.
fn start_net_delivery(shell: Entity<Shell>, hub: &NetHub, cx: &mut App) {
    let results = hub.receiver();
    cx.spawn(async move |cx| {
        while let Ok(delivery) = results.recv().await {
            shell
                .update(cx, |shell, cx| shell.deliver_net(delivery, cx))
                .ok();
        }
    })
    .detach();
}

/// Repaints the composer at each caret blink boundary, so the caret flips on time
/// rather than at the next typing-pulse repaint.
fn start_caret_blink(shell: Entity<Shell>, cx: &mut App) {
    cx.spawn(async move |cx| {
        loop {
            let wait = shell
                .update(cx, |shell, cx| shell.composer.read(cx).caret_flip_in())
                .unwrap_or(Duration::from_millis(530));
            cx.background_executor().timer(wait).await;
            shell
                .update(cx, |shell, cx| {
                    shell.composer.update(cx, |_, cx| cx.notify());
                })
                .ok();
        }
    })
    .detach();
}

fn start_drag_autoscroll(transcript: Entity<TranscriptView>, cx: &mut App) {
    cx.spawn(async move |cx| {
        loop {
            cx.background_executor()
                .timer(Duration::from_millis(50))
                .await;
            transcript
                .update(cx, |t, cx| {
                    if t.is_dragging() {
                        let app_style = cx.global::<OmaircStyleGlobal>().style.clone();
                        t.tick_drag_autoscroll(&app_style);
                        cx.notify();
                    }
                })
                .ok();
        }
    })
    .detach();
}

fn bench_log(line: &str) {
    eprintln!("{line}");
    let _ = (|| {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(BENCH_LOG)?;
        writeln!(file, "{line}")?;
        file.flush()?;
        Ok::<(), std::io::Error>(())
    })();
}

fn run_bench(shell: Entity<Shell>, window: AnyWindowHandle, cx: &mut App) {
    let _ = std::fs::create_dir_all("results");
    let _ = std::fs::write(BENCH_LOG, "");
    let shell_weak = shell.downgrade();
    cx.spawn(async move |cx| {
        cx.background_executor().timer(Duration::from_millis(500)).await;
        let row_count = shell_weak
            .update(cx, |s, cx| s.transcript.read(cx).rows().len())
            .ok()
            .unwrap_or(0);
        if row_count == 0 {
            return;
        }

        const SCROLL_STEPS: usize = 200;
        const FRAME_MS: u64 = 16;

        shell_weak
            .update(cx, |shell, _| {
                shell.bench.scroll_start = Some(Instant::now());
                shell.bench.scroll_frames = 0;
            })
            .ok();

        for step in 0..SCROLL_STEPS {
            let row_ix = (step * row_count / SCROLL_STEPS).min(row_count.saturating_sub(1));
            shell_weak
                .update(cx, |shell, cx| {
                    shell.transcript.update(cx, |t, _| t.scroll_to_row(row_ix));
                    shell.bench.scroll_frames += 1;
                    cx.notify();
                })
                .ok();
            cx.background_executor()
                .timer(Duration::from_millis(FRAME_MS))
                .await;
        }

        shell_weak
            .update(cx, |shell, _| {
                if let Some(start) = shell.bench.scroll_start.take() {
                    let secs = start.elapsed().as_secs_f64().max(0.001);
                    let fps = shell.bench.scroll_frames as f64 / secs;
                    bench_log(&format!(
                        "bench scroll fps: {fps:.1} ({} frames, {} rows open)",
                        shell.bench.scroll_frames, row_count
                    ));
                }
            })
            .ok();

        shell_weak
            .update(cx, |shell, _| {
                shell.bench.reflow_start = Some(Instant::now());
            })
            .ok();
        cx.update(|app| {
            if let Some(window) = app.active_window() {
                window
                    .update(app, |_, window, _| {
                        window.resize(size(px(960.), px(640.)));
                    })
                    .ok();
            }
        })
        .ok();
        cx.background_executor()
            .timer(Duration::from_millis(300))
            .await;
        shell_weak
            .update(cx, |shell, _| {
                if let Some(start) = shell.bench.reflow_start.take() {
                    shell.bench.reflow_done = Some(start.elapsed());
                }
            })
            .ok();

        shell_weak
            .update(cx, |shell, cx| {
                shell.composer.update(cx, |c, cx| {
                    c.set_text("bench ping");
                    cx.notify();
                });
            })
            .ok();
        // A command needs the window, as `/list` opens a sheet that takes focus.
        cx.update_window(window, |_, window, cx| {
            shell_weak
                .update(cx, |shell, cx| shell.send_message(window, cx))
                .ok();
        })
        .ok();

        cx.background_executor()
            .timer(Duration::from_millis(200))
            .await;

        shell_weak
            .update(cx, |shell, _| {
                print_bench_summary(&shell.bench, row_count);
                shell.bench.finished = true;
            })
            .ok();
    })
    .detach();
}

fn print_bench_summary(bench: &BenchState, row_count: usize) {
    bench_log(&format!("bench transcript rows: {row_count}"));
    if let Some(t) = bench.first_frame {
        bench_log(&format!(
            "bench first frame: {:.2} ms",
            t.as_secs_f64() * 1000.0
        ));
    }
    if let Some(t) = bench.reflow_done {
        bench_log(&format!("bench reflow: {:.2} ms", t.as_secs_f64() * 1000.0));
    }
    if let Some(t) = bench.append_latency {
        bench_log(&format!(
            "bench append latency: {:.2} ms",
            t.as_secs_f64() * 1000.0
        ));
    }
}
