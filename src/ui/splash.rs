//! Startup splash screen renderer with Ferrox-RC logo and firmware information.

use embedded_graphics::{
    mono_font::{ascii::FONT_4X6, ascii::FONT_6X10, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    text::Text,
};

use crate::display::St7567;
use crate::ui::glyphs::draw_ferrox_logo;

/// Render the Ferrox-RC splash screen onto the display buffer.
/// - Top: 28x27 Ferrox-RC kinetic delta emblem centered at x = 50, y = 3
/// - Brand Header: "FERROX-RC" in FONT_6X10 centered at x = 37, y = 41
/// - Firmware ID: "flysky-i6x-rs" in FONT_4X6 centered at x = 38, y = 50
/// - Version / Build: "v0.18.1" in FONT_4X6 centered at x = 50, y = 59
pub fn draw_splash(lcd: &mut St7567) {
    lcd.clear_buffer();

    // 1. Draw 28x27 Ferrox-RC Logo Emblem centered horizontally
    draw_ferrox_logo(lcd, 50, 3);

    // 2. Draw "FERROX-RC" in FONT_6X10 (54px wide, centered at x = 37, baseline y = 41)
    let title_style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
    Text::new("FERROX-RC", Point::new(37, 41), title_style).draw(lcd).ok();

    // 3. Draw "flysky-i6x-rs" in FONT_4X6 (52px wide, centered at x = 38, baseline y = 50)
    let sub_style = MonoTextStyle::new(&FONT_4X6, BinaryColor::On);
    Text::new("flysky-i6x-rs", Point::new(38, 50), sub_style).draw(lcd).ok();

    // 4. Draw Version Tag "v0.18.1" in FONT_4X6 (28px wide, centered at x = 50, baseline y = 59)
    Text::new("v0.18.1", Point::new(50, 59), sub_style).draw(lcd).ok();
}
