mod app;
mod convert;
mod fonts;
mod transcript_widget;

use app::App;
use fonts::IA_WRITER;
use iced::{Font, Pixels, Settings, Size};
use shared::{bench_channel, default_omarchy_colors_path, load_world, Theme, ThemeWatcher};
use std::env;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const FONT_REGULAR: &[u8] = include_bytes!("../../../fonts/iAWriterMonoS-Regular.ttf");
const FONT_BOLD: &[u8] = include_bytes!("../../../fonts/iAWriterMonoS-Bold.ttf");

fn main() -> iced::Result {
    let args = parse_args();
    let boot_config = BootConfig {
        theme_path: args.theme,
        scale: args.scale,
        lines: args.lines,
        bench: args.bench,
    };

    iced::application(
        {
            let boot_config = boot_config.clone();
            move || boot_app(&boot_config)
        },
        App::update,
        App::view,
    )
        .subscription(App::subscription)
        .title("Omairc (iced PoC)")
        .settings(Settings {
            fonts: vec![FONT_REGULAR.into(), FONT_BOLD.into()],
            default_font: IA_WRITER,
            default_text_size: Pixels(13.0),
            ..Settings::default()
        })
        .window(iced::window::Settings {
            size: Size::new(1180.0, 760.0),
            ..Default::default()
        })
        .run()
}

struct Args {
    theme: Option<PathBuf>,
    scale: f32,
    lines: usize,
    bench: bool,
}

#[derive(Clone)]
struct BootConfig {
    theme_path: Option<PathBuf>,
    scale: f32,
    lines: usize,
    bench: bool,
}

fn boot_app(config: &BootConfig) -> (App, iced::Task<app::Message>) {
    let theme = Theme::load(config.theme_path.as_deref()).expect("theme load");
    let watcher = ThemeWatcher::start(config.theme_path.as_deref()).expect("theme watcher");
    let watcher = Arc::new(Mutex::new(watcher));
    let mut world = load_world();
    if config.lines > 0 {
        let bench_conv = bench_channel(config.lines);
        for network in &mut world.networks {
            network.conversations.push(bench_conv.clone());
        }
    }
    (
        App::new(
            world,
            theme,
            config.scale,
            config.theme_path.clone(),
            watcher,
            config.bench,
            config.lines,
        ),
        iced::Task::done(app::Message::FirstFrame),
    )
}

fn parse_args() -> Args {
    let mut theme = None;
    let mut scale = 1.0f32;
    let mut lines = 0usize;
    let mut bench = false;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--theme" => theme = args.next().map(PathBuf::from),
            "--scale" => scale = args.next().and_then(|s| s.parse().ok()).unwrap_or(1.0),
            "--lines" => lines = args.next().and_then(|s| s.parse().ok()).unwrap_or(0),
            "--bench" => bench = true,
            "--help" | "-h" => {
                eprintln!(
                    "omairc-poc-iced [--theme PATH] [--scale F] [--lines N] [--bench]\nDefault theme: {}",
                    default_omarchy_colors_path().display()
                );
                std::process::exit(0);
            }
            other => eprintln!("unknown argument: {other}"),
        }
    }
    Args {
        theme,
        scale,
        lines,
        bench,
    }
}
