//! Named color palettes for nicks and network icons (`tui/internal/irc/colorpalette.go`).
//! Tokens are the `/pref palette` values and the `colorPalette` config key. Hex tables live
//! in the presentation layer (`model/color_palette.rs`); this module owns only the id.

use serde::{Deserialize, Serialize};

/// A configured color palette. Tokens are stable config and `/pref` values.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorPaletteId {
    /// The original Tokyo-Night-derived hues. Slot 0 follows the theme accent.
    #[default]
    Classic,
    /// Brighter fixed chromas (blue, violet, teal, orange, pink, green, red, gold).
    Vivid,
}

impl ColorPaletteId {
    /// Catalog order, which the slash popup offers after `/pref palette `.
    pub const ALL: [ColorPaletteId; 2] = [ColorPaletteId::Classic, ColorPaletteId::Vivid];

    /// The stored and typed token (`colorPalette=…` in omairc.conf).
    pub fn token(self) -> &'static str {
        match self {
            ColorPaletteId::Classic => "classic",
            ColorPaletteId::Vivid => "vivid",
        }
    }

    /// The id for a stored or typed word, ignoring case and surrounding space.
    /// `None` for anything else; the caller applies the ordinary default.
    pub fn from_token(token: &str) -> Option<Self> {
        let folded = token.trim().to_lowercase();
        Self::ALL.into_iter().find(|id| id.token() == folded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_round_trip_and_unknown_is_none() {
        for id in ColorPaletteId::ALL {
            assert_eq!(ColorPaletteId::from_token(id.token()), Some(id));
            assert_eq!(
                ColorPaletteId::from_token(&id.token().to_uppercase()),
                Some(id)
            );
        }
        assert_eq!(ColorPaletteId::from_token("loud"), None);
        assert_eq!(ColorPaletteId::from_token(""), None);
        assert_eq!(ColorPaletteId::default(), ColorPaletteId::Classic);
    }
}
