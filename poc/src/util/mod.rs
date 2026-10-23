use gpui::{Hsla, px, Pixels, Rgba};
use crate::model::Style;

pub mod secret;

pub use secret::Secret;

pub const FONT_FAMILY: &str = "iA Writer Mono S";

pub struct OmaircStyleGlobal {
    pub style: Style,
}

impl gpui::Global for OmaircStyleGlobal {}

pub fn to_gpui(color: crate::model::Color) -> Hsla {
    Hsla::from(Rgba {
        r: color.r,
        g: color.g,
        b: color.b,
        a: color.a,
    })
}

pub fn scaled_px(style: &Style, value: f32) -> Pixels {
    px(style.scaled(value) as f32)
}

pub fn style(cx: &gpui::App) -> Style {
    cx.global::<OmaircStyleGlobal>().style.clone()
}

/// Opens an http(s) link in the browser. Anything else is refused, the same
/// policy the transcript and the Link sheet apply. Returns whether it opened.
pub fn open_web_link(cx: &gpui::App, url: &str) -> bool {
    if !crate::model::format::is_allowed_http_url(url) {
        return false;
    }
    cx.open_url(url);
    true
}

/// Scratch folders for tests, so no test touches the real config or state.
#[cfg(test)]
pub mod testdir {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    /// A unique directory under the system temp dir, removed on drop.
    pub struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        pub fn new(label: &str) -> Self {
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or_default();
            let path = std::env::temp_dir().join(format!(
                "omairc-poc-test-{label}-{}-{n}-{stamp}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).expect("create test dir");
            Self { path }
        }

        pub fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}
