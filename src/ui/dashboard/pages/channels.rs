//! Page 1 (P2/5 & P3/5): 18-Channel Split Dual-Column Live Monitor.

use embedded_graphics::{
    mono_font::{ascii::FONT_4X6, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::Rectangle,
    text::Text,
};

use crate::display::St7567;
use crate::mixer::{CHANNEL_MAX_US, CHANNEL_MIN_US, CHANNEL_SPAN_US, NUM_CHANNELS};
use crate::ui::format::{ascii_as_str, u16_to_dec_4};
use crate::ui::widgets;

#[inline(never)]
pub fn render(lcd: &mut St7567, rf_chs: &[u16; NUM_CHANNELS], is_binding: bool, page_part: usize) {
    let text_style_small = MonoTextStyle::new(&FONT_4X6, BinaryColor::On);

    // page_part 0: CH 1..9 (Col 0: 1..5, Col 1: 6..9)
    // page_part 1: CH 10..18 (Col 0: 10..14, Col 1: 15..18)
    let (base_ch, footer_page, footer_title) = if page_part == 0 {
        (0, "P2/5", "CH 1-9 MONITOR")
    } else {
        (9, "P3/5", "CH 10-18 MONITOR")
    };

    for col in 0..2 {
        let col_x = if col == 0 { 2 } else { 66 };
        let col_start = base_ch + if col == 0 { 0 } else { 5 };
        let num_rows = if col == 0 { 5 } else { 4 };

        for row in 0..num_rows {
            let ch = col_start + row;
            if ch >= NUM_CHANNELS {
                break;
            }
            let y = 13 + (row as i32 * 8);

            // Label: " 1:" .. "18:"
            let mut lbl_buf = *b"  :";
            let ch_num = ch + 1;
            if ch_num >= 10 {
                lbl_buf[0] = b'0' + (ch_num / 10) as u8;
                lbl_buf[1] = b'0' + (ch_num % 10) as u8;
            } else {
                lbl_buf[0] = b' ';
                lbl_buf[1] = b'0' + ch_num as u8;
            }
            let lbl_str = ascii_as_str(&lbl_buf);
            Text::new(lbl_str, Point::new(col_x, y + 5), text_style_small)
                .draw(lcd)
                .ok();

            // Bar gauge (width 26, height 5) using shared widget
            let us = rf_chs[ch].clamp(CHANNEL_MIN_US, CHANNEL_MAX_US);
            let fill_w = (((us - CHANNEL_MIN_US) as u32 * 24) / CHANNEL_SPAN_US as u32).min(24);
            widgets::draw_bar_gauge(
                lcd,
                Rectangle::new(Point::new(col_x + 14, y + 1), Size::new(26, 5)),
                fill_w,
            );

            // Value: "1500"
            let mut val_buf = [0u8; 4];
            u16_to_dec_4(us, &mut val_buf);
            let val_str = ascii_as_str(&val_buf);
            Text::new(val_str, Point::new(col_x + 42, y + 5), text_style_small)
                .draw(lcd)
                .ok();
        }
    }

    // Vertical divider line between columns
    lcd.draw_vline(64, 12, 41, true);

    // Standardized Footer (y = 55..63)
    if is_binding {
        widgets::draw_footer(lcd, "[ESC] Finish Bind");
    } else {
        widgets::draw_footer_split(lcd, footer_page, footer_title);
    }
}
