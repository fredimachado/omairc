//! Per-nick presence metadata: the avatar a nick shows, whether it is a bot, and
//! its real name. Qt keeps these per network (the `761` numerics and the bot
//! metadata), so a nick's record in any channel of the network answers for all
//! of them. Toolkit-free: the GPUI widgets draw what these functions return.

use std::collections::HashMap;

use super::mock::Network;
pub use crate::irc::presence::NickFacts;

/// The prefix of a bundled image reference. Qt's demo seed writes it that way.
const BUNDLED_PREFIX: &str = "qrc:/";

/// The facts for every nick on `network`, keyed by nick. Each member record folds
/// in through `NickFacts::absorb`, so a nick listed in several channels keeps its
/// real name and avatar even where one record leaves them out.
pub fn nick_facts(network: &Network) -> HashMap<String, NickFacts> {
    let mut facts: HashMap<String, NickFacts> = HashMap::new();
    for member in network.conversations.iter().flat_map(|conv| &conv.members) {
        facts.entry(member.nick.clone()).or_default().absorb(
            &member.realname,
            member.bot,
            member.avatar.as_deref(),
        );
    }
    facts
}

/// The bundled image path for a `qrc:/` reference (`demo/mira-avatar.png`), or
/// `None` for anything else, such as the https avatar.
pub fn bundled_path(reference: &str) -> Option<&str> {
    reference.strip_prefix(BUNDLED_PREFIX)
}

/// The photo to draw for an avatar reference, given the `avatars` preference.
/// `None` means the initials. An https avatar is never fetched: gpui 0.2.2 ships
/// no HTTP client (its default refuses every request), so it keeps the initials,
/// which is also Qt's fallback when its own fetch fails.
pub fn photo_for(reference: Option<&str>, avatars_on: bool) -> Option<&str> {
    if !avatars_on {
        return None;
    }
    reference.and_then(bundled_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::mock::{load_world, Conversation, Member, Network};

    fn member(nick: &str, realname: &str, bot: bool, avatar: Option<&str>) -> Member {
        Member {
            nick: nick.to_string(),
            prefix: String::new(),
            realname: realname.to_string(),
            presence: "online".to_string(),
            bot,
            status: None,
            avatar: avatar.map(str::to_string),
        }
    }

    fn channel(name: &str, members: Vec<Member>) -> Conversation {
        Conversation {
            name: name.to_string(),
            channel: true,
            topic: None,
            peer_realname: None,
            unread: 0,
            mention: false,
            typing: false,
            members,
            messages: Vec::new(),
        }
    }

    fn network(conversations: Vec<Conversation>) -> Network {
        Network {
            id: "net".to_string(),
            display_name: "net".to_string(),
            icon_color: None,
            nick: "me".to_string(),
            status: Vec::new(),
            last_error: None,
            chantypes: "#".to_string(),
            icon: None,
            directory: Vec::new(),
            conversations,
        }
    }

    #[test]
    fn bundled_references_map_to_their_asset_path() {
        assert_eq!(
            bundled_path("qrc:/demo/mira-avatar.png"),
            Some("demo/mira-avatar.png")
        );
        assert_eq!(bundled_path("https://omairc.app/omairc128.png"), None);
        assert_eq!(bundled_path("demo/mira-avatar.png"), None);
    }

    #[test]
    fn the_avatars_pref_gates_the_photo() {
        let mira = Some("qrc:/demo/mira-avatar.png");
        assert_eq!(photo_for(mira, true), Some("demo/mira-avatar.png"));
        assert_eq!(photo_for(mira, false), None);
    }

    #[test]
    fn an_https_or_missing_avatar_falls_back_to_initials() {
        assert_eq!(
            photo_for(Some("https://omairc.app/omairc128.png"), true),
            None
        );
        assert_eq!(photo_for(None, true), None);
    }

    #[test]
    fn the_seeded_facts_for_omarchy() {
        let world = load_world();
        let facts = nick_facts(world.network("omarchy").expect("omarchy"));
        let mira = &facts["mira"];
        assert_eq!(mira.realname, "Mira Chen");
        assert_eq!(mira.avatar.as_deref(), Some("qrc:/demo/mira-avatar.png"));
        assert!(!mira.bot);
        assert!(facts["dax"].bot);
        assert_eq!(facts["dax"].realname, "Packet Bot");
        // kai has an avatar and no real name in the seed.
        assert_eq!(facts["kai"].realname, "");
        assert_eq!(facts["sol"].avatar, None);
    }

    #[test]
    fn a_real_name_or_avatar_missing_in_one_channel_comes_from_another() {
        let net = network(vec![
            channel("#a", vec![member("ann", "", false, None)]),
            channel(
                "#b",
                vec![member("ann", "Ann Lee", true, Some("qrc:/demo/ann.png"))],
            ),
        ]);
        let facts = nick_facts(&net);
        assert_eq!(facts["ann"].realname, "Ann Lee");
        assert_eq!(facts["ann"].avatar.as_deref(), Some("qrc:/demo/ann.png"));
        assert!(facts["ann"].bot);
    }

    #[test]
    fn the_first_avatar_wins_over_a_later_one() {
        let net = network(vec![
            channel("#a", vec![member("ann", "", false, Some("qrc:/one.png"))]),
            channel("#b", vec![member("ann", "", false, Some("qrc:/two.png"))]),
        ]);
        assert_eq!(
            nick_facts(&net)["ann"].avatar.as_deref(),
            Some("qrc:/one.png")
        );
    }

    #[test]
    fn member_for_answers_from_the_first_channel_that_lists_the_nick() {
        let world = load_world();
        let omarchy = world.network("omarchy").expect("omarchy");
        let anna = omarchy.member_for("anna").expect("anna");
        assert_eq!(anna.prefix, "&");
        assert!(omarchy.member_for("nobody").is_none());
    }
}
