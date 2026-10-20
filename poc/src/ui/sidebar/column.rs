use gpui::{div, prelude::*, Entity, ScrollHandle};

use crate::model::{Conversation, ConversationRef, Network, Style};
use crate::ui::scrollbar::{div_vertical_scrollbar, ScrollbarDragCell};
use crate::ui::sidebar::conversation_row::conversation_row;
use crate::ui::sidebar::identity_footer::identity_footer;
use crate::ui::sidebar::network_section::network_header;
use crate::ui::widgets::section_heading::section_label;
use crate::ui::Handler;
use crate::util::scaled_px;

/// What the sidebar shows: the current network and where the selection is.
pub struct SidebarData<'a> {
    pub network: &'a Network,
    pub selected: &'a ConversationRef,
    pub inbox_count: u32,
    pub typing_phase: u8,
}

/// The left column: the server header, the channel and DM list, and the
/// identity footer. `host` is only notified by the scrollbar.
pub fn render_sidebar<V: 'static>(
    data: SidebarData,
    style: &Style,
    scroll: &ScrollHandle,
    host: Entity<V>,
    drag: &ScrollbarDragCell,
    on_select: Handler<ConversationRef>,
) -> impl IntoElement {
    let sidebar_w = scaled_px(style, 244.);

    div()
        .w(sidebar_w)
        .h_full()
        .flex_shrink_0()
        .bg(crate::util::to_gpui(style.panel_color()))
        .border_r_1()
        .border_color(crate::util::to_gpui(style.divider_color()))
        .flex()
        .flex_col()
        .min_h_0()
        .child(network_header(data.network, style))
        .child(
            div()
                .relative()
                .flex_grow()
                .min_h_0()
                .child(
                    div()
                        .id("sidebar-conversations")
                        .size_full()
                        .overflow_y_scroll()
                        .track_scroll(scroll)
                        .child(conversation_list(&data, style, on_select)),
                )
                .child(div_vertical_scrollbar(scroll, style, host, drag.clone())),
        )
        .child(identity_footer(data.network, data.inbox_count, style))
}

/// Channels first, then direct messages, each in the network's stored order.
fn conversation_list(
    data: &SidebarData,
    style: &Style,
    on_select: Handler<ConversationRef>,
) -> impl IntoElement {
    let network = data.network;
    let channels: Vec<&Conversation> = network.conversations.iter().filter(|c| c.channel).collect();
    let directs: Vec<&Conversation> = network.conversations.iter().filter(|c| !c.channel).collect();
    let row = |conv: &Conversation| {
        let target = ConversationRef::new(network.id.clone(), conv.key());
        let selected = &target == data.selected;
        conversation_row(
            target,
            conv,
            selected,
            data.typing_phase,
            style,
            on_select.clone(),
        )
    };

    div()
        .flex()
        .flex_col()
        .py(scaled_px(style, 8.))
        .child(section_label(style, "CHANNELS"))
        .children(channels.into_iter().map(&row))
        .child(section_label(style, "DIRECT MESSAGES"))
        .children(directs.into_iter().map(&row))
}
