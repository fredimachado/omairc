//! Port of `tui/internal/controller/channellist.go` (Go) and `src/irc/channellistmodel.cpp`
//! (C++): the answer to `LIST mask` on a network. The server's list is what the demo
//! answers with on the wire (`answers::list_entries`: its seeded channels, then its
//! directory). Each joined channel the list does not name is added, and the mask keeps
//! the rows it matches. The order is most users first, then by name.
//!
//! The model's sheet ranks this answer for the query; it does not build it.

use crate::demo::answers::{list_entries, list_mask_matches};
use crate::demo::seed::world_for;

use super::controller::Controller;

/// One channel as a `LIST` answer gives it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListedChannel {
    pub name: String,
    pub users: usize,
    /// The topic as the server sent it, with its mIRC codes.
    pub topic: String,
}

impl Controller {
    /// What `LIST mask` answers on `network_id`. Empty for an unknown network with
    /// nothing joined. A joined channel the server's list names shows that row once,
    /// because the server's count is the one that counts.
    pub fn channel_listing(&self, network_id: &str, mask: &str) -> Vec<ListedChannel> {
        let mask = mask.trim();
        let mut rows: Vec<ListedChannel> = match (self.demo.is_some(), world_for(network_id)) {
            (true, Some(network)) => list_entries(network)
                .into_iter()
                .map(|entry| ListedChannel {
                    name: entry.name.to_owned(),
                    users: entry.users,
                    topic: entry.topic.to_owned(),
                })
                .collect(),
            _ => Vec::new(),
        };
        for (key, conversation) in self.reducer.conversations().iter() {
            let Some(channel) = conversation.channel.as_ref() else {
                continue;
            };
            if key.network_id != network_id {
                continue;
            }
            let listed = rows
                .iter()
                .any(|row| row.name.to_lowercase() == conversation.target.to_lowercase());
            if !listed {
                rows.push(ListedChannel {
                    name: conversation.target.clone(),
                    users: conversation.people_count(),
                    topic: channel.topic.clone(),
                });
            }
        }
        rows.retain(|row| list_mask_matches(&row.name, mask));
        rows.sort_by(|left, right| {
            right
                .users
                .cmp(&left.users)
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        });
        rows
    }
}
