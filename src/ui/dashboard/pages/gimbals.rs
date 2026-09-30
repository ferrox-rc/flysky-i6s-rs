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
use crate::ui::glyphs::draw_switch_arrow;
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
    let is_general = storage.active_model().model_type == 4;

    // CH1: Roll / 1 (y = 13)
    let lbl1 = if is_general { "1" } else { "A" };
    Text::new(lbl1, Point::new(2, 19), text_style).draw(lcd).ok();
    widgets::draw_channel_gauge(lcd, 12, 13, 76, 7, state.sticks.roll, trims.values.roll);
    let p1 = format_percent(state.sticks.roll, &mut pct_buf);
    Text::new(p1, Point::new(92, 19), text_style).draw(lcd).ok();

    // CH2: Pitch / 2 (y = 21)
    let lbl2 = if is_general { "2" } else { "E" };
    Text::new(lbl2, Point::new(2, 27), text_style).draw(lcd).ok();
    widgets::draw_channel_gauge(lcd, 12, 21, 76, 7, state.sticks.pitch, trims.values.pitch);
    let p2 = format_percent(state.sticks.pitch, &mut pct_buf);
    Text::new(p2, Point::new(92, 27), text_style).draw(lcd).ok();

    // CH3: Throttle / 3 (y = 29)
    let lbl3 = if is_general { "3" } else { "T" };
    Text::new(lbl3, Point::new(2, 35), text_style).draw(lcd).ok();
    if is_general {
        widgets::draw_channel_gauge(lcd, 12, 29, 76, 7, state.sticks.throttle, trims.values.throttle);
        let p3 = format_percent(state.sticks.throttle, &mut pct_buf);
        Text::new(p3, Point::new(92, 35), text_style).draw(lcd).ok();
    } else {
        let thr_trim = if storage.radio.throttle_trim != 0 { trims.values.throttle } else { 0 };
        widgets::draw_progress_bar(lcd, 12, 29, 76, 7, state.sticks.throttle, thr_trim);
        let p3 = format_throttle_percent(state.sticks.throttle, &mut pct_buf);
        Text::new(p3, Point::new(92, 35), text_style).draw(lcd).ok();
    }

    // CH4: Yaw / 4 (y = 37)
    let lbl4 = if is_general { "4" } else { "R" };
    Text::new(lbl4, Point::new(2, 43), text_style).draw(lcd).ok();
    widgets::draw_channel_gauge(lcd, 12, 37, 76, 7, state.sticks.yaw, trims.values.yaw);
    let p4 = format_percent(state.sticks.yaw, &mut pct_buf);
    Text::new(p4, Point::new(92, 43), text_style).draw(lcd).ok();

    // Switches Line with graphic arrows (y = 46..53)
    Text::new("A", Point::new(2, 53), text_style).draw(lcd).ok();
    draw_switch_arrow(lcd, 9, 46, state.switches.sa);

    Text::new("B", Point::new(20, 53), text_style).draw(lcd).ok();
    draw_switch_arrow(lcd, 27, 46, state.switches.sb);

    Text::new("C", Point::new(38, 53), text_style).draw(lcd).ok();
    draw_switch_arrow(lcd, 45, 46, state.switches.sc);

    Text::new("D", Point::new(56, 53), text_style).draw(lcd).ok();
    draw_switch_arrow(lcd, 63, 46, state.switches.sd);

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
        widgets::draw_footer_three(lcd, "P1/5", t_str, "Hold OK:Menu", invert_timer);
    } else {
        widgets::draw_footer_split(lcd, "P1/5", "Hold OK:Menu");
    }
}
