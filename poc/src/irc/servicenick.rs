//! Port of `tui/internal/irc/servicenick.go` (Go) and `src/irc/ircservicenick.cpp`
//! (C++). Neither source has a test of its own; the cases below follow the
//! documented contract and the session cases that depend on it.

use crate::irc::message::IrcMessage;
use crate::irc::serverfeatures::IrcServerFeatures;
use crate::irc::wiretext::wire_text;

/// Whether the source of `message` is a network service. A line with no source is
/// never a service. The source nick and host go through `is_service_identity`
/// with the network's advertised CHANTYPES.
pub fn is_service_source(message: &IrcMessage, features: &IrcServerFeatures) -> bool {
    message.prefix.as_ref().is_some_and(|prefix| {
        is_service_identity(
            &wire_text(&prefix.nick),
            &wire_text(&prefix.host),
            features.channel_types(),
        )
    })
}

/// Targets that name the network rather than a person: empty, `*` or `AUTH`.
pub fn is_network_notice_target(target: &str) -> bool {
    target.is_empty() || target == "*" || target.eq_ignore_ascii_case("AUTH")
}

/// Whether a message source is a network service rather than a person: a nick
/// ending in `serv` that is not a channel, or a host folding to `services` or
/// `services.*`. `channel_types` is the advertised CHANTYPES; when it is empty, a
/// target whose first character cannot start a nick counts as a channel.
pub fn is_service_identity(nick: &str, host: &str, channel_types: &str) -> bool {
    if !nick.is_empty() && !looks_like_channel(nick, channel_types) && ends_with_fold(nick, "serv")
    {
        return true;
    }
    if host.is_empty() {
        return false;
    }
    let folded = host.to_lowercase();
    folded == "services" || folded.starts_with("services.")
}

/// Whether a PRIVMSG to this nick reaches a person, so a conversation opened for
/// it is one the user can reply in. This is not a nick-grammar test: a leading
/// digit stays routable, and a bouncer pseudo-client such as `*status` does not.
pub fn nick_is_routable(nick: &str) -> bool {
    !nick.is_empty()
        && !nick
            .chars()
            .any(|mark| mark < '\u{20}' || " ,*?!@.$:".contains(mark))
}

/// Whether `mark` may begin a nick: an ASCII letter or one of the RFC specials.
pub(super) fn can_start_nick(mark: char) -> bool {
    mark.is_ascii_alphabetic()
        || matches!(
            mark,
            '-' | '[' | ']' | '\\' | '`' | '_' | '^' | '{' | '|' | '}'
        )
}

/// A target is a channel when its first character is a CHANTYPES entry, or, with
/// no CHANTYPES, when it cannot begin a nick.
pub(super) fn looks_like_channel(target: &str, channel_types: &str) -> bool {
    let Some(mark) = target.chars().next() else {
        return false;
    };
    if !channel_types.is_empty() {
        return channel_types.contains(mark);
    }
    !can_start_nick(mark)
}

/// ASCII case-insensitive suffix match. Compares bytes, so a multibyte scalar
/// cannot be split by the slice.
fn ends_with_fold(text: &str, suffix: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() >= suffix.len()
        && bytes[bytes.len() - suffix.len()..].eq_ignore_ascii_case(suffix.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::irc::message::IrcPrefix;

    #[test]
    fn a_serv_nick_is_a_service_when_it_is_not_a_channel() {
        assert!(is_service_identity("NickServ", "", ""));
        assert!(is_service_identity("ChanServ", "", ""));
        assert!(is_service_identity("NICKSERV", "", ""));
        assert!(is_service_identity("serv", "", ""));
        assert!(!is_service_identity("bob", "", ""));
    }

    #[test]
    fn a_channel_named_like_a_service_is_not_a_service() {
        assert!(!is_service_identity("#serv", "", "#&"));
        assert!(!is_service_identity("&serv", "", "#&"));
        // With no CHANTYPES, a target that cannot start a nick is a channel.
        assert!(!is_service_identity("~serv", "", ""));
        assert!(!is_service_identity("$serv", "", ""));
        // With CHANTYPES=$, `$serv` is the channel and `#serv` is a nick.
        assert!(!is_service_identity("$serv", "", "$"));
        assert!(is_service_identity("#serv", "", "$"));
    }

    #[test]
    fn a_services_host_marks_a_service_whatever_the_nick() {
        assert!(is_service_identity("bob", "services", ""));
        assert!(is_service_identity("bob", "Services.Example.Net", ""));
        assert!(!is_service_identity("bob", "servicesx.net", ""));
        assert!(!is_service_identity("bob", "example.org", ""));
        assert!(!is_service_identity("", "", ""));
    }

    #[test]
    fn routable_nicks_have_no_separators_or_control_characters() {
        assert!(nick_is_routable("nick"));
        assert!(nick_is_routable("nick_"));
        assert!(nick_is_routable("[nick]"));
        assert!(nick_is_routable("0day"));
        assert!(!nick_is_routable(""));
        assert!(!nick_is_routable("*status"));
        for unroutable in [
            "a b", "a,b", "a?b", "a!b", "a@b", "a.b", "a$", "a:b", "a\u{1}",
        ] {
            assert!(!nick_is_routable(unroutable), "{unroutable:?}");
        }
    }

    fn sourced(nick: &str, user: &str, host: &str) -> IrcMessage {
        IrcMessage {
            prefix: Some(IrcPrefix {
                raw: format!("{nick}!{user}@{host}").into_bytes(),
                nick: nick.into(),
                user: user.into(),
                host: host.into(),
            }),
            ..IrcMessage::default()
        }
    }

    #[test]
    fn a_service_source_is_judged_by_its_nick_and_host() {
        let features = IrcServerFeatures::new();
        assert!(is_service_source(
            &sourced("NickServ", "svc", "irc.example.net"),
            &features
        ));
        assert!(is_service_source(
            &sourced("bob", "b", "services.example.net"),
            &features
        ));
        assert!(!is_service_source(
            &sourced("bob", "b", "example.org"),
            &features
        ));
        assert!(!is_service_source(&IrcMessage::default(), &features));
    }

    #[test]
    fn network_notice_targets_are_empty_star_or_auth() {
        assert!(is_network_notice_target(""));
        assert!(is_network_notice_target("*"));
        assert!(is_network_notice_target("auth"));
        assert!(!is_network_notice_target("#auth"));
        assert!(!is_network_notice_target("bob"));
    }
}
