use super::color::Color;
use super::color_palette::{ColorPaletteId, COLOR_PALETTE_LEN};
use super::theme::Theme;

#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub theme: Theme,
    pub text_scale: f32,
    color_palette: ColorPaletteId,
    /// Resolved nick/network hues for `color_palette` under the current theme.
    resolved_palette: [Color; COLOR_PALETTE_LEN],
}

impl Style {
    pub fn new(theme: Theme, text_scale: f32) -> Self {
        Self::with_color_palette(theme, text_scale, ColorPaletteId::default())
    }

    pub fn with_color_palette(
        theme: Theme,
        text_scale: f32,
        color_palette: ColorPaletteId,
    ) -> Self {
        let resolved_palette = color_palette.colors(theme.dark_mode, theme.accent);
        Self {
            theme,
            text_scale: text_scale.max(0.01),
            color_palette,
            resolved_palette,
        }
    }

    pub fn color_palette(&self) -> ColorPaletteId {
        self.color_palette
    }

    pub fn dark_mode(&self) -> bool {
        self.theme.dark_mode
    }

    pub fn page_color(&self) -> Color {
        self.theme.background
    }

    pub fn ink_color(&self) -> Color {
        self.theme.foreground
    }

    pub fn accent_color(&self) -> Color {
        self.theme.accent
    }

    pub fn selection_color(&self) -> Color {
        self.theme.selection
    }

    pub fn panel_color(&self) -> Color {
        mix_colors(
            self.page_color(),
            self.ink_color(),
            if self.dark_mode() { 0.035 } else { 0.025 },
        )
    }

    pub fn raised_color(&self) -> Color {
        mix_colors(
            self.page_color(),
            self.ink_color(),
            if self.dark_mode() { 0.075 } else { 0.055 },
        )
    }

    pub fn hover_color(&self) -> Color {
        mix_colors(
            self.page_color(),
            self.ink_color(),
            if self.dark_mode() { 0.10 } else { 0.075 },
        )
    }

    pub fn divider_color(&self) -> Color {
        mix_colors(
            self.page_color(),
            self.ink_color(),
            if self.dark_mode() { 0.13 } else { 0.11 },
        )
    }

    pub fn muted_color(&self) -> Color {
        mix_colors(
            self.page_color(),
            self.ink_color(),
            if self.dark_mode() { 0.52 } else { 0.47 },
        )
    }

    pub fn overlay_veil_color(&self) -> Color {
        let veil = mix_colors(
            self.page_color(),
            self.ink_color(),
            if self.dark_mode() { 0.18 } else { 0.12 },
        );
        veil.with_alpha(0.5)
    }

    /// The dim behind a modal popup (Jump, Link, Nick, Inbox, Shortcuts).
    /// Qt's Material style draws `Material.backgroundDimColor`, read from Qt
    /// 6.11.2 with `qml.exe`: `#303030` at alpha 0.6 for the light theme and
    /// `#fafafa` at alpha 0.6 for the dark one. The Qt window picks the theme
    /// from `darkMode` (`client/src/ui/shell`), so this follows `dark_mode`.
    pub fn popup_dim_color(&self) -> Color {
        let base = if self.dark_mode() {
            Color::from_rgb8(250, 250, 250)
        } else {
            Color::from_rgb8(48, 48, 48)
        };
        base.with_alpha(0.6)
    }

    /// The eight hues for nicks and network icons under the configured palette.
    pub fn palette_colors(&self) -> [Color; COLOR_PALETTE_LEN] {
        self.resolved_palette
    }

    pub fn nick_avatar_fills(&self) -> [Color; COLOR_PALETTE_LEN] {
        let mix = if self.dark_mode() { 0.23 } else { 0.16 };
        self.palette_colors()
            .map(|c| mix_colors(self.page_color(), c, mix))
    }

    pub fn nick_palette_index(&self, nick: &str) -> usize {
        nick_palette_index(nick)
    }

    pub fn nick_color(&self, nick: &str) -> Color {
        self.palette_colors()[self.nick_palette_index(nick)]
    }

    pub fn initials(&self, nick: &str) -> String {
        nick.chars()
            .next()
            .map(|c| c.to_uppercase().collect::<String>())
            .unwrap_or_else(|| "?".to_string())
    }

    pub fn presence_color(&self, kind: &str) -> Color {
        match kind {
            "away" => Color::from_hex("#d6a552").unwrap(),
            "online" => Color::from_hex("#69b978").unwrap(),
            _ => self.muted_color(),
        }
    }

    pub fn scaled(&self, px: f32) -> i32 {
        (px * self.text_scale).round().max(1.0) as i32
    }
}

/// FNV-1a (32-bit) over the nick's UTF-8 bytes, then fold high bits before
/// modulo so the slot is not only the low 3 bits of each input byte.
/// Shared with `nickPaletteIndex` in `tui/internal/ui/style.go`.
fn nick_palette_index(nick: &str) -> usize {
    const OFFSET: u32 = 2166136261;
    const PRIME: u32 = 16777619;
    let mut hash = OFFSET;
    for byte in nick.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    ((hash ^ (hash >> 16)) as usize) % COLOR_PALETTE_LEN
}

pub fn mix_colors(base: Color, tint: Color, amount: f32) -> Color {
    base.mix(tint, amount)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::theme::Theme;

    #[test]
    fn dark_panel_mix_locked() {
        let style = Style::new(Theme::defaults_dark(), 1.0);
        assert_eq!(style.panel_color().to_hex(), "#181818");
        assert_eq!(style.raised_color().to_hex(), "#212121");
        assert_eq!(style.hover_color().to_hex(), "#262626");
        assert_eq!(style.divider_color().to_hex(), "#2d2d2d");
        assert_eq!(style.muted_color().to_hex(), "#838383");
        let veil = style.overlay_veil_color();
        assert_eq!(veil.to_rgb8(), (56, 56, 56));
        assert!((veil.a - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn light_derived_colors() {
        let mut theme = Theme::defaults_dark();
        theme.dark_mode = false;
        theme.background = Color::from_hex("#ffffff").unwrap();
        theme.foreground = Color::from_hex("#222324").unwrap();
        let style = Style::new(theme, 1.0);
        assert_eq!(style.panel_color().to_hex(), "#f9fafa");
        assert_eq!(style.raised_color().to_hex(), "#f3f3f3");
        assert_eq!(style.hover_color().to_hex(), "#eeefef");
        assert_eq!(style.divider_color().to_hex(), "#e7e7e7");
        assert_eq!(style.muted_color().to_hex(), "#979898");
        let veil = style.overlay_veil_color();
        assert_eq!(veil.to_rgb8(), (228, 229, 229));
        assert!((veil.a - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn popup_dim_matches_qt_material_background_dim() {
        // Values read from Qt 6.11.2's Material style (see popup_dim_color).
        let dark = Style::new(Theme::defaults_dark(), 1.0);
        assert_eq!(dark.popup_dim_color().to_rgb8(), (250, 250, 250));
        assert!((dark.popup_dim_color().a - 0.6).abs() < 1e-6);

        let mut theme = Theme::defaults_dark();
        theme.dark_mode = false;
        let light = Style::new(theme, 1.0);
        assert_eq!(light.popup_dim_color().to_rgb8(), (48, 48, 48));
        assert!((light.popup_dim_color().a - 0.6).abs() < 1e-6);
    }

    #[test]
    fn nick_hash_and_initials() {
        let style = Style::new(Theme::defaults_dark(), 1.0);
        assert_eq!(style.initials("fred"), "F");
        assert_eq!(style.initials(""), "?");
        assert_eq!(style.nick_palette_index("mira"), 2);
        assert_ne!(
            style.nick_palette_index("abc"),
            style.nick_palette_index("cab"),
            "anagrams must not share a palette slot"
        );
        assert_ne!(
            style.nick_palette_index("mira"),
            style.nick_palette_index("mara"),
            "letters eight apart must be able to change the slot"
        );
    }

    #[test]
    fn classic_is_default_and_vivid_differs() {
        let theme = Theme::defaults_dark();
        let classic = Style::new(theme.clone(), 1.0);
        let vivid = Style::with_color_palette(theme, 1.0, ColorPaletteId::Vivid);
        assert_eq!(classic.color_palette(), ColorPaletteId::Classic);
        assert_ne!(
            classic.palette_colors()[1].to_hex(),
            vivid.palette_colors()[1].to_hex()
        );
        assert_eq!(classic.palette_colors()[0], classic.accent_color());
        assert_eq!(vivid.palette_colors()[0].to_hex(), "#2f6ff5");
    }

    #[test]
    fn scaled_size() {
        let style = Style::new(Theme::defaults_dark(), 1.4);
        assert_eq!(style.scaled(10.0), 14);
        assert_eq!(style.scaled(0.2), 1);
    }
}
