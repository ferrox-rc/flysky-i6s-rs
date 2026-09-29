//! Page 1 (P2/4): 18-Channel Dual-Column Live Monitor.

use embedded_graphics::{
    mono_font::{ascii::FONT_4X6, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{Line, PrimitiveStyle, Rectangle},
    text::Text,
};

use crate::display::St7567;
use crate::mixer::{CHANNEL_MAX_US, CHANNEL_MIN_US, CHANNEL_SPAN_US, NUM_CHANNELS};
use crate::ui::format::{ascii_as_str, u16_to_dec_4};
use crate::ui::widgets;

pub fn render(lcd: &mut St7567, rf_chs: &[u16; NUM_CHANNELS], is_binding: bool) {
    let text_style_small = MonoTextStyle::new(&FONT_4X6, BinaryColor::On);
    let sep_style = PrimitiveStyle::with_stroke(BinaryColor::On, 1);

    // Col 0 (CH 1..9) at x = 2..62, Col 1 (CH 10..18) at x = 66..126
    for col in 0..2 {
        let col_x = if col == 0 { 2 } else { 66 };
        let start_ch = col * 9;

        for row in 0..9 {
            let ch = start_ch + row;
            let y = 10 + ((row as i32 * 44) / 9);

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

            // Bar gauge (width 22, height 4) using shared widget
            let us = rf_chs[ch].clamp(CHANNEL_MIN_US, CHANNEL_MAX_US);
            let fill_w = (((us - CHANNEL_MIN_US) as u32 * 20) / CHANNEL_SPAN_US as u32).min(20);
            widgets::draw_bar_gauge(
                lcd,
                Rectangle::new(Point::new(col_x + 13, y + 1), Size::new(22, 4)),
                fill_w,
            );

            // Value: "1500"
            let mut val_buf = [0u8; 4];
            u16_to_dec_4(us, &mut val_buf);
            let val_str = ascii_as_str(&val_buf);
            Text::new(val_str, Point::new(col_x + 37, y + 5), text_style_small)
                .draw(lcd)
                .ok();
        }
    }

    // Vertical divider line between columns
    Line::new(Point::new(64, 11), Point::new(64, 54))
        .into_styled(sep_style)
        .draw(lcd)
        .ok();

    // Standardized Footer (y = 55..63)
    if is_binding {
        widgets::draw_footer(lcd, "[ESC] Finish Bind");
    } else {
        widgets::draw_footer_split(lcd, "P2/4", "18-CH MONITOR");
    }
}
