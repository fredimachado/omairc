//! Linux primary-selection write and middle-click paste for [`SingleLineInput`].

use gpui::{Context, MouseDownEvent, Window};
#[cfg(target_os = "linux")]
use gpui::{ClipboardItem, EntityInputHandler};

#[cfg(target_os = "linux")]
use crate::model::text_edit::selection_text;

use super::SingleLineInput;

impl SingleLineInput {
    /// Writes the current selection to the Linux primary buffer. No-op when the
    /// field is masked, the selection is empty, or the target is not Linux.
    pub(super) fn write_primary_selection(&self, cx: &mut Context<Self>) {
        #[cfg(target_os = "linux")]
        {
            if self.mask {
                return;
            }
            let text = selection_text(&self.content, self.selected_range.clone());
            if !text.is_empty() {
                cx.write_to_primary(ClipboardItem::new_string(text.to_string()));
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = cx;
        }
    }

    /// Middle-click paste from the Linux primary selection at the click point.
    pub(super) fn on_middle_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        #[cfg(target_os = "linux")]
        {
            window.focus(&self.focus_handle);
            let offset = self.index_for_mouse_position(event.position);
            self.move_to(offset, cx);
            if let Some(text) = cx.read_from_primary().and_then(|item| item.text()) {
                self.replace_text_in_range(None, &text.replace('\n', " "), window, cx);
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (event, window, cx);
        }
    }
}
