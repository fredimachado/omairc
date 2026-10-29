//! Ban and quiet list masks: the list-mode kind and the nick-to-mask expansion
//! shared by `/ban`, `/kickban`, `/unban`, `/quiet` and `/unquiet`.

/// Which list mode a wrapper or inquiry targets (`ListMaskKind`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListMaskKind {
    /// Channel ban list mode `b`.
    Ban,
    /// Channel quiet list mode `q` (CHANMODES A).
    Quiet,
}

impl ListMaskKind {
    /// The MODE letter this kind inquires or sets.
    pub fn mode_letter(self) -> char {
        match self {
            ListMaskKind::Ban => 'b',
            ListMaskKind::Quiet => 'q',
        }
    }
}

/// Expands a nick or hostmask the way `/ban` does: a token with no `!` or `@`
/// becomes `nick!*@*`; otherwise the token is kept as typed.
pub fn expand_list_mask(token: &str) -> String {
    if token.contains('!') || token.contains('@') {
        token.to_owned()
    } else {
        format!("{token}!*@*")
    }
}

/// The nick `/kickban` kicks when the typed token is a mask: the text before the
/// first `!`, or the whole token when there is none.
pub fn kick_nick_from_mask(token: &str) -> &str {
    token.split_once('!').map_or(token, |(nick, _)| nick)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_list_mask_appends_host_wildcards_for_a_bare_nick() {
        assert_eq!(expand_list_mask("bob"), "bob!*@*");
        assert_eq!(expand_list_mask("bob!*@*"), "bob!*@*");
        assert_eq!(expand_list_mask("bob!u@h"), "bob!u@h");
        assert_eq!(expand_list_mask("*@evil.example"), "*@evil.example");
    }

    #[test]
    fn kick_nick_from_mask_takes_the_text_before_bang() {
        assert_eq!(kick_nick_from_mask("bob"), "bob");
        assert_eq!(kick_nick_from_mask("bob!*@*"), "bob");
        assert_eq!(kick_nick_from_mask("bob!u@h"), "bob");
        assert_eq!(kick_nick_from_mask("*!*@evil.example"), "*");
    }

    #[test]
    fn list_mask_kind_letters_match_ban_and_quiet() {
        assert_eq!(ListMaskKind::Ban.mode_letter(), 'b');
        assert_eq!(ListMaskKind::Quiet.mode_letter(), 'q');
    }
}
