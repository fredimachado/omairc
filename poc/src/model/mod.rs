//! Toolkit-agnostic Omairc PoC model: theme, style, mock world, IRC formatting,
//! jump palette, and virtualized transcript selection.

pub mod byte_budget;
pub mod channel_list;
pub mod color;
pub mod cursor;
pub mod format;
pub mod ids;
pub mod chords;
pub mod composer;
pub mod connect;
pub mod directs;
pub mod filter_list;
pub mod find;
pub mod fuzzy;
pub mod inbox;
pub mod jump;
pub mod links;
pub mod membership;
pub mod mention;
pub mod mock;
pub mod navigation;
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
pub mod transcript;
pub mod typing_tail;
pub mod unread;

pub use color::Color;
pub use ids::ConversationRef;
pub use format::{format_irc, plain_irc_text, FormattedText, StyleRun, TextStyle};
pub use jump::{jump_targets, JumpKind, JumpTarget, JUMP_RESULT_LIMIT};
pub use mock::{
    bench_channel, load_world, local_log_stamp, local_send_stamp, outgoing_action,
    outgoing_message, whois_message, Conversation, Member, MockMessage, MockWorld, Network,
};
pub use style::{mix_colors, Style};
pub use theme::{
    default_omarchy_colors_path, parse_colors_toml, Theme, ThemeWatcher,
};
pub use transcript::{
    map_pointer, row_for_message, row_from_message, rows_from_messages, PointerZone, Pos, Row,
    RowKind, ScrollAnchor, Selection, Transcript,
};
