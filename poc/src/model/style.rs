use super::color::Color;
use super::theme::Theme;

#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub theme: Theme,
    pub text_scale: f32,
}

impl Style {
    pub fn new(theme: Theme, text_scale: f32) -> Self {
        Self {
            theme,
            text_scale: text_scale.max(0.01),
        }
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

    pub fn nick_palette(&self) -> [Color; 8] {
        let accent = self.accent_color();
        if self.dark_mode() {
            [
                accent,
                Color::from_hex("#c099ff").unwrap(),
                Color::from_hex("#7fc8a9").unwrap(),
                Color::from_hex("#efb366").unwrap(),
                Color::from_hex("#ed8f9d").unwrap(),
                Color::from_hex("#9ece6a").unwrap(),
                Color::from_hex("#7dcfff").unwrap(),
                Color::from_hex("#ddd06e").unwrap(),
            ]
        } else {
            [
                accent,
                Color::from_hex("#7950b8").unwrap(),
                Color::from_hex("#237a58").unwrap(),
                Color::from_hex("#a45f14").unwrap(),
                Color::from_hex("#b44355").unwrap(),
                Color::from_hex("#4d7c0f").unwrap(),
                Color::from_hex("#0f7b8f").unwrap(),
                Color::from_hex("#7c6a0a").unwrap(),
            ]
        }
    }

    pub fn nick_avatar_fills(&self) -> [Color; 8] {
        let mix = if self.dark_mode() { 0.23 } else { 0.16 };
        self.nick_palette()
            .map(|c| mix_colors(self.page_color(), c, mix))
    }

    pub fn nick_palette_index(&self, nick: &str) -> usize {
        let mut hash = 0usize;
        for ch in nick.chars() {
            hash = (hash + ch as usize) % 8;
        }
        hash
    }

    pub fn nick_color(&self, nick: &str) -> Color {
        self.nick_palette()[self.nick_palette_index(nick)]
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
    fn nick_hash_and_initials() {
        let style = Style::new(Theme::defaults_dark(), 1.0);
        assert_eq!(style.initials("fred"), "F");
        assert_eq!(style.initials(""), "?");
        assert_eq!(style.nick_palette_index("aa"), 2);
    }

    #[test]
    fn scaled_size() {
        let style = Style::new(Theme::defaults_dark(), 1.4);
        assert_eq!(style.scaled(10.0), 14);
        assert_eq!(style.scaled(0.2), 1);
    }
}
