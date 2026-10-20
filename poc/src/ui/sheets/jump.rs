//! The Jump sheet (`Ctrl+K`): every conversation across networks, plus one
//! Status row per network, fuzzy-matched on the label.

use gpui::AnyElement;

use crate::model::{jump_targets, JumpKind, JumpTarget, MockWorld};
use crate::model::Style;
use crate::ui::sheets::filter_sheet::{text_row, FilterSource};
use gpui::IntoElement;

pub struct JumpSource {
    pub world: MockWorld,
}

impl FilterSource for JumpSource {
    type Item = JumpTarget;

    fn placeholder(&self) -> &'static str {
        "Jump to conversation…"
    }

    fn items(&self, query: &str) -> Vec<JumpTarget> {
        jump_targets(&self.world, query)
    }

    fn activatable(&self, item: &JumpTarget) -> bool {
        item.kind == JumpKind::Conversation
    }

    fn width(&self) -> f32 {
        348.
    }

    fn row_height(&self) -> f32 {
        28.
    }

    fn render_row(&self, item: &JumpTarget, selected: bool, style: &Style) -> AnyElement {
        text_row(
            style,
            28.,
            item.label.clone(),
            &item.label_indices,
            selected,
        )
        .into_any_element()
    }
}
