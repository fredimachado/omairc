//! Toolkit-agnostic Omairc PoC model: theme, style, mock world, IRC formatting,
//! jump palette, and virtualized transcript selection.

pub mod color;
pub mod format;
pub mod jump;
pub mod mock;
pub mod style;
pub mod theme;
pub mod transcript;

pub use color::Color;
pub use format::{format_irc, plain_irc_text, FormattedText, StyleRun, TextStyle};
pub use jump::{jump_score, jump_targets, JumpKind, JumpTarget, JUMP_RESULT_LIMIT};
pub use mock::{
    bench_channel, load_world, local_send_stamp, outgoing_message, Conversation, Member,
    MockMessage, MockWorld, Network,
};
pub use style::{mix_colors, Style};
pub use theme::{
    default_omarchy_colors_path, parse_colors_toml, Theme, ThemeWatcher,
};
pub use transcript::{
    continues_group, map_pointer, row_from_message, row_from_mock_message, rows_from_messages,
    PointerZone, Pos, Row, RowKind, ScrollAnchor, Selection, Transcript,
};
