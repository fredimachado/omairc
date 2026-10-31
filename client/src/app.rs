use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use clap::Parser;
use gpui::{
    px, size, AnyWindowHandle, App, Application, Entity, WindowOptions, prelude::*,
};
use crate::desktop::systemtheme::SystemAppearance;
use crate::desktop::windowstate::{self, GeometryKeeper};
use crate::model::{Style, Theme, ThemeWatcher};
use crate::net::NetHub;
use crate::storage::preferences::PreferenceStore;
use crate::storage::{geometry, paths, Backing};
use crate::ui::shell::{Shell, CONTROLLER_TICK};

use crate::ui::input::bind_input_keys;
use crate::ui::shell::bind_shell_keys;
use crate::ui::transcript::TranscriptView;
use crate::util::OmaircStyleGlobal;

#[derive(Parser, Debug)]
#[command(
    name = "omairc",
    about = "A simple IRC client for Omarchy.",
    version = crate::version::version()
)]
struct Args {
    #[arg(long, hide = true)]
    theme: Option<PathBuf>,
    #[arg(long, hide = true)]
    scale: Option<f32>,
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

pub fn run() {
    #[cfg(windows)]
    console::attach_parent_console();

    // A control command talks to the running window and exits before clap or gpui start.
    let raw_args: Vec<String> = normalize_windows_args(std::env::args_os().skip(1));
    if crate::desktop::cli::is_control_invocation(&raw_args) {
        std::process::exit(crate::desktop::cli::run(&raw_args));
    }

    let args = Args::parse();
    let theme_path = args.theme.clone();
    let scale = args
        .scale
        .or_else(crate::desktop::textscale::read_portal_text_scale)
        .unwrap_or(1.0);
    let cli_scale_override = args.scale.is_some();
    let demo_server = args.demo_server;
    let _embedding = args.embedding;
    // Match Qt `OMAIRC_SMOKE`: non-empty means prove a window opened, then quit.
    let smoke = std::env::var_os("OMAIRC_SMOKE").is_some_and(|value| !value.is_empty());

    // One window per user: a second launch raises the first and exits, as Qt's guard does.
    let ipc = match crate::desktop::singleinstance::acquire_for_app() {
        Ok(crate::desktop::singleinstance::Startup::Primary(server)) => Some(server),
        Ok(crate::desktop::singleinstance::Startup::Unguarded) => None,
        Ok(crate::desktop::singleinstance::Startup::Secondary) => return,
        Err(error) => {
            eprintln!("omairc: {error}");
            std::process::exit(1);
        }
    };

    // Demo mode keeps no log file, as the Qt client does.
    if !demo_server {
        let _ = crate::desktop::filelog::install(paths::log_path());
        // Product builds probe the install layout (and import Windows registry
        // settings once) after the file log is installed so the lines land in
        // omairc.log.
        #[cfg(not(feature = "dev"))]
        crate::storage::migrate::run_at_startup();
    }

    Application::new().with_assets(crate::assets::OmaircAssets).run(move |cx: &mut App| {
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
        let color_palette = PreferenceStore::new(Backing::user()).preferences().palette;
        cx.set_global(OmaircStyleGlobal {
            style: Style::with_color_palette(theme, scale, color_palette),
        });

        bind_input_keys(cx);
        bind_shell_keys(cx);
        #[cfg(target_os = "macos")]
        crate::ui::macos_menu::register_menus(cx);

        // Demo runs restore the window but never save over the user's
        // geometry, as the Qt client's demo mode is ephemeral.
        let settings_path = paths::settings_path();
        let window_path = paths::window_path();
        let stored = geometry::load_stored(&window_path, settings_path.as_deref());
        let keeper = (!demo_server).then(|| GeometryKeeper::new(window_path, stored));
        let window_bounds = windowstate::initial_bounds(stored, cx);
        // The socket hub runs its net thread for a live launch. The demo's peers are in-process
        // so the demo has none.
        let net = (!demo_server).then(|| Rc::new(NetHub::new()));
        let (activation_tx, activation_rx) = async_channel::unbounded();
        #[cfg(windows)]
        let pending_activation = crate::desktop::notifier::activation_from_args(&raw_args);
        #[cfg(not(windows))]
        let pending_activation: Option<crate::desktop::notifier::Activation> = None;
        let window = match cx.open_window(
                WindowOptions {
                    window_bounds: Some(window_bounds),
                    window_min_size: Some(size(px(760.), px(540.))),
                    titlebar: Some(gpui::TitlebarOptions {
                        title: Some("Omairc".into()),
                        ..Default::default()
                    }),
                    // Same id as the Qt client's desktop file name. Omarchy's
                    // compositor rules match it to give the window transparency.
                    app_id: Some("omairc".into()),
                    ..Default::default()
                },
                |window, cx| {
                    let shell = cx.new(|cx| {
                        Shell::new(
                            cx,
                            smoke,
                            demo_server,
                            net.clone(),
                            activation_tx.clone(),
                        )
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
                    let transcript = shell.read(cx).transcript.clone();
                    if let Some(keeper) = &keeper {
                        // Geometry save and a ready installer launch share one close
                        // handler; GPUI replaces an earlier registration.
                        let close_shell = shell.clone();
                        windowstate::attach(keeper, &shell, window, cx, move |_, cx| {
                            close_shell.update(cx, |shell, cx| shell.launch_update_on_close(cx));
                        });
                    } else {
                        let close_shell = shell.clone();
                        window.on_window_should_close(cx, move |_, cx| {
                            close_shell.update(cx, |shell, cx| shell.launch_update_on_close(cx));
                            true
                        });
                    }
                    if let Some(server) = ipc {
                        crate::desktop::ipchandler::serve(shell.clone(), window.window_handle(), server, cx);
                    }
                    start_typing_pulse(shell.clone(), cx);
                    start_controller_tick(shell.clone(), cx);
                    start_caret_blink(shell.clone(), cx);
                    start_unseen_bounce(transcript.clone(), cx);
                    start_drag_autoscroll(transcript.clone(), cx);

                    if !cli_scale_override {
                        if let Some(portal_scale) =
                            crate::desktop::textscale::start_portal_text_scale_watcher()
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
                    let mut watcher =
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
                                watcher
                                    .rearm(watch_path.as_deref())
                                    .expect("re-arm theme watcher");
                            }
                            cx.background_executor()
                                .timer(Duration::from_millis(250))
                                .await;
                        }
                    })
                    .detach();

                    shell
                },
            ) {
            Ok(window) => window,
            Err(_) if smoke => {
                eprintln!("Omairc smoke: could not open a window");
                std::process::exit(1);
            }
            Err(error) => panic!("open window: {error:?}"),
        };

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
    let color_palette = cx.global::<OmaircStyleGlobal>().style.color_palette();
    let theme = Theme::load(theme_path, dark).unwrap_or_else(|_| Theme::defaults(dark));
    cx.update_global::<OmaircStyleGlobal, _>(|global, _| {
        global.style = Style::with_color_palette(theme, scale, color_palette);
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
    activation_rx: async_channel::Receiver<crate::desktop::notifier::Activation>,
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

fn start_unseen_bounce(transcript: Entity<TranscriptView>, cx: &mut App) {
    cx.spawn(async move |cx| {
        loop {
            let wait = transcript
                .update(cx, |view, _| view.unseen_bounce_next_frame())
                .ok()
                .flatten()
                .unwrap_or(Duration::from_millis(200));
            cx.background_executor().timer(wait).await;
            transcript
                .update(cx, |_, cx| cx.notify())
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
                        t.tick_drag_autoscroll(&app_style, cx);
                        cx.notify();
                    }
                })
                .ok();
        }
    })
    .detach();
}
