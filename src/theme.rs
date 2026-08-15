use ratatui::style::Color;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Palette {
    pub background: Color,
    pub panel: Color,
    pub primary: Color,
    pub hot: Color,
    pub text: Color,
    pub muted: Color,
    pub grid: Color,
    pub warning: Color,
    pub success: Color,
    pub unknown: Color,
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            background: rgb(0x080704),
            panel: rgb(0x100d08),
            primary: rgb(0xd69b3c),
            hot: rgb(0xffd37a),
            text: rgb(0xf1e3c2),
            muted: rgb(0x8c8068),
            grid: rgb(0x2a2114),
            warning: rgb(0xf06445),
            success: rgb(0x69c76d),
            unknown: rgb(0x48b9c7),
        }
    }
}

const fn rgb(hex: u32) -> Color {
    Color::Rgb(
        ((hex >> 16) & 0xff) as u8,
        ((hex >> 8) & 0xff) as u8,
        (hex & 0xff) as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_roles_do_not_depend_on_one_color() {
        let palette = Palette::default();
        assert_ne!(palette.background, palette.text);
        assert_ne!(palette.success, palette.warning);
        assert_ne!(palette.primary, palette.unknown);
        assert_ne!(palette.panel, palette.grid);
    }
}
