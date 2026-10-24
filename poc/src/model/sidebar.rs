//! Sidebar rows and the sidebar's keyboard state. The order, the unread and mention
//! counts, and the collapse set come from the controller (`conversations`,
//! `network_order`, `is_network_collapsed`), so this module only shapes them into the
//! rows the UI walks. Ports Qt's `ircSidebarOrder` for the walk.
//!
//! What stays here is what the window remembers between chords: the network the
//! keyboard has on its header, and whether the server list is shown.

use super::ids::ConversationRef;
use crate::controller::snapshots::ConversationRow;

/// What the sidebar remembers between chords. Focus is a network id, so it survives
/// the network being moved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarState {
    pub focus: Option<String>,
    pub rail_visible: bool,
}

impl Default for SidebarState {
    fn default() -> Self {
        Self {
            focus: None,
            rail_visible: true,
        }
    }
}

/// One conversation in sidebar order. `collapsed` is the owning network's state when
/// the rows were built; `unread` and `mention` are the row's badges.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarRow {
    pub conversation: ConversationRef,
    pub name: String,
    pub channel: bool,
    pub collapsed: bool,
    pub unread: u32,
    pub mention: bool,
}

/// One network's header and its badges, as the sidebar and its footer draw them. The
/// name, nick and counts come from the controller; the icon colour comes from the
/// Connect profile the sheet keeps for the network.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NetworkHeader {
    pub id: String,
    /// The display name, with the clash suffix when another network shares it.
    pub name: String,
    /// The nick the network's session holds.
    pub nick: String,
    /// The account beside the nick in the identity footer, empty when it adds nothing
    /// (`Controller::display_account`).
    pub account: String,
    /// The nick's avatar reference, from the presence facts.
    pub avatar: Option<String>,
    /// Index into the nick palette, if the profile has one.
    pub icon_color: Option<u8>,
    /// The network's icon (`draft/ICON`): a bundled image or an https URL.
    pub icon: Option<String>,
    pub unread: bool,
    pub mention: bool,
    /// Unseen alert-severity Status lines (`liveAlerts`).
    pub alerts: u32,
    /// Whether the `edit` label shows (`showEdit`: the network has a session).
    pub show_edit: bool,
    /// Whether the network has a direct message, so the DM heading shows.
    pub has_directs: bool,
    pub collapsed: bool,
    /// The status line under the name: the last error while one is set, else the
    /// connection word (`model::status::connection_line`).
    pub status: String,
}

/// Every conversation in sidebar order, collapsed networks included. Callers that walk
/// only visible rows filter on `collapsed`. `conversations` arrives in sidebar order
/// already, from `Controller::conversations`.
pub fn sidebar_rows(
    conversations: &[ConversationRow],
    is_collapsed: impl Fn(&str) -> bool,
) -> Vec<SidebarRow> {
    conversations
        .iter()
        .map(|row| SidebarRow {
            conversation: ConversationRef::from_key(&row.key),
            name: row.target.clone(),
            channel: !row.direct,
            collapsed: is_collapsed(&row.key.network_id),
            unread: row.unread,
            mention: row.mention,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::controller::Controller;

    fn labels(rows: &[SidebarRow]) -> Vec<String> {
        rows.iter()
            .map(|row| format!("{}/{}", row.conversation.network_id, row.name))
            .collect()
    }

    #[test]
    fn order_is_channels_then_directs_case_insensitively_per_network() {
        let controller = Controller::seeded_for_tests();
        let rows = sidebar_rows(&controller.conversations(), |_| false);
        let omarchy: Vec<_> = rows
            .iter()
            .filter(|r| r.conversation.network_id == "omarchy")
            .map(|r| r.name.as_str())
            .collect();
        assert_eq!(
            omarchy,
            ["#desktop", "#help", "#omarchy", "#ricing", "anna", "dax"]
        );
    }

    #[test]
    fn networks_keep_their_order_and_rows_carry_collapse_state() {
        let mut controller = Controller::seeded_for_tests();
        controller.set_network_collapsed("oftc", true);
        let rows = sidebar_rows(&controller.conversations(), |id| {
            controller.is_network_collapsed(id)
        });
        let first_oftc = rows
            .iter()
            .position(|r| r.conversation.network_id == "oftc")
            .unwrap();
        assert!(rows[..first_oftc]
            .iter()
            .all(|r| r.conversation.network_id == "omarchy"));
        assert!(rows[first_oftc..].iter().all(|r| r.collapsed));
        assert!(rows[..first_oftc].iter().all(|r| !r.collapsed));
        assert!(labels(&rows).contains(&"oftc/#build".to_string()));
    }

    #[test]
    fn rows_carry_the_unread_and_mention_badges() {
        let controller = Controller::seeded_for_tests();
        let rows = sidebar_rows(&controller.conversations(), |_| false);
        let ricing = rows.iter().find(|r| r.name == "#ricing").expect("#ricing");
        assert_eq!(ricing.unread, 12);
        assert!(ricing.mention);
        let desktop = rows.iter().find(|r| r.name == "#desktop").expect("#desktop");
        assert_eq!(desktop.unread, 3);
        assert!(!desktop.mention);
    }
}
