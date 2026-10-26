//! The `/list` channel sheet's rows: the controller's answer to `LIST mask`, ranked by
//! the sheet's query, and the status line of `client/src/ui/sheets/channel_list.rs`. The
//! answer is the overlay's rows (`Controller::channel_list`, fed by the wire in
//! `controller/channellist`), and this module only ranks and shows it.
//! `ChannelListModel::rowLess` (most users first, then name).

use super::format::{format_irc, FormattedText, TextStyle};
use super::fuzzy::rank;
use crate::controller::channellist::ListedChannel;

/// A name hit outranks a topic hit: the name weighs twice the topic.
const NAME_WEIGHT: u32 = 2;
const TOPIC_WEIGHT: u32 = 1;

/// One row of the channel list sheet.
#[derive(Clone, Debug, PartialEq)]
pub struct ListRow {
    pub name: String,
    pub users: usize,
    /// The topic without colours. Its bold, italic and underline runs stay.
    pub topic: FormattedText,
    /// Byte offsets into `name` of the characters the query matched.
    pub name_indices: Vec<usize>,
    /// Byte offsets into `topic.plain` of the characters the query matched.
    pub topic_indices: Vec<usize>,
}

/// The sheet's rows for `query`. `listing` is in listing order. The query ranks it
/// fuzzily, a name hit weighing twice a topic hit, and equal scores keep the
/// listing order. An empty query keeps the listing order: most users first.
pub fn channel_list_rows(listing: &[ListedChannel], query: &str) -> Vec<ListRow> {
    let rows = listing.iter().map(|channel| ListRow {
        name: channel.name.clone(),
        users: channel.users,
        topic: list_topic(&channel.topic),
        name_indices: Vec::new(),
        topic_indices: Vec::new(),
    });
    rank(query, rows, |row: &ListRow| {
        [
            (row.name.as_str(), NAME_WEIGHT),
            (row.topic.plain.as_str(), TOPIC_WEIGHT),
        ]
    })
    .into_iter()
    .map(|ranked| {
        let [name_hit, topic_hit] = ranked.fields;
        ListRow {
            name_indices: name_hit.map(|hit| hit.indices).unwrap_or_default(),
            topic_indices: topic_hit.map(|hit| hit.indices).unwrap_or_default(),
            ..ranked.item
        }
    })
    .collect()
}

/// The topic with its colours and its reverse, monospace and strikethrough codes
/// removed. Bold, italic and underline runs stay, as Qt's `emphasizedIrcText` keeps them.
fn list_topic(topic: &str) -> FormattedText {
    let mut text = format_irc(topic);
    for run in &mut text.runs {
        run.style = TextStyle {
            bold: run.style.bold,
            italic: run.style.italic,
            underline: run.style.underline,
            ..TextStyle::default()
        };
    }
    text.links.clear();
    text
}

/// The sheet's status line (`channelListStatus`): what the filter kept out of
/// what the mask returned.
pub fn listing_status(
    listed: usize,
    shown: usize,
    query: &str,
    loading: bool,
    error: &str,
) -> String {
    if !error.is_empty() {
        return error.to_owned();
    }
    if loading && listed == 0 {
        return "Loading channels…".to_owned();
    }
    if loading {
        return format!("Loading… {listed} channels");
    }
    if shown == 0 {
        let text = if query.is_empty() {
            "No channels"
        } else {
            "No matches"
        };
        return text.to_owned();
    }
    if shown != listed {
        return format!("{shown} of {listed} channels");
    }
    format!("{listed} channels")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channel(name: &str, users: usize, topic: &str) -> ListedChannel {
        ListedChannel {
            name: name.to_string(),
            users,
            topic: topic.to_string(),
        }
    }

    #[test]
    fn an_empty_query_keeps_the_listing_order() {
        let listing = [channel("#omarchy", 12, ""), channel("#desktop", 3, "")];
        let names: Vec<_> = channel_list_rows(&listing, "")
            .into_iter()
            .map(|row| row.name)
            .collect();
        assert_eq!(names, ["#omarchy", "#desktop"]);
    }

    #[test]
    fn a_name_hit_outranks_a_topic_hit() {
        let listing = [channel("#other", 9, "about desktop"), channel("#desktop", 2, "")];
        let rows = channel_list_rows(&listing, "desktop");
        assert_eq!(rows[0].name, "#desktop");
        assert!(!rows[0].name_indices.is_empty());
    }

    #[test]
    fn the_status_line_reports_loading_and_errors() {
        assert_eq!(
            listing_status(0, 0, "", true, ""),
            "Loading channels…"
        );
        assert_eq!(
            listing_status(12, 12, "", true, ""),
            "Loading… 12 channels"
        );
        assert_eq!(
            listing_status(3, 3, "", false, "Channel list failed."),
            "Channel list failed."
        );
    }

    #[test]
    fn the_topic_drops_colours_and_keeps_emphasis() {
        let listing = [channel("#x", 1, "\u{3}4red\u{3} \u{2}bold\u{2}")];
        let row = &channel_list_rows(&listing, "")[0];
        assert!(!row.topic.plain.contains('\u{3}'));
        assert!(row.topic.runs.iter().any(|run| run.style.bold));
    }

    #[test]
    fn the_status_line_counts_what_the_filter_kept() {
        assert_eq!(listing_status(0, 0, "", false, ""), "No channels");
        assert_eq!(listing_status(3, 0, "zzz", false, ""), "No matches");
        assert_eq!(listing_status(3, 1, "x", false, ""), "1 of 3 channels");
        assert_eq!(listing_status(3, 3, "", false, ""), "3 channels");
    }

}
