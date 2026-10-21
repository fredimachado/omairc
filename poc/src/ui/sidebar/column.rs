use gpui::{div, prelude::*, Entity, ScrollHandle};

use crate::controller::snapshots::ConversationRow;
use crate::model::presence::photo_for;
use crate::model::sidebar::{NetworkHeader, SidebarState};
use crate::model::{ConversationRef, Style};
use crate::ui::scrollbar::{div_vertical_scrollbar, ScrollbarDragCell};
use crate::ui::sidebar::conversation_row::{conversation_row, RowLook};
use crate::ui::sidebar::identity_footer::identity_footer;
use crate::ui::sidebar::network_section::network_header;
use crate::ui::widgets::section_heading::{channels_heading, directs_heading};
use crate::ui::Handler;
use crate::util::{scaled_px, to_gpui};

/// What the sidebar shows: every network, the rail and focus state, the selection, and
/// the identity footer's numbers.
pub struct SidebarData<'a> {
    /// Every network in sidebar order, with its header and badges.
    pub networks: &'a [NetworkHeader],
    /// Every conversation row in sidebar order (`Controller::conversations`).
    pub conversations: &'a [ConversationRow],
    pub state: &'a SidebarState,
    pub selected: &'a ConversationRef,
    /// Status is showing, so no conversation row is the current one (Qt `current`).
    pub status_open: bool,
    pub inbox_count: u32,
    pub typing_phase: u8,
    /// The `avatars` preference: off, rows and the footer show initials.
    pub avatars: bool,
    /// A network icon or name click: opens that network's Status.
    pub on_status: Handler<String>,
    /// The `edit` label on a network: opens the Connect sheet on that network.
    pub on_edit: Handler<String>,
    /// The footer's `inbox` link.
    pub on_inbox: Handler<()>,
    /// The footer's version: opens the About sheet, or starts a ready installer.
    pub on_about: Handler<()>,
    /// The installer is ready, so the footer's version reads `Restart to update`.
    pub update_ready: bool,
    /// A conversation row's click: selects it.
    pub on_select: Handler<ConversationRef>,
    /// A direct message row's close control.
    pub on_close: Handler<ConversationRef>,
    /// A network header's collapse toggle.
    pub on_toggle: Handler<String>,
}

/// The left column: every network stacked in sidebar order, then the identity footer.
/// The column collapses by width (`rail_visible`), clipped, so its rows stay mounted.
/// `host` is only notified by the scrollbar.
pub fn render_sidebar<V: 'static>(
    data: SidebarData,
    style: &Style,
    scroll: &ScrollHandle,
    host: Entity<V>,
    drag: &ScrollbarDragCell,
) -> impl IntoElement {
    let visible = data.state.rail_visible;
    let width = if visible { 244. } else { 0. };
    // With no network the footer shows the blank identity, as a first run does.
    let footer_network = data
        .networks
        .iter()
        .find(|network| network.id == data.selected.network_id)
        .cloned()
        .unwrap_or_default();

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
                        .child(network_list(&data, style)),
                )
                .child(div_vertical_scrollbar(scroll, style, host, drag.clone())),
        )
        .child(identity_footer(
            &footer_network,
            data.inbox_count,
            data.on_inbox.clone(),
            data.on_about.clone(),
            data.update_ready,
            data.avatars,
            style,
        ))
}

fn network_list(data: &SidebarData, style: &Style) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .pb(scaled_px(style, 8.))
        .children(data.networks.iter().map(|network| {
            let collapsed = network.collapsed;
            let focused = data.state.focus.as_deref() == Some(network.id.as_str());
            div()
                .flex()
                .flex_col()
                .child(network_header(
                    network,
                    focused,
                    collapsed,
                    style,
                    data.on_toggle.clone(),
                    data.on_status.clone(),
                    data.on_edit.clone(),
                ))
                .when(!collapsed, |this| {
                    this.child(conversation_groups(
                        network,
                        data,
                        style,
                        data.on_select.clone(),
                        data.on_close.clone(),
                    ))
                })
        }))
}

/// One network's channels, then its direct messages. The DM heading shows only when
/// the network has a DM.
fn conversation_groups(
    network: &NetworkHeader,
    data: &SidebarData,
    style: &Style,
    on_select: Handler<ConversationRef>,
    on_close: Handler<ConversationRef>,
) -> impl IntoElement {
    let rows: Vec<&ConversationRow> = data
        .conversations
        .iter()
        .filter(|row| row.key.network_id == network.id)
        .collect();
    let row = |conv: &ConversationRow| {
        let target = ConversationRef::from_key(&conv.key);
        let selected = !data.status_open && &target == data.selected;
        // A DM's photo and bot flag come from the peer's presence facts, which the row
        // already carries.
        let look = RowLook {
            selected,
            typing_phase: data.typing_phase,
            photo: photo_for(Some(conv.avatar.as_str()).filter(|a| !a.is_empty()), data.avatars),
            bot: conv.bot,
        };
        conversation_row(target, conv, look, style, on_select.clone(), on_close.clone())
    };
    let channels: Vec<&ConversationRow> = rows.iter().copied().filter(|c| !c.direct).collect();
    let directs: Vec<&ConversationRow> = rows.iter().copied().filter(|c| c.direct).collect();

    div()
        .flex()
        .flex_col()
        .child(channels_heading(style))
        .children(channels.into_iter().map(&row))
        .when(network.has_directs, |this| {
            this.child(directs_heading(style))
                .children(directs.into_iter().map(&row))
        })
}
