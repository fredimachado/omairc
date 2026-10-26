//! Tracks the tokens of one CAP LS / REQ / ACK / NAK / END exchange: what the
//! server advertised, what we asked for, what it granted and what it refused.
//! The wanted table is the fixed policy. It follows the Go order, which the tests
//! pin. The C++ table lists the read-marker rows before `znc.in/playback`; that
//! changes only the order of tokens within a CAP REQ, never which tokens are
//! requested.
//! Two deliberate differences from Go:
//! - Token names are folded with ASCII case rules only. Every IRCv3 capability
//!   name is ASCII, so a non-ASCII spelling that Unicode folding would map onto
//!   one (such as KELVIN SIGN onto `k`) is unknown here and is ignored.
//! - A request's SASL mechanism is `Option<&'static str>`, `None` when SASL is
//!   not requested, rather than an empty string.

use std::collections::HashMap;

use super::capability::{IrcCapability, IrcCapabilitySet};

/// One CAP REQ outcome: the lines to send, whether SASL is being requested, and
/// the chosen SASL mechanism (`IrcCapabilityNegotiation::Request`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IrcCapabilityRequest {
    pub lines: Vec<String>,
    pub requests_sasl: bool,
    /// `None` unless `requests_sasl`. `SCRAM-SHA-256` when the server listed it
    /// otherwise `PLAIN`.
    pub sasl_mechanism: Option<&'static str>,
}

/// Where one advertised token stands (`TokenState`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TokenState {
    Advertised,
    Requested,
    Enabled,
    Refused,
}

/// One row of the wanted table (`wantedCapability`).
#[derive(Clone, Copy)]
struct Wanted {
    capability: IrcCapability,
    /// The lowercase spelling on the wire. It is also the key in the token map.
    token: &'static str,
    /// A capability this one needs advertised before it can be requested.
    dependency: Option<IrcCapability>,
    needs_credentials: bool,
    /// The token rides its own CAP REQ line, so a NAK aimed at another token in
    /// the same request cannot refuse it.
    own_request_line: bool,
    accepts_value: fn(&str) -> bool,
}

impl Wanted {
    const fn new(capability: IrcCapability, token: &'static str) -> Self {
        Self {
            capability,
            token,
            dependency: None,
            needs_credentials: false,
            own_request_line: false,
            accepts_value: accepts_any_value,
        }
    }

    const fn depends_on(self, dependency: IrcCapability) -> Self {
        Self {
            dependency: Some(dependency),
            ..self
        }
    }

    const fn needs_credentials(self) -> Self {
        Self {
            needs_credentials: true,
            ..self
        }
    }

    const fn own_line(self) -> Self {
        Self {
            own_request_line: true,
            ..self
        }
    }

    const fn accepts(self, accepts_value: fn(&str) -> bool) -> Self {
        Self {
            accepts_value,
            ..self
        }
    }
}

/// The fixed capability policy.
/// REQ line order. `draft/chathistory` follows `chathistory`, and `soju.im/read`
/// follows `draft/read-marker`: each pair names one capability, so the second
/// spelling is requested only when the first one is not requestable.
const WANTED: &[Wanted] = &[
    Wanted::new(IrcCapability::Sasl, "sasl")
        .needs_credentials()
        .own_line()
        .accepts(accepts_sasl_value),
    Wanted::new(IrcCapability::AwayNotify, "away-notify"),
    Wanted::new(IrcCapability::Batch, "batch"),
    Wanted::new(IrcCapability::MemberMetadata, "draft/metadata-2").depends_on(IrcCapability::Batch),
    Wanted::new(IrcCapability::MessageTags, "message-tags").own_line(),
    Wanted::new(IrcCapability::MultiPrefix, "multi-prefix"),
    Wanted::new(IrcCapability::Chghost, "chghost"),
    Wanted::new(IrcCapability::CapNotify, "cap-notify"),
    Wanted::new(IrcCapability::EchoMessage, "echo-message"),
    Wanted::new(IrcCapability::ServerTime, "server-time").own_line(),
    Wanted::new(IrcCapability::ChatHistory, "chathistory").depends_on(IrcCapability::Batch),
    Wanted::new(IrcCapability::ChatHistory, "draft/chathistory").depends_on(IrcCapability::Batch),
    Wanted::new(IrcCapability::ZncPlayback, "znc.in/playback")
        .depends_on(IrcCapability::Batch)
        .own_line(),
    Wanted::new(IrcCapability::LabeledResponse, "labeled-response")
        .depends_on(IrcCapability::MessageTags)
        .own_line(),
    Wanted::new(IrcCapability::AccountTag, "account-tag").depends_on(IrcCapability::MessageTags),
    Wanted::new(IrcCapability::AccountNotify, "account-notify"),
    Wanted::new(IrcCapability::ExtendedJoin, "extended-join"),
    Wanted::new(IrcCapability::ReadMarker, "draft/read-marker"),
    Wanted::new(IrcCapability::ReadMarker, "soju.im/read"),
];

/// Tracks one CAP exchange (`IrcCapabilityNegotiation`). Build one with `new`
/// and call `reset` when a connection starts over.
#[derive(Clone, Debug, Default)]
pub struct IrcCapabilityNegotiation {
    /// Keyed by the wanted table's lowercase token.
    tokens: HashMap<&'static str, TokenState>,
    sasl_advertised_value: String,
    sasl_credentials_available: bool,
}

impl IrcCapabilityNegotiation {
    /// A negotiation with nothing advertised. `sasl_credentials_available` says
    /// whether an account and password are configured, so SASL can be requested.
    pub fn new(sasl_credentials_available: bool) -> Self {
        Self {
            tokens: HashMap::new(),
            sasl_advertised_value: String::new(),
            sasl_credentials_available,
        }
    }

    /// Clears every token and restores the credentials availability.
    pub fn reset(&mut self, sasl_credentials_available: bool) {
        *self = Self::new(sasl_credentials_available);
    }

    /// Records the tokens of a CAP LS. Unknown tokens, and tokens whose advertised
    /// value cannot be used, are ignored. A token already requested or enabled
    /// keeps its state.
    pub fn advertise<S: AsRef<str>>(&mut self, tokens: &[S]) {
        for token in tokens {
            let token = token.as_ref();
            let Some(wanted) = wanted_for(token_name(token)) else {
                continue;
            };
            let value = token_value(token);
            if !(wanted.accepts_value)(value) {
                continue;
            }
            if matches!(
                self.state_of(wanted.token),
                Some(TokenState::Enabled | TokenState::Requested)
            ) {
                continue;
            }
            if wanted.capability == IrcCapability::Sasl {
                self.sasl_advertised_value = value.to_owned();
            }
            self.tokens.insert(wanted.token, TokenState::Advertised);
        }
    }

    /// Forgets the tokens a CAP DEL (or CAP NEW follow-up) removed.
    pub fn withdraw<S: AsRef<str>>(&mut self, tokens: &[S]) {
        for token in tokens {
            let Some(wanted) = wanted_for(token_name(token.as_ref())) else {
                continue;
            };
            if wanted.capability == IrcCapability::Sasl {
                self.sasl_advertised_value.clear();
            }
            self.tokens.remove(wanted.token);
        }
    }

    /// The CAP REQ lines for every token that can be requested now, marking them
    /// requested. Calling it again returns nothing until something new is
    /// advertised.
    pub fn take_request(&mut self) -> IrcCapabilityRequest {
        let mut request = IrcCapabilityRequest::default();
        let mut presence: Vec<&'static str> = Vec::new();

        for wanted in WANTED {
            if !self.is_requestable(wanted) {
                continue;
            }
            if wanted.own_request_line {
                request.lines.push(wanted.token.to_owned());
            } else {
                presence.push(wanted.token);
            }
            if wanted.capability == IrcCapability::Sasl {
                request.requests_sasl = true;
                request.sasl_mechanism = Some(chosen_sasl_mechanism(&self.sasl_advertised_value));
            }
            self.tokens.insert(wanted.token, TokenState::Requested);
        }

        if !presence.is_empty() {
            request.lines.push(presence.join(" "));
        }
        request
    }

    /// Applies a CAP ACK and returns the capabilities it newly enabled. A `-token`
    /// disables an enabled token. An ACK for a token we are not waiting on is
    /// ignored, so a late ACK after a timeout cannot re-enable it.
    pub fn acknowledge<S: AsRef<str>>(&mut self, tokens: &[S]) -> IrcCapabilitySet {
        let mut newly_enabled = IrcCapabilitySet::default();
        for token in tokens {
            let token = token.as_ref();
            let Some(wanted) = wanted_for(token_name(token)) else {
                continue;
            };
            let state = self.state_of(wanted.token);
            if token.starts_with('-') {
                if state == Some(TokenState::Enabled) {
                    self.tokens.insert(wanted.token, TokenState::Refused);
                }
                continue;
            }
            if state != Some(TokenState::Requested) {
                continue;
            }
            let was_enabled = self.any_token(wanted.capability, &[TokenState::Enabled]);
            self.tokens.insert(wanted.token, TokenState::Enabled);
            if !was_enabled {
                newly_enabled.insert(wanted.capability);
            }
        }
        newly_enabled
    }

    /// Applies a CAP NAK and returns the capabilities it refused. Only a token we
    /// are waiting on can be refused.
    pub fn reject<S: AsRef<str>>(&mut self, tokens: &[S]) -> IrcCapabilitySet {
        let mut refused = IrcCapabilitySet::default();
        for token in tokens {
            let Some(wanted) = wanted_for(token_name(token.as_ref())) else {
                continue;
            };
            if self.state_of(wanted.token) != Some(TokenState::Requested) {
                continue;
            }
            self.tokens.insert(wanted.token, TokenState::Refused);
            refused.insert(wanted.capability);
        }
        refused
    }

    /// Refuses every token still waiting on an ACK or NAK, after the CAP timeout.
    /// Returns the capabilities it gave up on.
    pub fn abandon_outstanding(&mut self) -> IrcCapabilitySet {
        let mut abandoned = IrcCapabilitySet::default();
        for wanted in WANTED {
            if self.state_of(wanted.token) != Some(TokenState::Requested) {
                continue;
            }
            self.tokens.insert(wanted.token, TokenState::Refused);
            abandoned.insert(wanted.capability);
        }
        abandoned
    }

    /// True when nothing is still waiting on an ACK or NAK.
    pub fn settled(&self) -> bool {
        !self
            .tokens
            .values()
            .any(|state| *state == TokenState::Requested)
    }

    /// The capabilities the server has acknowledged.
    pub fn enabled(&self) -> IrcCapabilitySet {
        let mut set = IrcCapabilitySet::default();
        for wanted in WANTED {
            if self.state_of(wanted.token) == Some(TokenState::Enabled) {
                set.insert(wanted.capability);
            }
        }
        set
    }

    fn state_of(&self, token: &str) -> Option<TokenState> {
        self.tokens.get(token).copied()
    }

    fn any_token(&self, capability: IrcCapability, states: &[TokenState]) -> bool {
        WANTED
            .iter()
            .filter(|wanted| wanted.capability == capability)
            .any(|wanted| {
                self.state_of(wanted.token)
                    .is_some_and(|state| states.contains(&state))
            })
    }

    fn is_requestable(&self, wanted: &Wanted) -> bool {
        if self.state_of(wanted.token) != Some(TokenState::Advertised) {
            return false;
        }
        if self.any_token(
            wanted.capability,
            &[TokenState::Enabled, TokenState::Requested],
        ) {
            return false;
        }
        if wanted.needs_credentials && !self.sasl_credentials_available {
            return false;
        }
        match wanted.dependency {
            Some(dependency) => self.any_token(
                dependency,
                &[
                    TokenState::Advertised,
                    TokenState::Requested,
                    TokenState::Enabled,
                ],
            ),
            None => true,
        }
    }
}

/// The wanted row for a capability name, matched ASCII-case-insensitively.
fn wanted_for(name: &str) -> Option<&'static Wanted> {
    WANTED
        .iter()
        .find(|wanted| wanted.token.eq_ignore_ascii_case(name))
}

fn accepts_any_value(_value: &str) -> bool {
    true
}

/// Whether a comma-separated SASL value lists `mechanism`, ignoring case and empty
/// entries.
fn sasl_value_lists(value: &str, mechanism: &str) -> bool {
    value
        .split(',')
        .any(|offered| !offered.is_empty() && offered.eq_ignore_ascii_case(mechanism))
}

/// A bare `sasl` (no value) is accepted. Otherwise the value must list PLAIN or
/// SCRAM-SHA-256; EXTERNAL alone is not supported here.
fn accepts_sasl_value(value: &str) -> bool {
    value.is_empty() || sasl_value_lists(value, "PLAIN") || sasl_value_lists(value, "SCRAM-SHA-256")
}

/// SCRAM-SHA-256 wins when the server lists it, otherwise PLAIN.
fn chosen_sasl_mechanism(value: &str) -> &'static str {
    if sasl_value_lists(value, "SCRAM-SHA-256") {
        "SCRAM-SHA-256"
    } else {
        "PLAIN"
    }
}

/// The capability name of a CAP token: everything before the first `=`, without a
/// leading `-`.
fn token_name(token: &str) -> &str {
    let name = token.split_once('=').map_or(token, |(name, _)| name);
    name.strip_prefix('-').unwrap_or(name)
}

/// Everything after the first `=`, or empty.
fn token_value(token: &str) -> &str {
    token.split_once('=').map_or("", |(_, value)| value)
}

#[cfg(test)]
mod tests {
    use super::*;

    // tui/internal/irc/capability_test.go. `tokens` splits a CAP token
    // list the way the Go helper does.

    fn tokens(line: &str) -> Vec<&str> {
        line.split_whitespace().collect()
    }

    fn assert_cap_lines(got: &[String], want: &[&str]) {
        assert_eq!(got, want, "CAP REQ lines");
    }

    fn set_of(capabilities: &[IrcCapability]) -> IrcCapabilitySet {
        let mut set = IrcCapabilitySet::default();
        for &capability in capabilities {
            set.insert(capability);
        }
        set
    }

    #[test]
    fn unwanted_advertisement_produces_no_request() {
        let mut negotiation = IrcCapabilityNegotiation::new(false);
        negotiation.advertise(&tokens("invite-notify"));

        let request = negotiation.take_request();
        assert!(request.lines.is_empty());
        assert!(!request.requests_sasl);
        assert!(negotiation.settled());
        assert!(negotiation.enabled().is_empty());
    }

    #[test]
    fn quiet_caps_request_when_advertised() {
        let mut negotiation = IrcCapabilityNegotiation::new(false);
        negotiation.advertise(&tokens(
            "multi-prefix chghost cap-notify echo-message invite-notify",
        ));

        let request = negotiation.take_request();
        assert_cap_lines(
            &request.lines,
            &["multi-prefix chghost cap-notify echo-message"],
        );
        assert!(!request.requests_sasl);
        assert!(!negotiation.settled(), "not settled before the ACK");

        let granted =
            negotiation.acknowledge(&tokens("multi-prefix chghost cap-notify echo-message"));
        for capability in [
            IrcCapability::MultiPrefix,
            IrcCapability::Chghost,
            IrcCapability::CapNotify,
            IrcCapability::EchoMessage,
        ] {
            assert!(
                granted.contains(capability),
                "granted set missing {capability:?}"
            );
        }
        assert!(negotiation.settled(), "settled after the ACK");
    }

    #[test]
    fn sasl_keeps_its_own_line() {
        let mut negotiation = IrcCapabilityNegotiation::new(true);
        negotiation.advertise(&tokens(
            "sasl=PLAIN,EXTERNAL away-notify batch draft/metadata-2 multi-prefix",
        ));

        let request = negotiation.take_request();
        assert_cap_lines(
            &request.lines,
            &["sasl", "away-notify batch draft/metadata-2 multi-prefix"],
        );
        assert!(request.requests_sasl);
        assert!(!negotiation.settled(), "not settled before the ACK");
    }

    #[test]
    fn message_tags_keeps_its_own_line() {
        let mut negotiation = IrcCapabilityNegotiation::new(true);
        negotiation.advertise(&tokens(
            "sasl=PLAIN message-tags away-notify batch draft/metadata-2",
        ));

        let request = negotiation.take_request();
        assert_cap_lines(
            &request.lines,
            &["sasl", "message-tags", "away-notify batch draft/metadata-2"],
        );
        assert!(request.requests_sasl);

        let refused = negotiation.reject(&tokens("message-tags"));
        assert!(refused.contains(IrcCapability::MessageTags));
        assert!(!negotiation.enabled().contains(IrcCapability::MessageTags));
        assert!(!negotiation.settled(), "sasl is still outstanding");
    }

    #[test]
    fn server_time_keeps_its_own_line() {
        let mut negotiation = IrcCapabilityNegotiation::new(false);
        negotiation.advertise(&tokens("server-time batch multi-prefix"));

        let request = negotiation.take_request();
        assert_cap_lines(&request.lines, &["server-time", "batch multi-prefix"]);

        let refused = negotiation.reject(&tokens("server-time"));
        assert!(refused.contains(IrcCapability::ServerTime));
        assert!(!negotiation.enabled().contains(IrcCapability::ServerTime));
        assert!(
            !negotiation.settled(),
            "batch and multi-prefix are still outstanding"
        );

        let granted = negotiation.acknowledge(&tokens("batch multi-prefix"));
        assert!(granted.contains(IrcCapability::Batch));
        assert!(granted.contains(IrcCapability::MultiPrefix));
        assert!(
            !negotiation.enabled().contains(IrcCapability::ServerTime),
            "server-time stays disabled"
        );
        assert!(negotiation.settled());
    }

    #[test]
    fn sasl_needs_credentials_and_plain() {
        let mut without_credentials = IrcCapabilityNegotiation::new(false);
        without_credentials.advertise(&tokens("sasl=PLAIN"));
        let denied = without_credentials.take_request();
        assert!(denied.lines.is_empty(), "no lines without credentials");
        assert!(!denied.requests_sasl);
        assert_eq!(denied.sasl_mechanism, None);

        let mut external_only = IrcCapabilityNegotiation::new(true);
        external_only.advertise(&tokens("sasl=EXTERNAL"));
        let external = external_only.take_request();
        assert!(external.lines.is_empty(), "no lines for EXTERNAL-only");
        assert!(!external.requests_sasl);
        assert_eq!(external.sasl_mechanism, None);

        let mut plain = IrcCapabilityNegotiation::new(true);
        plain.advertise(&tokens("sasl=PLAIN"));
        let plain_request = plain.take_request();
        assert_cap_lines(&plain_request.lines, &["sasl"]);
        assert!(plain_request.requests_sasl);
        assert_eq!(plain_request.sasl_mechanism, Some("PLAIN"));

        let mut bare = IrcCapabilityNegotiation::new(true);
        bare.advertise(&tokens("sasl"));
        let bare_request = bare.take_request();
        assert_cap_lines(&bare_request.lines, &["sasl"]);
        assert!(bare_request.requests_sasl);
        assert_eq!(bare_request.sasl_mechanism, Some("PLAIN"));
    }

    #[test]
    fn scram_sha_256_is_preferred_when_advertised() {
        let mut both = IrcCapabilityNegotiation::new(true);
        both.advertise(&tokens("sasl=SCRAM-SHA-256,PLAIN"));
        let request = both.take_request();
        assert_cap_lines(&request.lines, &["sasl"]);
        assert!(request.requests_sasl);
        assert_eq!(request.sasl_mechanism, Some("SCRAM-SHA-256"));
        assert_eq!(
            both.take_request(),
            IrcCapabilityRequest::default(),
            "second request is empty"
        );

        let mut reversed = IrcCapabilityNegotiation::new(true);
        reversed.advertise(&tokens("sasl=PLAIN,SCRAM-SHA-256"));
        assert_eq!(
            reversed.take_request().sasl_mechanism,
            Some("SCRAM-SHA-256")
        );

        let mut scram_only = IrcCapabilityNegotiation::new(true);
        scram_only.advertise(&tokens("sasl=SCRAM-SHA-256"));
        let only = scram_only.take_request();
        assert_cap_lines(&only.lines, &["sasl"]);
        assert_eq!(only.sasl_mechanism, Some("SCRAM-SHA-256"));

        let mut folded = IrcCapabilityNegotiation::new(true);
        folded.advertise(&tokens("sasl=scram-sha-256"));
        assert_eq!(folded.take_request().sasl_mechanism, Some("SCRAM-SHA-256"));

        let mut with_external = IrcCapabilityNegotiation::new(true);
        with_external.advertise(&tokens("sasl=EXTERNAL,SCRAM-SHA-256"));
        assert_eq!(
            with_external.take_request().sasl_mechanism,
            Some("SCRAM-SHA-256")
        );

        let mut without_credentials = IrcCapabilityNegotiation::new(false);
        without_credentials.advertise(&tokens("sasl=SCRAM-SHA-256,PLAIN"));
        assert_eq!(
            without_credentials.take_request(),
            IrcCapabilityRequest::default()
        );
    }

    #[test]
    fn member_metadata_needs_batch() {
        let mut negotiation = IrcCapabilityNegotiation::new(false);
        negotiation.advertise(&tokens("draft/metadata-2 away-notify"));
        assert_cap_lines(&negotiation.take_request().lines, &["away-notify"]);

        negotiation.advertise(&tokens("batch"));
        assert_cap_lines(
            &negotiation.take_request().lines,
            &["batch draft/metadata-2"],
        );
    }

    #[test]
    fn acknowledge_and_reject_settle_independently() {
        let mut negotiation = IrcCapabilityNegotiation::new(true);
        negotiation.advertise(&tokens("sasl=PLAIN away-notify batch draft/metadata-2"));
        negotiation.take_request();

        let refused = negotiation.reject(&tokens("away-notify batch draft/metadata-2"));
        assert!(refused.contains(IrcCapability::AwayNotify));
        assert!(refused.contains(IrcCapability::MemberMetadata));
        assert!(!refused.contains(IrcCapability::Sasl));
        assert!(negotiation.enabled().is_empty(), "nothing enabled yet");
        assert!(!negotiation.settled(), "sasl is still outstanding");

        let granted = negotiation.acknowledge(&tokens("sasl"));
        assert!(granted.contains(IrcCapability::Sasl));
        assert!(negotiation.settled());
        assert!(negotiation.enabled().contains(IrcCapability::Sasl));
        assert!(
            !negotiation.enabled().contains(IrcCapability::AwayNotify),
            "away-notify stays refused"
        );
    }

    #[test]
    fn deletion_withdraws_an_enabled_capability() {
        let mut negotiation = IrcCapabilityNegotiation::new(false);
        negotiation.advertise(&tokens("away-notify batch draft/metadata-2"));
        negotiation.take_request();
        negotiation.acknowledge(&tokens("away-notify batch draft/metadata-2"));
        assert!(negotiation.enabled().contains(IrcCapability::AwayNotify));

        negotiation.withdraw(&tokens("away-notify"));
        assert!(!negotiation.enabled().contains(IrcCapability::AwayNotify));
        assert!(negotiation
            .enabled()
            .contains(IrcCapability::MemberMetadata));
        assert!(
            negotiation.take_request().lines.is_empty(),
            "nothing new is requestable"
        );
    }

    #[test]
    fn timeout_abandons_outstanding_requests() {
        let mut negotiation = IrcCapabilityNegotiation::new(false);
        negotiation.advertise(&tokens("away-notify batch"));
        negotiation.take_request();
        assert!(!negotiation.settled(), "not settled after the request");

        let abandoned = negotiation.abandon_outstanding();
        assert!(abandoned.contains(IrcCapability::AwayNotify));
        assert!(abandoned.contains(IrcCapability::Batch));
        assert!(negotiation.settled(), "settled after the timeout");
        assert!(
            negotiation.enabled().is_empty(),
            "nothing enabled after the timeout"
        );
    }

    #[test]
    fn request_is_idempotent_until_something_new_is_advertised() {
        let mut negotiation = IrcCapabilityNegotiation::new(false);
        negotiation.advertise(&tokens("away-notify"));
        assert_cap_lines(&negotiation.take_request().lines, &["away-notify"]);
        assert!(
            negotiation.take_request().lines.is_empty(),
            "the second request is empty"
        );

        negotiation.acknowledge(&tokens("away-notify"));
        assert!(
            negotiation.take_request().lines.is_empty(),
            "empty after the ACK"
        );

        negotiation.reset(false);
        assert!(
            negotiation.enabled().is_empty(),
            "reset clears enabled capabilities"
        );
        assert!(
            negotiation.take_request().lines.is_empty(),
            "reset clears advertised tokens"
        );
    }

    #[test]
    fn dual_chat_history_offer_requests_stable_token() {
        let mut negotiation = IrcCapabilityNegotiation::new(false);
        negotiation.advertise(&tokens("batch chathistory draft/chathistory"));

        let request = negotiation.take_request();
        assert_cap_lines(&request.lines, &["batch chathistory"]);
        assert!(!request.lines.join(" ").contains("draft/chathistory"));
    }

    #[test]
    fn draft_only_chat_history_requests_draft_token() {
        let mut negotiation = IrcCapabilityNegotiation::new(false);
        negotiation.advertise(&tokens("batch draft/chathistory"));
        assert_cap_lines(
            &negotiation.take_request().lines,
            &["batch draft/chathistory"],
        );
    }

    #[test]
    fn chat_history_without_batch_is_not_requested() {
        let mut negotiation = IrcCapabilityNegotiation::new(false);
        negotiation.advertise(&tokens("chathistory draft/chathistory"));
        assert!(
            negotiation.take_request().lines.is_empty(),
            "no lines without batch"
        );
    }

    #[test]
    fn deleting_stable_chat_history_keeps_draft_advertised() {
        let mut negotiation = IrcCapabilityNegotiation::new(false);
        negotiation.advertise(&tokens("batch chathistory draft/chathistory"));
        negotiation.take_request();
        negotiation.acknowledge(&tokens("batch chathistory"));
        assert!(negotiation.enabled().contains(IrcCapability::ChatHistory));

        negotiation.withdraw(&tokens("chathistory"));
        assert_cap_lines(&negotiation.take_request().lines, &["draft/chathistory"]);
    }

    #[test]
    fn deleting_unused_draft_chat_history_keeps_stable_enabled() {
        let mut negotiation = IrcCapabilityNegotiation::new(false);
        negotiation.advertise(&tokens("batch chathistory draft/chathistory"));
        negotiation.take_request();
        negotiation.acknowledge(&tokens("batch chathistory"));
        assert!(negotiation.enabled().contains(IrcCapability::ChatHistory));

        negotiation.withdraw(&tokens("draft/chathistory"));
        assert!(
            negotiation.enabled().contains(IrcCapability::ChatHistory),
            "chat history stays enabled"
        );
        assert!(
            negotiation.take_request().lines.is_empty(),
            "nothing new is requestable"
        );
    }

    #[test]
    fn nak_of_stable_chat_history_requests_draft_token() {
        let mut negotiation = IrcCapabilityNegotiation::new(false);
        negotiation.advertise(&tokens("batch chathistory draft/chathistory"));
        assert_cap_lines(&negotiation.take_request().lines, &["batch chathistory"]);
        negotiation.acknowledge(&tokens("batch"));
        negotiation.reject(&tokens("chathistory"));
        assert_cap_lines(&negotiation.take_request().lines, &["draft/chathistory"]);
    }

    #[test]
    fn late_acknowledge_after_timeout_is_ignored() {
        let mut negotiation = IrcCapabilityNegotiation::new(false);
        negotiation.advertise(&tokens("away-notify"));
        negotiation.take_request();
        negotiation.abandon_outstanding();

        assert!(
            negotiation.acknowledge(&tokens("away-notify")).is_empty(),
            "a late ACK grants nothing"
        );
        assert!(negotiation.enabled().is_empty());
        assert!(negotiation.settled(), "negotiation stays settled");
    }

    #[test]
    fn late_reject_after_timeout_is_ignored() {
        let mut negotiation = IrcCapabilityNegotiation::new(true);
        negotiation.advertise(&tokens("sasl"));
        assert!(negotiation.take_request().requests_sasl);
        negotiation.abandon_outstanding();

        assert!(
            negotiation.reject(&tokens("sasl")).is_empty(),
            "a late NAK refuses nothing"
        );
    }

    #[test]
    fn acknowledging_either_chat_history_spelling_settles() {
        let mut negotiation = IrcCapabilityNegotiation::new(false);
        negotiation.advertise(&tokens("batch draft/chathistory"));
        assert_cap_lines(
            &negotiation.take_request().lines,
            &["batch draft/chathistory"],
        );

        negotiation.acknowledge(&tokens("batch draft/chathistory"));
        assert!(negotiation.settled());
        assert!(negotiation.enabled().contains(IrcCapability::ChatHistory));
        assert!(
            negotiation.take_request().lines.is_empty(),
            "nothing new is requestable"
        );
    }

    #[test]
    fn acknowledged_removal_disables_the_capability() {
        let mut negotiation = IrcCapabilityNegotiation::new(false);
        negotiation.advertise(&tokens("batch chathistory"));
        negotiation.take_request();
        negotiation.acknowledge(&tokens("batch chathistory"));
        assert!(negotiation.enabled().contains(IrcCapability::ChatHistory));

        negotiation.acknowledge(&tokens("-chathistory"));
        assert!(
            !negotiation.enabled().contains(IrcCapability::ChatHistory),
            "disabled after -chathistory"
        );
        assert!(
            negotiation.enabled().contains(IrcCapability::Batch),
            "batch stays enabled"
        );
    }

    #[test]
    fn sts_is_advertised_but_never_requested() {
        let mut negotiation = IrcCapabilityNegotiation::new(false);
        negotiation.advertise(&tokens("sts=port=6697,duration=60 multi-prefix cap-notify"));

        let request = negotiation.take_request();
        assert_cap_lines(&request.lines, &["multi-prefix cap-notify"]);
        assert!(!negotiation.settled(), "not settled before the ACK");
    }

    #[test]
    fn labeled_response_keeps_its_own_line() {
        let mut negotiation = IrcCapabilityNegotiation::new(false);
        negotiation.advertise(&tokens("message-tags labeled-response away-notify"));

        let request = negotiation.take_request();
        assert_cap_lines(
            &request.lines,
            &["message-tags", "labeled-response", "away-notify"],
        );
        assert!(!negotiation.settled(), "not settled before the ACK");

        let refused = negotiation.reject(&tokens("labeled-response"));
        assert!(refused.contains(IrcCapability::LabeledResponse));
        assert!(!negotiation
            .enabled()
            .contains(IrcCapability::LabeledResponse));
        assert!(!negotiation.settled(), "message-tags is still outstanding");

        let granted = negotiation.acknowledge(&tokens("message-tags away-notify"));
        assert!(granted.contains(IrcCapability::MessageTags));
        assert!(granted.contains(IrcCapability::AwayNotify));
        assert!(
            !negotiation
                .enabled()
                .contains(IrcCapability::LabeledResponse),
            "labeled-response stays disabled"
        );
        assert!(negotiation.settled());
    }

    #[test]
    fn account_caps_request_when_advertised() {
        let mut negotiation = IrcCapabilityNegotiation::new(false);
        negotiation.advertise(&tokens(
            "message-tags account-tag account-notify extended-join",
        ));

        assert_cap_lines(
            &negotiation.take_request().lines,
            &["message-tags", "account-tag account-notify extended-join"],
        );
    }

    #[test]
    fn account_tag_needs_message_tags() {
        let mut negotiation = IrcCapabilityNegotiation::new(false);
        negotiation.advertise(&tokens("account-tag account-notify extended-join"));

        assert_cap_lines(
            &negotiation.take_request().lines,
            &["account-notify extended-join"],
        );
    }

    #[test]
    fn partial_account_advertisement_requests_only_present_caps() {
        let mut only_notify = IrcCapabilityNegotiation::new(false);
        only_notify.advertise(&tokens("account-notify"));
        assert_cap_lines(&only_notify.take_request().lines, &["account-notify"]);

        let mut tag_and_join = IrcCapabilityNegotiation::new(false);
        tag_and_join.advertise(&tokens("message-tags account-tag extended-join"));
        assert_cap_lines(
            &tag_and_join.take_request().lines,
            &["message-tags", "account-tag extended-join"],
        );

        let mut none = IrcCapabilityNegotiation::new(false);
        none.advertise(&tokens("invite-notify"));
        assert!(none.take_request().lines.is_empty(), "no lines expected");
        assert!(none.settled(), "settled");
    }

    #[test]
    fn account_cap_ack_enables_matching_bits() {
        let mut negotiation = IrcCapabilityNegotiation::new(false);
        negotiation.advertise(&tokens(
            "message-tags account-tag account-notify extended-join",
        ));
        negotiation.take_request();

        let granted = negotiation.acknowledge(&tokens(
            "message-tags account-tag account-notify extended-join",
        ));
        for capability in [
            IrcCapability::AccountTag,
            IrcCapability::AccountNotify,
            IrcCapability::ExtendedJoin,
        ] {
            assert!(
                granted.contains(capability),
                "granted set missing {capability:?}"
            );
            assert!(
                negotiation.enabled().contains(capability),
                "enabled set missing {capability:?}"
            );
        }
        assert!(negotiation.settled());
    }

    #[test]
    fn znc_playback_needs_batch_and_its_own_line() {
        let mut without_batch = IrcCapabilityNegotiation::new(false);
        without_batch.advertise(&tokens("znc.in/playback multi-prefix"));
        assert_cap_lines(&without_batch.take_request().lines, &["multi-prefix"]);
        assert!(!without_batch.enabled().contains(IrcCapability::ZncPlayback));

        let mut negotiation = IrcCapabilityNegotiation::new(false);
        negotiation.advertise(&tokens("znc.in/playback batch server-time"));
        assert_cap_lines(
            &negotiation.take_request().lines,
            &["server-time", "znc.in/playback", "batch"],
        );

        let granted = negotiation.acknowledge(&tokens("znc.in/playback batch server-time"));
        for capability in [
            IrcCapability::ZncPlayback,
            IrcCapability::Batch,
            IrcCapability::ServerTime,
        ] {
            assert!(
                granted.contains(capability),
                "granted set missing {capability:?}"
            );
        }
        assert!(negotiation.enabled().contains(IrcCapability::ZncPlayback));
        assert!(negotiation.settled());
    }

    #[test]
    fn znc_playback_absent_is_not_requested() {
        let mut negotiation = IrcCapabilityNegotiation::new(false);
        negotiation.advertise(&tokens("batch server-time multi-prefix"));
        let lines = negotiation.take_request().lines;
        assert!(
            lines.iter().all(|line| !line.contains("znc.in/playback")),
            "lines = {lines:?}"
        );

        negotiation.acknowledge(&tokens("batch server-time multi-prefix"));
        assert!(
            !negotiation.enabled().contains(IrcCapability::ZncPlayback),
            "znc playback stays disabled"
        );
    }

    #[test]
    fn labeled_response_needs_message_tags() {
        let mut negotiation = IrcCapabilityNegotiation::new(false);
        negotiation.advertise(&tokens("labeled-response away-notify"));
        assert_cap_lines(&negotiation.take_request().lines, &["away-notify"]);

        negotiation.advertise(&tokens("message-tags"));
        assert_cap_lines(
            &negotiation.take_request().lines,
            &["message-tags", "labeled-response"],
        );
    }

    #[test]
    fn enabled_set_lists_only_acknowledged_capabilities() {
        let mut negotiation = IrcCapabilityNegotiation::new(false);
        negotiation.advertise(&tokens("away-notify batch"));
        negotiation.take_request();
        negotiation.acknowledge(&tokens("away-notify"));

        assert_eq!(negotiation.enabled(), set_of(&[IrcCapability::AwayNotify]));
    }

    // tui/internal/irc/capability_readmarker_test.go.

    #[test]
    fn dual_read_marker_offer_requests_draft_token() {
        let mut negotiation = IrcCapabilityNegotiation::new(false);
        negotiation.advertise(&tokens("draft/read-marker soju.im/read"));

        let request = negotiation.take_request();
        assert_cap_lines(&request.lines, &["draft/read-marker"]);
        assert!(
            !request
                .lines
                .join(" ")
                .to_ascii_lowercase()
                .contains("soju.im/read"),
            "must not request soju.im/read when draft/read-marker is advertised"
        );
    }

    #[test]
    fn soju_only_read_marker_requests_soju_token() {
        let mut negotiation = IrcCapabilityNegotiation::new(false);
        negotiation.advertise(&tokens("soju.im/read"));
        assert_cap_lines(&negotiation.take_request().lines, &["soju.im/read"]);
    }

    #[test]
    fn capability_names_match_case_insensitively_and_keep_the_wire_spelling() {
        let mut negotiation = IrcCapabilityNegotiation::new(false);
        negotiation.advertise(&tokens("Away-Notify"));
        assert_cap_lines(&negotiation.take_request().lines, &["away-notify"]);
    }

    #[test]
    fn a_non_ascii_lookalike_name_is_ignored() {
        // KELVIN SIGN folds to `k` under Unicode rules. Here it stays unknown.
        let mut negotiation = IrcCapabilityNegotiation::new(false);
        negotiation.advertise(&tokens("draft/read-ma\u{212A}er"));
        assert!(negotiation.take_request().lines.is_empty());
    }
}
