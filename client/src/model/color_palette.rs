//! Named color palettes for nicks and network icons. Each id is a config token
//! under `[preferences] colorPalette=…`. Adding a palette is one table row plus
//! one [`ColorPaletteId`] variant; unknown tokens fall back to
//! [`ColorPaletteId::Classic`].

use super::color::Color;

/// How many hue families every color palette carries. The hash modulus.
pub const COLOR_PALETTE_LEN: usize = 8;

/// A configured color palette. Tokens are stable config keys.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorPaletteId {
    /// The original Tokyo-Night-derived hues. Slot 0 follows the theme accent.
    #[default]
    Classic,
    /// Brighter fixed chromas (blue, violet, teal, orange, pink, green, red, gold).
    Vivid,
}

impl ColorPaletteId {
    /// Catalog order. Append new ids here when a palette is added.
    pub const ALL: [ColorPaletteId; 2] = [ColorPaletteId::Classic, ColorPaletteId::Vivid];

    /// The stored token (`colorPalette=…` in omairc.conf).
    pub fn token(self) -> &'static str {
        self.def().token
    }

    /// The id for a stored token, ignoring case and surrounding space.
    /// `None` for anything unknown; the caller applies the ordinary default.
    pub fn from_token(token: &str) -> Option<Self> {
        let folded = token.trim().to_lowercase();
        Self::ALL.into_iter().find(|id| id.token() == folded)
    }

    /// The eight colors for this palette in the given mode. When the definition
    /// marks slot 0 as accent-backed, `accent` replaces that slot.
    pub fn colors(self, dark: bool, accent: Color) -> [Color; COLOR_PALETTE_LEN] {
        let def = self.def();
        let hexes = if dark { def.dark } else { def.light };
        let mut colors = hexes.map(|hex| Color::from_hex(hex).expect("color palette hex"));
        if def.accent_slot0 {
            colors[0] = accent;
        }
        colors
    }

    fn def(self) -> &'static ColorPaletteDef {
        COLOR_PALETTE_TABLE
            .iter()
            .find(|row| row.id == self)
            .expect("every ColorPaletteId has a table row")
    }
}

struct ColorPaletteDef {
    id: ColorPaletteId,
    token: &'static str,
    /// When true, slot 0 is the live theme accent at resolve time.
    accent_slot0: bool,
    dark: [&'static str; COLOR_PALETTE_LEN],
    light: [&'static str; COLOR_PALETTE_LEN],
}

/// Registry of color palettes. Add a row (and a [`ColorPaletteId`] variant) to
/// ship a new set; keep dark/light families parallel slot-for-slot.
const COLOR_PALETTE_TABLE: &[ColorPaletteDef] = &[
    ColorPaletteDef {
        id: ColorPaletteId::Classic,
        token: "classic",
        accent_slot0: true,
        dark: [
            "#5584aa", // overwritten by accent
            "#c099ff",
            "#7fc8a9",
            "#efb366",
            "#ed8f9d",
            "#9ece6a",
            "#7dcfff",
            "#ddd06e",
        ],
        light: [
            "#2077b2", // overwritten by accent
            "#7950b8",
            "#237a58",
            "#a45f14",
            "#b44355",
            "#4d7c0f",
            "#0f7b8f",
            "#7c6a0a",
        ],
    },
    ColorPaletteDef {
        id: ColorPaletteId::Vivid,
        token: "vivid",
        accent_slot0: false,
        dark: [
            "#2f6ff5",
            "#8a5ef8",
            "#2eb8c9",
            "#e4661c",
            "#e14d99",
            "#68c65e",
            "#e14844",
            "#e8c23a",
        ],
        light: [
            "#3474f6",
            "#895df6",
            "#2db6c7",
            "#e4681e",
            "#df4c98",
            "#67c45c",
            "#e04744",
            "#e6bf38",
        ],
    },
];

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

    #[test]
    fn classic_slot_zero_follows_accent() {
        let accent = Color::from_hex("#112233").unwrap();
        let colors = ColorPaletteId::Classic.colors(true, accent);
        assert_eq!(colors[0], accent);
        assert_eq!(colors[1].to_hex(), "#c099ff");
    }

    #[test]
    fn vivid_ignores_accent() {
        let accent = Color::from_hex("#112233").unwrap();
        let colors = ColorPaletteId::Vivid.colors(true, accent);
        assert_eq!(colors[0].to_hex(), "#2f6ff5");
    }

    #[test]
    fn every_id_has_eight_colors() {
        let accent = Color::from_hex("#5584aa").unwrap();
        for id in ColorPaletteId::ALL {
            assert_eq!(id.colors(true, accent).len(), COLOR_PALETTE_LEN);
            assert_eq!(id.colors(false, accent).len(), COLOR_PALETTE_LEN);
        }
    }
}
