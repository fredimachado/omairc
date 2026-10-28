pub mod center;
pub mod format_ui;
pub mod input;
pub mod key_bindings;
#[cfg(target_os = "macos")]
pub mod macos_menu;
pub mod members;
pub mod message_row;
pub mod scrollbar;
pub mod sheets;
pub mod shell;
pub mod sidebar;
pub mod status_row;
#[cfg(test)]
mod tests;
pub mod transcript;
pub mod typing_row;
pub mod update;
pub mod widgets;

use std::rc::Rc;

use gpui::{App, Window};

/// A callback handed to rows and widgets in place of the `Shell`. Rows call it
/// with their own data; the owner decides what happens.
pub type Handler<A> = Rc<dyn Fn(A, &mut Window, &mut App)>;

/// The height of both bottom bars: the identity footer under the sidebar
/// (`sidebar::identity_footer`) and the composer strip under the transcript
/// (`center::composer`). Both are border-box at this height with the same
/// `border_t_1` top rule, so the two dividers land on one line at any text scale.
pub const BOTTOM_BAR_HEIGHT: f32 = 66.;
