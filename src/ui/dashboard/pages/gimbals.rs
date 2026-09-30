//! Page 0 (P1/4): Primary Gimbals & Trims, Switches, Pots, and Trim status.

use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    text::Text,
};

use crate::display::St7567;
use crate::input::InputState;
use crate::storage::RadioStorage;
use crate::trim::{self, TrimController};
use crate::ui::format::{ascii_as_str, format_percent, format_throttle_percent, format_trim};
use crate::ui::widgets;

#[inline(never)]
pub fn render(
    lcd: &mut St7567,
    state: &InputState,
    storage: &RadioStorage,
    trims: &TrimController,
    is_binding: bool,
    timer_str: Option<&str>,
    timer_expired: bool,
    blink_on: bool,
) {
    let text_style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
    let mut pct_buf = [0u8; 5];

    // CH1: Roll / Aileron (y = 13)
    Text::new("A", Point::new(2, 19), text_style).draw(lcd).ok();
    widgets::draw_channel_gauge(lcd, 12, 13, 76, 7, state.sticks.roll, trims.values.roll);
    let p1 = format_percent(state.sticks.roll, &mut pct_buf);
    Text::new(p1, Point::new(92, 19), text_style).draw(lcd).ok();

    // CH2: Pitch / Elevator (y = 21)
    Text::new("E", Point::new(2, 27), text_style).draw(lcd).ok();
    widgets::draw_channel_gauge(lcd, 12, 21, 76, 7, state.sticks.pitch, trims.values.pitch);
    let p2 = format_percent(state.sticks.pitch, &mut pct_buf);
    Text::new(p2, Point::new(92, 27), text_style).draw(lcd).ok();

    // CH3: Throttle (y = 29)
    Text::new("T", Point::new(2, 35), text_style).draw(lcd).ok();
    let thr_trim = if storage.radio.throttle_trim != 0 { trims.values.throttle } else { 0 };
    widgets::draw_progress_bar(lcd, 12, 29, 76, 7, state.sticks.throttle, thr_trim);
    let p3 = format_throttle_percent(state.sticks.throttle, &mut pct_buf);
    Text::new(p3, Point::new(92, 35), text_style).draw(lcd).ok();

    // CH4: Yaw / Rudder (y = 37)
    Text::new("R", Point::new(2, 43), text_style).draw(lcd).ok();
    widgets::draw_channel_gauge(lcd, 12, 37, 76, 7, state.sticks.yaw, trims.values.yaw);
    let p4 = format_percent(state.sticks.yaw, &mut pct_buf);
    Text::new(p4, Point::new(92, 43), text_style).draw(lcd).ok();

    // Switches Line (y = 47..53)
    let mut sw_buf = *b"A:U B:U C:U D:U";
    sw_buf[2] = state.switches.sa.as_char() as u8;
    sw_buf[6] = state.switches.sb.as_char() as u8;
    sw_buf[10] = state.switches.sc.as_char() as u8;
    sw_buf[14] = state.switches.sd.as_char() as u8;
    let sw_str = ascii_as_str(&sw_buf);
    Text::new(sw_str, Point::new(2, 53), text_style).draw(lcd).ok();

    // Pots: V1 / V2 on right (scaled 0..9 across full turn)
    let mut pot_buf = *b"V:0/0";
    let p1_val = (((state.pots.vr1 as i32 + 1000) * 9) / 2000).clamp(0, 9) as u8;
    let p2_val = (((state.pots.vr2 as i32 + 1000) * 9) / 2000).clamp(0, 9) as u8;
    pot_buf[2] = b'0' + p1_val;
    pot_buf[4] = b'0' + p2_val;
    let pot_str = ascii_as_str(&pot_buf);
    Text::new(pot_str, Point::new(96, 53), text_style).draw(lcd).ok();

    // Standardized Footer (y = 55..63)
    if is_binding {
        widgets::draw_footer(lcd, "[ESC] Finish Bind");
    } else if trims.last_active != trim::ActiveTrim::None {
        let mut trm_buf = [0u8; 9];
        let val = match trims.last_active {
            trim::ActiveTrim::Roll => trims.values.roll,
            trim::ActiveTrim::Pitch => trims.values.pitch,
            trim::ActiveTrim::Throttle => trims.values.throttle,
            trim::ActiveTrim::Yaw => trims.values.yaw,
            trim::ActiveTrim::None => 0,
        };
        let trm_str = format_trim(trims.last_active, val, &mut trm_buf);
        widgets::draw_footer_split(lcd, trm_str, "Hold OK:Menu");
    } else if let Some(t_str) = timer_str {
        let invert_timer = timer_expired && blink_on;
        widgets::draw_footer_three(lcd, "P1/4", t_str, "Hold OK:Menu", invert_timer);
    } else {
        widgets::draw_footer_split(lcd, "P1/4", "Hold OK:Menu");
    }
}
