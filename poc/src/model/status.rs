//! The Status pane: a network's server log, not a conversation. Its rows follow
//! `NetworkLogModel` (`src/irc/networklogmodel.cpp`), and the glyph rule follows
//! `src/qml/ConsoleLine.qml`. Toolkit-free: the UI draws the rows.

use super::format::format_irc;
use super::mock::{MockWorld, StatusLine, StatusSeverity, StatusSource};
use super::transcript::{Row, RowKind};

/// What a Status row carries besides its text: the label, the source that
/// picks the glyph, and the severity that picks the colour.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusMark {
    pub label: String,
    pub source: StatusSource,
    pub severity: StatusSeverity,
}

impl StatusMark {
    /// `>>` for a line the client sent, `--` for a local line, `<<` for the server.
    pub fn glyph(&self) -> &'static str {
        match self.source {
            StatusSource::Client => ">>",
            StatusSource::Local => "--",
            StatusSource::Server => "<<",
        }
    }

    /// The label column: the glyph, a space, then the label.
    pub fn label_text(&self) -> String {
        format!("{} {}", self.glyph(), self.label)
    }
}

/// The Status row for one line. `id` is the row's transcript id.
#[allow(dead_code)] // Phase 2 Status wiring: drop this allow when the Status view renders rows.
pub fn row_from_status_line(id: u64, line: &StatusLine) -> Row {
    Row {
        id,
        kind: RowKind::Status,
        author: None,
        time: Some(line.time.clone()),
        body: format_irc(&line.text),
        continues: false,
        action: false,
        separator_text: None,
        event_text: None,
        status: Some(StatusMark {
            label: line.label.clone(),
            source: line.source,
            severity: line.severity,
        }),
    }
}

/// The Status rows for `network_id`, oldest first. An unknown network has none.
#[allow(dead_code)] // Phase 2 Status wiring: drop this allow when the Status view renders rows.
pub fn status_rows(world: &MockWorld, network_id: &str) -> Vec<Row> {
    let Some(network) = world.network(network_id) else {
        return Vec::new();
    };
    network
        .status
        .iter()
        .enumerate()
        .map(|(index, line)| row_from_status_line(index as u64 + 1, line))
        .collect()
}

/// The Status title, `{display name} Status`, such as `irc.example · fred Status`.
///
/// Qt names the pane from the network's display name (`statusTitleText`). That
/// name already carries the clash suffix, so the conversation duplicate-name
/// rule does not apply here. The window title does not add ` - Omairc`.
pub fn status_title(world: &MockWorld, network_id: &str) -> Option<String> {
    let network = world.network(network_id)?;
    if network.display_name.is_empty() {
        return Some("Status".to_string());
    }
    Some(format!("{} Status", network.display_name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::mock::load_world;

    fn server_lines(world: &MockWorld, network_id: &str) -> Vec<StatusLine> {
        world.network(network_id).expect("network").status.clone()
    }

    #[test]
    fn omarchy_status_is_the_registration_burst() {
        let world = load_world();
        let lines = server_lines(&world, "omarchy");
        // CAP LS, CAP ACK, AUTH, 001-004 (004 splits into five), 250-255, 265, 266, 005.
        assert_eq!(lines.len(), 20);
        assert_eq!(lines[0].label, "CAP");
        assert_eq!(
            lines[0].text,
            "Server supports: echo-message | message-tags | labeled-response | away-notify | multi-prefix | batch | draft/metadata-2 | server-time | account-tag | account-notify | extended-join"
        );
        assert_eq!(lines[1].label, "CAP");
        assert!(lines[1]
            .text
            .starts_with("Acknowledged: echo-message | message-tags"));
        assert_eq!(lines[2].label, "NOTICE");
        assert_eq!(lines[2].text, "-AUTH- *** Looking up your hostname...");
        assert_eq!(lines[3].label, "001");
        assert_eq!(lines[3].text, "Welcome to the demo network");
        assert_eq!(
            lines[4].text,
            "Your host is demo.omairc, running version 1.0"
        );
        assert_eq!(lines[5].text, "This server was created Fri Sep 18 2026");
        let myinfo: Vec<&str> = lines[6..11].iter().map(|l| l.text.as_str()).collect();
        assert_eq!(
            myinfo,
            [
                "Host: demo.omairc",
                "IRCd: OmaircDemo",
                "User modes: iw",
                "Channel modes: abc",
                "Parametric channel modes: def",
            ]
        );
        assert!(lines[6..11].iter().all(|l| l.label == "004"));
        assert_eq!(lines[11].text, "Highest connection count: 42");
        assert_eq!(lines[13].text, "1 IRC Operator online");
        assert_eq!(lines[14].text, "0 unknown connections");
        assert_eq!(lines[15].text, "3 channels formed");
        assert_eq!(lines[16].text, "I have 9 clients and 1 servers");
        assert_eq!(lines[17].text, "Current local users 9, max 12");
        assert_eq!(lines[18].text, "Current global users 12, max 42");
        assert_eq!(lines[19].label, "005");
        assert_eq!(
            lines[19].text,
            "CHANTYPES=# PREFIX=(qaohv)~&@%+ MONITOR=100 draft/ICON=qrc:/demo/omarchy-icon.png are supported by this server"
        );
        assert!(lines.iter().all(|l| !l.text.contains("invited you to")));
    }

    #[test]
    fn oftc_status_has_no_icon_and_keeps_the_invite() {
        let world = load_world();
        let lines = server_lines(&world, "oftc");
        assert_eq!(lines.len(), 21);
        assert_eq!(lines[3].text, "Welcome to the demo OFTC network");
        assert_eq!(
            lines[19].text,
            "CHANTYPES=# PREFIX=(qaohv)~&@%+ MONITOR=100 are supported by this server"
        );
        let invite = lines.last().expect("invite");
        assert_eq!(invite.label, "INVITE");
        assert_eq!(invite.text, "rio invited you to #debian");
    }

    #[test]
    fn seeded_lines_are_server_info() {
        let world = load_world();
        for network in &world.networks {
            for line in &network.status {
                assert_eq!(line.source, StatusSource::Server, "{}", line.label);
                assert_eq!(line.severity, StatusSeverity::Info, "{}", line.label);
                assert_eq!(line.time, "09:58:00");
            }
        }
    }

    #[test]
    fn glyph_follows_the_source() {
        let mark = |source| StatusMark {
            label: "NOTICE".to_string(),
            source,
            severity: StatusSeverity::Info,
        };
        assert_eq!(mark(StatusSource::Client).glyph(), ">>");
        assert_eq!(mark(StatusSource::Local).glyph(), "--");
        assert_eq!(mark(StatusSource::Server).glyph(), "<<");
        assert_eq!(mark(StatusSource::Server).label_text(), "<< NOTICE");
    }

    #[test]
    fn status_row_carries_time_label_and_plain_text() {
        let line = StatusLine {
            time: "09:58:00".to_string(),
            label: "001".to_string(),
            text: "\u{2}Welcome\u{2} home".to_string(),
            source: StatusSource::Server,
            severity: StatusSeverity::Info,
        };
        let row = row_from_status_line(7, &line);
        assert_eq!(row.id, 7);
        assert_eq!(row.kind, RowKind::Status);
        assert_eq!(row.author, None);
        assert_eq!(row.time.as_deref(), Some("09:58:00"));
        assert_eq!(row.body.plain, "Welcome home");
        assert_eq!(
            row.status,
            Some(StatusMark {
                label: "001".to_string(),
                source: StatusSource::Server,
                severity: StatusSeverity::Info,
            })
        );
    }

    #[test]
    fn status_rows_are_numbered_oldest_first() {
        let world = load_world();
        let rows = status_rows(&world, "omarchy");
        assert_eq!(rows.len(), 20);
        assert_eq!(rows[0].id, 1);
        assert_eq!(rows[19].id, 20);
        assert_eq!(
            rows[3].status.as_ref().map(|m| m.label.as_str()),
            Some("001")
        );
        assert!(status_rows(&world, "nope").is_empty());
    }

    #[test]
    fn status_title_uses_the_display_name() {
        let world = load_world();
        assert_eq!(
            status_title(&world, "omarchy").as_deref(),
            Some("irc.example · fred Status")
        );
        assert_eq!(
            status_title(&world, "oftc").as_deref(),
            Some("irc.example · oak Status")
        );
        assert_eq!(status_title(&world, "nope"), None);
    }

    #[test]
    fn an_empty_display_name_falls_back_to_plain_status() {
        let mut world = load_world();
        world.networks[0].display_name.clear();
        assert_eq!(status_title(&world, "omarchy").as_deref(), Some("Status"));
    }
}
