use std::time::{Duration, Instant};

/// Timings the `--bench` run records. Idle unless `--bench` was passed.
pub struct BenchState {
    pub enabled: bool,
    pub started: Instant,
    pub first_frame: Option<Duration>,
    pub scroll_frames: u32,
    pub scroll_start: Option<Instant>,
    pub reflow_start: Option<Instant>,
    pub reflow_done: Option<Duration>,
    pub append_latency: Option<Duration>,
    pub finished: bool,
}

impl Default for BenchState {
    fn default() -> Self {
        Self {
            enabled: false,
            started: Instant::now(),
            first_frame: None,
            scroll_frames: 0,
            scroll_start: None,
            reflow_start: None,
            reflow_done: None,
            append_latency: None,
            finished: false,
        }
    }
}
