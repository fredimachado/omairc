//! The `/list` channel sheet's data: each network's joined channels and its
//! directory, filtered by the `/list` mask, then ranked by the sheet's query.
//! Ports `listEntries` and `listMaskMatches` (`src/irc/ircdemoserver.cpp`), the
//! row order of `ChannelListModel::rowLess` (most users first, then name), and
//! the status line of `src/qml/ChannelListSheet.qml`.

use std::collections::HashMap;

use super::format::{format_irc, FormattedText, TextStyle};
use super::fuzzy::rank;
use super::mock::MockWorld;

/// A name hit outranks a topic hit: the name weighs twice the topic.
const NAME_WEIGHT: u32 = 2;
const TOPIC_WEIGHT: u32 = 1;

/// One channel as a `LIST` answer gives it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListedChannel {
    pub name: String,
    pub users: usize,
    /// The topic as the server sent it, with its mIRC codes.
    pub topic: String,
}

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

/// What `LIST mask` answers on `network_id`: its joined channels and its
/// directory, kept to the mask, most users first and then by name. A joined
/// channel the directory also lists shows the directory's row once, because the
/// server's count is the one that counts. A mock join starts with no members.
pub fn channel_listing(world: &MockWorld, network_id: &str, mask: &str) -> Vec<ListedChannel> {
    let Some(network) = world.network(network_id) else {
        return Vec::new();
    };
    let mask = mask.trim();
    let mut rows: Vec<ListedChannel> = network
        .directory
        .iter()
        .map(|entry| ListedChannel {
            name: entry.name.clone(),
            users: entry.users,
            topic: entry.topic.clone(),
        })
        .collect();
    for conv in network.conversations.iter().filter(|conv| conv.channel) {
        let listed = rows.iter().any(|row| fold(&row.name) == fold(&conv.name));
        if !listed {
            rows.push(ListedChannel {
                name: conv.name.clone(),
                users: conv.people_count(),
                topic: conv.topic.clone().unwrap_or_default(),
            });
        }
    }
    rows.retain(|row| list_mask_matches(&row.name, mask));
    rows.sort_by(|left, right| {
        right
            .users
            .cmp(&left.users)
            .then_with(|| fold(&left.name).cmp(&fold(&right.name)))
    });
    rows
}

/// The answers the sheet has shown, kept per network and mask. A mask matches an
/// earlier one ignoring case and surrounding spaces, as Qt's
/// `IrcChannelListRequest::sameMask` does. A repeat `/list` reuses the rows.
#[derive(Debug, Default)]
pub struct ChannelListCache {
    answers: HashMap<(String, String), Vec<ListedChannel>>,
}

impl ChannelListCache {
    /// The answer for `mask` on `network_id`. The first ask builds it from the
    /// world. A repeat returns the rows already held, even if the world has moved on.
    pub fn listing(&mut self, world: &MockWorld, network_id: &str, mask: &str) -> &[ListedChannel] {
        let key = (network_id.to_string(), fold(mask.trim()));
        self.answers
            .entry(key)
            .or_insert_with(|| channel_listing(world, network_id, mask))
    }
}

/// Whether `name` is in the answer to `LIST mask` (`listMaskMatches`). The mask
/// is a comma-separated list. A part with `*` or `?` is a wildcard that must
/// cover the whole name. Any other part must equal the name. Case is ignored. An
/// empty mask lists everything, and so does a mask of only commas.
pub fn list_mask_matches(name: &str, mask: &str) -> bool {
    if mask.is_empty() {
        return true;
    }
    let parts: Vec<&str> = mask.split(',').filter(|part| !part.is_empty()).collect();
    if parts.is_empty() {
        return true;
    }
    let name = fold(name);
    parts
        .into_iter()
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .any(|part| {
            let part = fold(part);
            if part.contains(['*', '?']) {
                wildcard_matches(&part, &name)
            } else {
                part == name
            }
        })
}

/// Glob matching over the whole text: `*` takes any run of characters, `?` takes
/// one. Both sides are already folded to lower case.
fn wildcard_matches(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let (mut p, mut t) = (0, 0);
    // The last `*` seen, with the pattern and text positions it started at.
    let mut star: Option<(usize, usize)> = None;
    while t < text.len() {
        if p < pattern.len() && pattern[p] == '*' {
            star = Some((p, t));
            p += 1;
        } else if p < pattern.len() && (pattern[p] == '?' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if let Some((star_p, star_t)) = star {
            // Let the last `*` absorb one more character, then try again.
            p = star_p + 1;
            t = star_t + 1;
            star = Some((star_p, star_t + 1));
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|&ch| ch == '*')
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
pub fn listing_status(listed: usize, shown: usize, query: &str) -> String {
    if shown == 0 {
        let text = if query.is_empty() {
            "No channels"
        } else {
            "No matches"
        };
        return text.to_string();
    }
    if shown != listed {
        return format!("{shown} of {listed} channels");
    }
    format!("{listed} channels")
}

/// Lower-cases for comparison, as Qt's case-insensitive compares do.
fn fold(text: &str) -> String {
    text.to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::mock::{load_world, DirectoryEntry};
    use crate::model::{inbox::join_channel, ConversationRef};

    fn listed(name: &str, users: usize, topic: &str) -> ListedChannel {
        ListedChannel {
            name: name.to_string(),
            users,
            topic: topic.to_string(),
        }
    }

    fn names(rows: &[ListRow]) -> Vec<&str> {
        rows.iter().map(|row| row.name.as_str()).collect()
    }

    const LINUX_TOPIC: &str = "\u{2}Kernel\u{2} discussion and \u{3}04distro\u{3} help.";

    #[test]
    fn the_demo_directory_lists_linux_random_and_debian() {
        let world = load_world();
        let omarchy = world.network("omarchy").expect("omarchy");
        let linux = omarchy
            .directory
            .iter()
            .find(|entry| entry.name == "#linux")
            .expect("#linux");
        assert_eq!(linux.users, 42);
        assert_eq!(linux.topic, LINUX_TOPIC);
        assert_eq!(
            omarchy
                .directory
                .iter()
                .find(|entry| entry.name == "#random"),
            Some(&DirectoryEntry {
                name: "#random".to_string(),
                users: 4,
                topic: "Off-topic chatter.".to_string(),
            })
        );
        let oftc = world.network("oftc").expect("oftc");
        assert_eq!(oftc.directory.len(), 1);
        assert_eq!(oftc.directory[0].name, "#debian");
        assert_eq!(oftc.directory[0].users, 28);
    }

    #[test]
    fn the_listing_has_joined_channels_and_directory_most_users_first() {
        let world = load_world();
        let rows = channel_listing(&world, "omarchy", "");
        let listed_names: Vec<&str> = rows.iter().map(|row| row.name.as_str()).collect();
        assert!(listed_names.contains(&"#omarchy"), "joined channels are listed");
        assert!(listed_names.contains(&"#linux") && listed_names.contains(&"#random"));
        assert_eq!(rows[0].name, "#linux", "42 users is the most");
        assert!(rows.windows(2).all(|pair| pair[0].users >= pair[1].users));
        let joined = rows
            .iter()
            .find(|row| row.name == "#omarchy")
            .expect("#omarchy");
        let conv = world
            .find(&ConversationRef::new("omarchy", "#omarchy"))
            .expect("#omarchy");
        assert_eq!(joined.users, conv.people_count());
    }

    #[test]
    fn a_network_without_that_name_lists_nothing() {
        assert!(channel_listing(&load_world(), "nowhere", "").is_empty());
    }

    #[test]
    fn a_joined_channel_the_directory_lists_shows_once_with_the_server_count() {
        let mut world = load_world();
        join_channel(&mut world, "oftc", "#debian").expect("oftc exists");
        let rows = channel_listing(&world, "oftc", "");
        let debian: Vec<&ListedChannel> = rows
            .iter()
            .filter(|row| row.name.eq_ignore_ascii_case("#debian"))
            .collect();
        assert_eq!(debian.len(), 1);
        assert_eq!(debian[0].users, 28);
        assert_eq!(debian[0].topic, "Debian GNU/Linux.");
    }

    #[test]
    fn a_joined_channel_with_no_directory_row_counts_its_members() {
        let world = load_world();
        let rows = channel_listing(&world, "oftc", "#lab");
        assert_eq!(rows.len(), 1);
        let lab = world
            .find(&ConversationRef::new("oftc", "#lab"))
            .expect("#lab");
        assert_eq!(rows[0].users, lab.people_count());
    }

    #[test]
    fn the_mask_keeps_matching_names_only() {
        let world = load_world();
        let linux = channel_listing(&world, "omarchy", "#lin*");
        assert_eq!(
            linux.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
            ["#linux"]
        );
        let both = channel_listing(&world, "omarchy", "#linux,#random");
        assert_eq!(both.len(), 2);
        assert!(channel_listing(&world, "omarchy", "#nope").is_empty());
    }

    #[test]
    fn a_mask_part_without_wildcards_must_match_the_whole_name() {
        assert!(list_mask_matches("#linux", "#linux"));
        assert!(list_mask_matches("#linux", "#LINUX"));
        assert!(!list_mask_matches("#linux", "#lin"));
        assert!(!list_mask_matches("#linux", "linux"));
    }

    #[test]
    fn an_empty_mask_or_only_commas_lists_everything() {
        assert!(list_mask_matches("#anything", ""));
        assert!(list_mask_matches("#anything", ","));
        assert!(list_mask_matches("#anything", ",,"));
    }

    #[test]
    fn a_blank_part_matches_nothing_as_qt_does() {
        // Qt splits on commas that are not empty, then skips parts that trim to
        // nothing. A mask with only blank parts therefore lists nothing.
        assert!(!list_mask_matches("#anything", " , "));
    }

    #[test]
    fn a_star_covers_any_run_and_a_question_mark_one_character() {
        assert!(list_mask_matches("#linux", "#lin*"));
        assert!(list_mask_matches("#linux", "*ux"));
        assert!(list_mask_matches("#linux", "*"));
        assert!(list_mask_matches("#linux", "#?inux"));
        assert!(list_mask_matches("#linux", "*x*"));
        assert!(list_mask_matches("axxbyyc", "a*b*c"));
        assert!(!list_mask_matches("#random", "#lin*"));
        assert!(!list_mask_matches("#linux", "#?nux"));
        assert!(!list_mask_matches("axxbyy", "a*b*c"));
    }

    #[test]
    fn wildcards_match_case_insensitively() {
        assert!(list_mask_matches("#Linux", "#LIN*"));
        assert!(list_mask_matches("#LINUX", "#lin?x"));
    }

    #[test]
    fn a_comma_list_mixes_exact_names_and_wildcards() {
        assert!(list_mask_matches("#random", "#linux, #ran*"));
        assert!(!list_mask_matches("#help", "#linux, #ran*"));
    }

    #[test]
    fn rows_list_most_users_first_when_the_query_is_empty() {
        let listing = vec![
            listed("#big", 42, ""),
            listed("#mid", 12, ""),
            listed("#small", 4, ""),
        ];
        let rows = channel_list_rows(&listing, "");
        assert_eq!(names(&rows), ["#big", "#mid", "#small"]);
        assert!(rows.iter().all(|row| row.name_indices.is_empty()));
    }

    #[test]
    fn a_name_hit_outranks_a_topic_only_hit() {
        // The first row's topic has the query, and the second's name does.
        let listing = vec![listed("#busy", 50, "about linux"), listed("#linux", 10, "")];
        let rows = channel_list_rows(&listing, "linux");
        assert_eq!(names(&rows), ["#linux", "#busy"]);
    }

    #[test]
    fn equal_scores_keep_the_most_users_first_order() {
        let listing = vec![listed("#chat-b", 9, ""), listed("#chat-a", 3, "")];
        let rows = channel_list_rows(&listing, "chat");
        assert_eq!(names(&rows), ["#chat-b", "#chat-a"]);
    }

    #[test]
    fn a_query_drops_rows_that_match_neither_name_nor_topic() {
        let listing = vec![listed("#linux", 42, LINUX_TOPIC), listed("#random", 4, "")];
        let rows = channel_list_rows(&listing, "zq");
        assert!(rows.is_empty());
    }

    #[test]
    fn the_name_and_topic_hits_are_reported_for_drawing() {
        // "lin" is contiguous in the name. The topic also holds l, i, n in order,
        // so it reports a weaker hit of its own.
        let listing = vec![listed("#linux", 42, LINUX_TOPIC)];
        let rows = channel_list_rows(&listing, "lin");
        assert_eq!(rows[0].name_indices, [1, 2, 3]);
        assert!(!rows[0].topic_indices.is_empty());
    }

    #[test]
    fn the_topic_keeps_emphasis_and_drops_colours() {
        let rows = channel_list_rows(&[listed("#linux", 42, LINUX_TOPIC)], "");
        let topic = &rows[0].topic;
        assert_eq!(topic.plain, "Kernel discussion and distro help.");
        assert!(topic
            .runs
            .iter()
            .all(|run| run.style.fg.is_none() && run.style.bg.is_none()));
        let kernel = topic
            .runs
            .iter()
            .find(|run| run.range == (0..6))
            .expect("the bold word");
        assert!(kernel.style.bold);
        assert!(topic
            .runs
            .iter()
            .any(|run| run.range.start >= 22 && !run.style.bold));
    }

    #[test]
    fn a_topic_match_is_reported_on_the_plain_text() {
        let rows = channel_list_rows(&[listed("#x", 1, LINUX_TOPIC)], "dist");
        let row = &rows[0];
        assert!(!row.topic_indices.is_empty());
        let picked: String = row
            .topic_indices
            .iter()
            .map(|&at| row.topic.plain[at..].chars().next().expect("a character"))
            .collect();
        assert_eq!(picked, "dist");
    }

    #[test]
    fn the_status_line_reads_like_the_qml_sheet() {
        assert_eq!(listing_status(0, 0, ""), "No channels");
        assert_eq!(listing_status(3, 0, "zq"), "No matches");
        assert_eq!(listing_status(3, 1, "lin"), "1 of 3 channels");
        assert_eq!(listing_status(3, 3, ""), "3 channels");
    }

    #[test]
    fn a_repeat_ask_reuses_the_rows_it_already_has() {
        let world = load_world();
        let mut cache = ChannelListCache::default();
        let first = cache.listing(&world, "omarchy", "").to_vec();
        assert_eq!(first, channel_listing(&world, "omarchy", ""));

        // A world that has changed since the first ask: the cached rows stand.
        let mut moved = world.clone();
        moved.networks[0].directory.push(DirectoryEntry {
            name: "#new".to_string(),
            users: 99,
            topic: String::new(),
        });
        assert_eq!(cache.listing(&moved, "omarchy", "").to_vec(), first);
        assert_eq!(cache.listing(&moved, "omarchy", "  ").to_vec(), first);

        // A different mask is a different answer.
        let masked = cache.listing(&moved, "omarchy", "#NEW").to_vec();
        assert_eq!(masked.len(), 1);
        assert_eq!(masked[0].name, "#new");
    }

    #[test]
    fn a_mask_is_cached_per_network() {
        let world = load_world();
        let mut cache = ChannelListCache::default();
        let omarchy = cache.listing(&world, "omarchy", "#linux").to_vec();
        let oftc = cache.listing(&world, "oftc", "#linux").to_vec();
        assert_eq!(omarchy.len(), 1);
        assert!(oftc.is_empty());
    }
}
