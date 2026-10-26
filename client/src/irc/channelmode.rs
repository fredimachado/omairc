//! Port of `tui/internal/irc/channelmode.go`.
//!
//! Parses a `/mode` argument into a query or a change. The channel must start
//! with a type the server advertised, so CHANTYPES decides what counts as a channel.

use super::parser::contains_forbidden;
use super::serverfeatures::IrcServerFeatures;

/// A parsed `/mode` argument (`IrcChannelModeRequest`). `query` is true for a bare
/// `MODE <channel>`. Otherwise `modes` and `parameters` describe a change.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IrcChannelModeRequest {
    pub channel: String,
    pub modes: String,
    pub parameters: Vec<String>,
    pub query: bool,
}

/// Parses a `/mode` argument against `features`. It splits on single spaces,
/// rejects CR, LF and NUL, and requires the first token to be an advertised
/// channel. `None` for empty input, a forbidden character, or a non-channel start.
pub fn parse_channel_mode_request(argument: &str, features: &IrcServerFeatures) -> Option<IrcChannelModeRequest> {
    let tokens: Vec<&str> = argument.split(' ').filter(|token| !token.is_empty()).collect();
    let (channel, rest) = tokens.split_first()?;
    if tokens.iter().any(|token| contains_forbidden(token.as_bytes())) {
        return None;
    }
    if !features.is_channel(channel) {
        return None;
    }

    match rest {
        [] => Some(IrcChannelModeRequest {
            channel: (*channel).to_owned(),
            query: true,
            ..IrcChannelModeRequest::default()
        }),
        [modes, parameters @ ..] => Some(IrcChannelModeRequest {
            channel: (*channel).to_owned(),
            modes: (*modes).to_owned(),
            parameters: parameters.iter().map(|&parameter| parameter.to_owned()).collect(),
            query: false,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channel_mode_features() -> IrcServerFeatures {
        let mut features = IrcServerFeatures::new();
        features.apply_token("CHANTYPES=#");
        features
    }

    #[test]
    fn parses_a_query() {
        let features = channel_mode_features();
        for argument in ["#chan", "  #chan  ", "#chan"] {
            let request = parse_channel_mode_request(argument, &features)
                .unwrap_or_else(|| panic!("parse_channel_mode_request({argument:?}) refused"));
            assert!(request.query);
            assert_eq!(request.channel, "#chan");
            assert_eq!(request.modes, "");
            assert!(request.parameters.is_empty());
        }
    }

    #[test]
    fn parses_a_change() {
        let features = channel_mode_features();
        let request = parse_channel_mode_request("#chan +o alice", &features).expect("valid change");
        assert!(!request.query);
        assert_eq!(request.channel, "#chan");
        assert_eq!(request.modes, "+o");
        assert_eq!(request.parameters, ["alice"]);
    }

    #[test]
    fn parses_multiple_parameters() {
        let features = channel_mode_features();
        let request = parse_channel_mode_request("#chan +ov alice bob", &features).expect("valid change");
        assert_eq!(request.modes, "+ov");
        assert_eq!(request.parameters, ["alice", "bob"]);
    }

    #[test]
    fn rejects_a_non_channel() {
        let features = channel_mode_features();
        for argument in ["alice +o bob", "&other +o bob"] {
            assert!(parse_channel_mode_request(argument, &features).is_none(), "{argument:?} should be refused");
        }
    }

    #[test]
    fn rejects_empty_input() {
        let features = channel_mode_features();
        for argument in ["", "   "] {
            assert!(parse_channel_mode_request(argument, &features).is_none(), "{argument:?} should be refused");
        }
    }

    #[test]
    fn rejects_control_characters() {
        let features = channel_mode_features();
        for argument in ["#chan +o a\rb", "#chan\n+o alice", "#chan\0", "#chan +o ali\0ce"] {
            assert!(parse_channel_mode_request(argument, &features).is_none(), "{argument:?} should be refused");
        }
    }

    #[test]
    fn honors_channel_types() {
        let mut features = IrcServerFeatures::new();
        features.apply_token("CHANTYPES=&");
        assert!(parse_channel_mode_request("#chan", &features).is_none(), "#chan is not a channel under &");
        let request = parse_channel_mode_request("&chan +o alice", &features).expect("&chan parses under &");
        assert_eq!(request.channel, "&chan");
    }
}
