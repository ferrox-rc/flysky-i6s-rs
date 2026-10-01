//! Standalone 1-bit bitmap glyphs and direct rasterizers for UI elements.
//!
//! Stores 12x12 MDI icons and switch indicator arrows as compact 18-byte bit-arrays,
//! eliminating heavy external crate dependencies and saving FLASH.

use crate::display::St7567;
use crate::input::SwitchPos;

pub const GLYPH_AIRPLANE: [u8; 18] = [
    0, 0, 0, 0, 194, 59, 240, 1, 30, 224, 193, 27, 24, 1, 17, 0, 0, 0,
];

pub const GLYPH_AIRPLANE_COG: [u8; 18] = [
    0, 0, 16, 142, 193, 15, 240, 0, 15, 216, 192, 32, 136, 15, 80, 128, 15, 32,
];

pub const GLYPH_CHART_LINE: [u8; 18] = [
    0, 32, 0, 2, 36, 96, 50, 34, 23, 154, 161, 0, 6, 224, 127, 254, 7, 0,
];

pub const GLYPH_CHART_BELL_CURVE: [u8; 18] = [
    0, 32, 0, 226, 32, 10, 18, 33, 17, 18, 161, 32, 14, 38, 0, 254, 7, 0,
];

pub const GLYPH_SWAP_HORIZONTAL: [u8; 18] = [
    0, 0, 0, 0, 0, 48, 224, 3, 48, 8, 192, 7, 8, 0, 0, 0, 0, 0,
];

pub const GLYPH_TOGGLE_SWITCH: [u8; 18] = [
    0, 0, 0, 0, 128, 31, 252, 227, 71, 126, 196, 63, 248, 1, 0, 0, 0, 0,
];

pub const GLYPH_SWAP_VERTICAL: [u8; 18] = [
    0, 0, 0, 48, 128, 3, 16, 0, 9, 144, 0, 8, 192, 1, 24, 0, 0, 0,
];

pub const GLYPH_COG: [u8; 18] = [
    0, 0, 6, 240, 192, 63, 158, 199, 48, 12, 227, 121, 252, 3, 15, 96, 0, 0,
];

pub const GLYPH_RADIO_TOWER: [u8; 18] = [
    0, 0, 15, 12, 67, 47, 154, 165, 86, 106, 37, 70, 244, 66, 41, 144, 0, 0,
];

pub const GLYPH_GAUGE: [u8; 18] = [
    0, 0, 15, 12, 67, 44, 66, 36, 70, 98, 36, 64, 244, 194, 63, 240, 0, 0,
];

pub const GLYPH_CROSSHAIRS_GPS: [u8; 18] = [
    0, 0, 6, 248, 193, 48, 100, 98, 111, 246, 70, 38, 12, 131, 31, 96, 0, 0,
];

pub const GLYPH_PULSE: [u8; 18] = [
    0, 0, 0, 0, 0, 1, 16, 128, 11, 252, 67, 34, 32, 0, 0, 0, 0, 0,
];

pub const GLYPH_INFORMATION_OUTLINE: [u8; 18] = [
    0, 0, 15, 12, 67, 32, 2, 36, 64, 98, 36, 70, 4, 194, 48, 240, 0, 0,
];

pub const MENU_GLYPHS: [&[u8; 18]; 13] = [
    &GLYPH_AIRPLANE,
    &GLYPH_AIRPLANE_COG,
    &GLYPH_CHART_LINE,
    &GLYPH_CHART_BELL_CURVE,
    &GLYPH_SWAP_HORIZONTAL,
    &GLYPH_TOGGLE_SWITCH,
    &GLYPH_SWAP_VERTICAL,
    &GLYPH_COG,
    &GLYPH_RADIO_TOWER,
    &GLYPH_GAUGE,
    &GLYPH_CROSSHAIRS_GPS,
    &GLYPH_PULSE,
    &GLYPH_INFORMATION_OUTLINE,
];

/// Draw a 12x12 1-bit bitmap at (x, y) directly onto the St7567 display.
#[inline]
pub fn draw_glyph_12x12(lcd: &mut St7567, x: i32, y: i32, data: &[u8; 18], on: bool) {
    for row in 0..12 {
        let py = y + row as i32;
        let base_bit = row * 12;
        for col in 0..12 {
            let px = x + col as i32;
            let bit = base_bit + col;
            let byte_idx = bit / 8;
            let bit_idx = bit % 8;
            if (data[byte_idx] & (1 << bit_idx)) != 0 {
                lcd.set_pixel(px, py, on);
            }
        }
    }
}

/// Draw a 5x7 switch position arrow at (x, y).
/// Up: ▲ (pointed upward)
/// Mid: — (centered horizontal bar)
/// Down: ▼ (pointed downward)
#[inline]
pub fn draw_switch_arrow(lcd: &mut St7567, x: i32, y: i32, pos: SwitchPos) {
    match pos {
        SwitchPos::Up => {
            // Triangle peak at y, base at y+2, stem down to y+6
            lcd.set_pixel(x + 2, y, true);
            lcd.draw_hline(x + 1, y + 1, 3, true);
            lcd.draw_hline(x, y + 2, 5, true);
            lcd.draw_vline(x + 2, y + 3, 4, true);
        }
        SwitchPos::Mid => {
            // Centered double bar at y+2..y+3
            lcd.draw_hline(x, y + 3, 5, true);
        }
        SwitchPos::Down => {
            // Stem at y..y+3, base at y+4, peak at y+6
            lcd.draw_vline(x + 2, y, 4, true);
            lcd.draw_hline(x, y + 4, 5, true);
            lcd.draw_hline(x + 1, y + 5, 3, true);
            lcd.set_pixel(x + 2, y + 6, true);
        }
    }
}

/// Draw a horizontal battery gauge with outline and solid right-to-left fill.
/// - Dimensions: 11x7 px (terminal tip at x, body at x+1..x+10, y..y+6).
/// - Inner cavity is 8x5 px (x+2..x+9, y+1..y+5).
/// - `bars`: 0 (empty outline), 1 (3px fill), 2 (6px fill), or 3 (8px full solid fill).
#[inline]
pub fn draw_battery_gauge(lcd: &mut St7567, x: i32, y: i32, bars: u8) {
    // Battery terminal tip on left (x, y + 2, h = 3)
    lcd.draw_vline(x, y + 2, 3, true);

    // Battery body outline (x + 1, y, w = 10, h = 7)
    lcd.draw_rect(x + 1, y, 10, 7, true);

    // Solid charge fill from base (right) to tip (left) without internal gap lines
    let fill_w = match bars {
        1 => 3,
        2 => 6,
        3 => 8,
        _ => 0,
    };
    if fill_w > 0 {
        lcd.fill_rect(x + 10 - fill_w, y + 1, fill_w as u32, 5, true);
    }
}

/// 28x27 Ferrox-RC Delta & Crystal Emblem Bitmap (95 bytes).
pub const LOGO_FERROX_28X27: [u8; 95] = [
    0, 96, 0, 0, 0, 6, 0, 0, 240, 0, 0, 0,
    9, 0, 0, 152, 1, 0, 192, 48, 0, 0, 12, 3,
    0, 64, 32, 0, 0, 6, 6, 0, 32, 64, 0, 0,
    99, 12, 0, 144, 144, 0, 128, 9, 25, 0, 8, 6,
    1, 64, 96, 32, 0, 134, 16, 6, 32, 0, 64, 0,
    1, 0, 8, 48, 0, 192, 128, 1, 0, 24, 8, 0,
    0, 129, 6, 0, 22, 60, 0, 192, 227, 3, 0, 124,
    30, 240, 128, 247, 1, 6, 248, 7, 96, 0, 14,
];

/// Draw the 28x27 Ferrox-RC emblem at (x, y) directly onto the LCD.
#[inline]
pub fn draw_ferrox_logo(lcd: &mut St7567, x: i32, y: i32) {
    for row in 0..27 {
        let py = y + row as i32;
        let base_bit = row * 28;
        for col in 0..28 {
            let px = x + col as i32;
            let bit = base_bit + col;
            let byte_idx = bit / 8;
            let bit_idx = bit % 8;
            if (LOGO_FERROX_28X27[byte_idx] & (1 << bit_idx)) != 0 {
                lcd.set_pixel(px, py, true);
            }
        }
    }
}
