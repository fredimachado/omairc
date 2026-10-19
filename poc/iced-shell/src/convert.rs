use iced::Color as IcedColor;
use shared::Color;

pub fn to_iced(c: Color) -> IcedColor {
    IcedColor::from_rgba(c.r, c.g, c.b, c.a)
}
