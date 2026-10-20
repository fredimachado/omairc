pub mod center;
pub mod format_ui;
pub mod input;
pub mod members;
pub mod message_row;
pub mod scrollbar;
pub mod sheets;
pub mod shell;
pub mod sidebar;
pub mod status_row;
pub mod transcript;
pub mod typing_row;
pub mod widgets;

use std::rc::Rc;

use gpui::{App, Window};

/// A callback handed to rows and widgets in place of the `Shell`. Rows call it
/// with their own data; the owner decides what happens.
pub type Handler<A> = Rc<dyn Fn(A, &mut Window, &mut App)>;
