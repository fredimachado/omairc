mod format_ui;
mod input;
mod message_row;
mod shell;
mod util;

use std::borrow::Cow;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const BENCH_LOG: &str = "poc/results/gpui-bench-stderr.txt";

use clap::Parser;
use gpui::{
    px, size, App, Application, Bounds, Entity, Focusable, ListOffset, WindowBounds, WindowOptions,
    prelude::*,
};
use shared::{Style, Theme, ThemeWatcher};
use shell::{BenchState, Shell};

use crate::input::bind_input_keys;
use crate::shell::bind_shell_keys;
use crate::util::OmaircStyleGlobal;

#[derive(Parser, Debug)]
#[command(name = "omairc-poc-gpui", about = "Omairc GPUI visual shell PoC")]
struct Args {
    #[arg(long)]
    theme: Option<PathBuf>,
    #[arg(long, default_value_t = 1.0)]
    scale: f32,
    #[arg(long)]
    lines: Option<usize>,
    #[arg(long)]
    bench: bool,
}

fn main() {
    let args = Args::parse();
    let theme_path = args.theme.clone();
    let scale = args.scale;
    let lines = args.lines;
    let bench = args.bench;

    Application::new().run(move |cx: &mut App| {
        cx.text_system()
            .add_fonts(vec![Cow::Borrowed(
                include_bytes!("../../../fonts/iAWriterMonoS-Regular.ttf") as &[u8],
            )])
            .expect("regular font");
        cx.text_system()
            .add_fonts(vec![Cow::Borrowed(
                include_bytes!("../../../fonts/iAWriterMonoS-Bold.ttf") as &[u8],
            )])
            .expect("bold font");

        let theme = Theme::load(theme_path.as_deref()).expect("theme load");
        cx.set_global(OmaircStyleGlobal {
            style: Style::new(theme, scale),
            theme_path: theme_path.clone(),
        });

        bind_input_keys(cx);
        bind_shell_keys(cx);

        let bounds = Bounds::centered(None, size(px(1180.), px(760.)), cx);
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(gpui::TitlebarOptions {
                        title: Some("Omairc GPUI PoC".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                |_, cx| {
                    let shell = cx.new(|cx| Shell::new(cx, lines, bench));
                    if bench {
                        run_bench(shell.clone(), cx);
                    }
                    start_typing_pulse(shell.clone(), cx);
                    start_drag_autoscroll(shell.clone(), cx);

                    let watch_path = theme_path.clone();
                    let shell_watch = shell.clone();
                    let mut watcher =
                        ThemeWatcher::start(theme_path.as_deref()).expect("theme watcher");
                    cx.spawn(async move |cx| {
                        loop {
                            while watcher.receiver().try_recv().is_ok() {
                                let path = watch_path.clone();
                                cx.update(|cx| {
                                    let theme = Theme::load(path.as_deref())
                                        .unwrap_or_else(|_| Theme::defaults_dark());
                                    cx.update_global::<OmaircStyleGlobal, _>(|global, _| {
                                        global.style = Style::new(theme, scale);
                                    });
                                    shell_watch.update(cx, |shell, cx| {
                                        shell.transcript.invalidate_all();
                                        cx.notify();
                                    });
                                    cx.refresh_windows();
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
                window.focus(&shell.composer.read(cx).focus_handle(cx));
                cx.activate(true);
            })
            .ok();
    });
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
                    cx.notify();
                })
                .ok();
        }
    })
    .detach();
}

fn start_drag_autoscroll(shell: Entity<Shell>, cx: &mut App) {
    cx.spawn(async move |cx| {
        loop {
            cx.background_executor()
                .timer(Duration::from_millis(50))
                .await;
            shell
                .update(cx, |shell, cx| {
                    if shell.dragging && shell.drag_autoscroll {
                        shell.list_state.scroll_by(px(8.));
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

fn run_bench(shell: Entity<Shell>, cx: &mut App) {
    let _ = std::fs::write(BENCH_LOG, "");
    let shell_weak = shell.downgrade();
    cx.spawn(async move |cx| {
        cx.background_executor().timer(Duration::from_millis(500)).await;
        let row_count = shell_weak
            .update(cx, |s, _| s.transcript.rows().len())
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
                    shell.transcript.set_pinned_to_bottom(false);
                    shell.list_state.scroll_to(ListOffset {
                        item_ix: row_ix,
                        offset_in_item: px(0.),
                    });
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
        shell_weak
            .update(cx, |shell, cx| {
                shell.send_message(cx);
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
