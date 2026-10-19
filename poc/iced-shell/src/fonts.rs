use iced::Font;

pub const IA_WRITER: Font = Font::with_name("iA Writer Mono S");

pub fn bold() -> Font {
    Font {
        family: IA_WRITER.family,
        weight: iced::font::Weight::Bold,
        ..IA_WRITER
    }
}
