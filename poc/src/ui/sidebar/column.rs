use gpui::{div, prelude::*, Entity, ScrollHandle};

use crate::model::sidebar::{has_directs, ordered_conversations, SidebarState};
use crate::model::{Conversation, ConversationRef, MockWorld, Network, Style};
use crate::ui::scrollbar::{div_vertical_scrollbar, ScrollbarDragCell};
use crate::ui::sidebar::conversation_row::conversation_row;
use crate::ui::sidebar::identity_footer::identity_footer;
use crate::ui::sidebar::network_section::network_header;
use crate::ui::widgets::section_heading::{channels_heading, directs_heading};
use crate::ui::Handler;
use crate::util::{scaled_px, to_gpui};

/// What the sidebar shows: every network, the collapse and rail state, the
/// selection, and the identity footer's numbers.
pub struct SidebarData<'a> {
    pub world: &'a MockWorld,
    pub state: &'a SidebarState,
    pub selected: &'a ConversationRef,
    pub inbox_count: u32,
    pub typing_phase: u8,
}

/// The left column: every network stacked in world order, then the identity
/// footer. The column collapses by width (`rail_visible`), clipped, so its
/// rows stay mounted. `host` is only notified by the scrollbar.
pub fn render_sidebar<V: 'static>(
    data: SidebarData,
    style: &Style,
    scroll: &ScrollHandle,
    host: Entity<V>,
    drag: &ScrollbarDragCell,
    on_select: Handler<ConversationRef>,
    on_toggle: Handler<String>,
) -> impl IntoElement {
    let visible = data.state.rail_visible;
    let width = if visible { 244. } else { 0. };
    let footer_network = data
        .world
        .network(&data.selected.network_id)
        .expect("selected network is in the world");

    div()
        .w(scaled_px(style, width))
        .h_full()
        .flex_shrink_0()
        .overflow_hidden()
        .bg(to_gpui(style.panel_color()))
        .when(visible, |this| {
            this.border_r_1()
                .border_color(to_gpui(style.divider_color()))
        })
        .flex()
        .flex_col()
        .min_h_0()
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
                        .child(network_list(&data, style, on_select, on_toggle)),
                )
                .child(div_vertical_scrollbar(scroll, style, host, drag.clone())),
        )
        .child(identity_footer(
            footer_network,
            data.inbox_count,
            style,
        ))
}

fn network_list(
    data: &SidebarData,
    style: &Style,
    on_select: Handler<ConversationRef>,
    on_toggle: Handler<String>,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .pb(scaled_px(style, 8.))
        .children(data.world.networks.iter().map(|network| {
            let collapsed = data.state.is_collapsed(&network.id);
            let focused = data.state.focus.as_deref() == Some(network.id.as_str());
            div()
                .flex()
                .flex_col()
                .child(network_header(
                    network,
                    focused,
                    collapsed,
                    style,
                    on_toggle.clone(),
                ))
                .when(!collapsed, |this| {
                    this.child(conversation_groups(
                        network,
                        data,
                        style,
                        on_select.clone(),
                    ))
                })
        }))
}

/// One network's channels, then its direct messages. The DM heading shows
/// only when the network has a DM.
fn conversation_groups(
    network: &Network,
    data: &SidebarData,
    style: &Style,
    on_select: Handler<ConversationRef>,
) -> impl IntoElement {
    let ordered = ordered_conversations(network);
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
    let channels: Vec<&Conversation> = ordered.iter().copied().filter(|c| c.channel).collect();
    let directs: Vec<&Conversation> = ordered.iter().copied().filter(|c| !c.channel).collect();

    div()
        .flex()
        .flex_col()
        .child(channels_heading(style))
        .children(channels.into_iter().map(&row))
        .when(has_directs(network), |this| {
            this.child(directs_heading(style))
                .children(directs.into_iter().map(&row))
        })
}
