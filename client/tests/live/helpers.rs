use omairc::controller::controller::Controller;
use omairc::controller::snapshots::ConversationRow;
use omairc::irc::message::IrcMessage;
use omairc::irc::wiretext::wire_text;
use omairc::live::{live_daemons, live_enabled, LiveClient};

/// Fails closed when someone runs `--ignored` without the compose runner.
pub fn require_live() {
    assert!(
        live_enabled(),
        "Run bin/test-live. Live tests need OMAIRC_LIVE_DAEMONS and published ports."
    );
    assert!(
        !live_daemons().is_empty(),
        "OMAIRC_LIVE_DAEMONS was set but no daemon ports were exported."
    );
}

pub fn conversation_row(controller: &Controller, network_id: &str, target: &str) -> Option<usize> {
    controller
        .conversations()
        .iter()
        .enumerate()
        .find(|(_, row)| {
            row.key.network_id == network_id && row.target.eq_ignore_ascii_case(target)
        })
        .map(|(index, _)| index)
}

pub fn status_text_contains(client: &LiveClient, needle: &str) -> bool {
    client.status_lines().iter().any(|entry| {
        entry
            .text
            .to_ascii_lowercase()
            .contains(&needle.to_ascii_lowercase())
    })
}

pub fn no_status_text_contains(client: &LiveClient, needle: &str) -> bool {
    !status_text_contains(client, needle)
}

pub fn names_end_count(incoming: &[IrcMessage], channel: &str) -> usize {
    incoming
        .iter()
        .filter(|message| {
            message.command == "366"
                && message
                    .params
                    .get(1)
                    .is_some_and(|param| wire_text(param).eq_ignore_ascii_case(channel))
        })
        .count()
}

pub fn saw_channel_mode(incoming: &[IrcMessage], channel: &str, mode: char) -> bool {
    incoming.iter().any(|message| {
        if message.command != "MODE" || message.params.len() < 2 {
            return false;
        }
        if !wire_text(&message.params[0]).eq_ignore_ascii_case(channel) {
            return false;
        }
        // Only the mode string (params[1]): require `mode` inside a `+` run.
        let mut adding = false;
        for character in wire_text(&message.params[1]).chars() {
            match character {
                '+' => adding = true,
                '-' => adding = false,
                flag if adding && flag == mode => return true,
                _ => {}
            }
        }
        false
    })
}

/// Direct-message rows for `nick` under the network's case mapping.
pub fn direct_rows_for(client: &LiveClient, nick: &str) -> Vec<ConversationRow> {
    let mapping = client.features().case_mapping();
    client
        .controller
        .conversations()
        .into_iter()
        .filter(|row| {
            row.direct
                && row.key.network_id == client.network_id
                && mapping.equals(&row.target, nick)
        })
        .collect()
}

pub fn saw_part(incoming: &[IrcMessage], channel: &str) -> bool {
    incoming.iter().any(|message| {
        message.command == "PART"
            && message
                .params
                .first()
                .is_some_and(|param| wire_text(param).eq_ignore_ascii_case(channel))
    })
}

pub fn channel_has_body(client: &LiveClient, body: &str) -> bool {
    client.messages().iter().any(|message| message.body == body)
}

pub fn wait_names_end(client: &mut LiveClient, channel: &str) -> bool {
    client.wait_until(|client| names_end_count(&client.incoming(), channel) >= 1)
}
