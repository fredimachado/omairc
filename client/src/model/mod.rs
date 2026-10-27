//! Toolkit-agnostic Omairc model: theme, style, the presentation of controller
//! reads (transcript rows, sidebar and jump walks, titles, Connect's form), IRC
//! formatting for display, chords, and the URL policy. Nothing here owns IRC state:
//! the `controller` does, and the UI reads it through `Controller` snapshots.

pub mod byte_budget;
pub mod channel_list;
pub mod chords;
pub mod color;
pub mod composer;
pub mod connect;
pub mod cursor;
pub mod filter_list;
pub mod filelink;
pub mod find;
pub mod format;
pub mod fuzzy;
pub mod ids;
pub mod jump;
pub mod links;
pub mod mention;
pub mod navigation;
pub mod color_palette;
pub mod nick_pick;
pub mod prefs;
pub mod presence;
pub mod scroll_place;
pub mod shortcuts;
pub mod sidebar;
pub mod slash;
pub mod slash_complete;
pub mod status;
pub mod style;
pub mod text_edit;
pub mod theme;
pub mod title;
pub mod topic_line;
pub mod transcript;
pub mod typing_tail;
pub mod unread;
pub mod update_check;

pub use color::Color;
pub use ids::ConversationRef;
pub use format::{StyleRun, TextStyle};
pub use jump::{jump_targets, JumpTarget};
pub use color_palette::{ColorPaletteId, COLOR_PALETTE_LEN};
pub use style::{mix_colors, Style};
pub use theme::{Theme, ThemeWatcher};
pub use transcript::{
    map_pointer, row_from_message, PointerZone, Pos, Row, RowKind, Transcript,
};
