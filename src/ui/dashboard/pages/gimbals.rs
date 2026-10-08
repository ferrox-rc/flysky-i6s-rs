//! Page 0 (P1/4): Primary Gimbals & Trims, Switches, Pots, and Trim status.


use crate::display::St7567;
use crate::input::InputState;
use crate::storage::RadioStorage;
use crate::trim::{self, TrimController};
use crate::ui::format::{format_percent, format_throttle_percent, format_trim};
use crate::ui::glyphs::draw_switch_arrow;
use crate::ui::widgets;

#[inline(never)]
#[allow(clippy::too_many_arguments)]
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
    let mut pct_buf = [0u8; 5];
    let is_general = storage.active_model().model_type == 4;

    // CH1: Roll / 1 (y = 13, text baseline was 19 -> top-left y = 11)
    let lbl1 = if is_general { "1" } else { "A" };
    lcd.draw_str_6x10(2, 11, lbl1, false);
    widgets::draw_channel_gauge(lcd, 12, 13, 76, 7, state.sticks.roll, trims.values.roll);
    let p1 = format_percent(state.sticks.roll, &mut pct_buf);
    lcd.draw_str_6x10(92, 11, p1, false);

    // CH2: Pitch / 2 (y = 21, text baseline was 27 -> top-left y = 19)
    let lbl2 = if is_general { "2" } else { "E" };
    lcd.draw_str_6x10(2, 19, lbl2, false);
    widgets::draw_channel_gauge(lcd, 12, 21, 76, 7, state.sticks.pitch, trims.values.pitch);
    let p2 = format_percent(state.sticks.pitch, &mut pct_buf);
    lcd.draw_str_6x10(92, 19, p2, false);

    // CH3: Throttle / 3 (y = 29, text baseline was 35 -> top-left y = 27)
    let lbl3 = if is_general { "3" } else { "T" };
    lcd.draw_str_6x10(2, 27, lbl3, false);
    if is_general {
        widgets::draw_channel_gauge(lcd, 12, 29, 76, 7, state.sticks.throttle, trims.values.throttle);
        let p3 = format_percent(state.sticks.throttle, &mut pct_buf);
        lcd.draw_str_6x10(92, 27, p3, false);
    } else {
        let thr_trim = if storage.radio.throttle_trim != 0 { trims.values.throttle } else { 0 };
        widgets::draw_progress_bar(lcd, 12, 29, 76, 7, state.sticks.throttle, thr_trim);
        let p3 = format_throttle_percent(state.sticks.throttle, &mut pct_buf);
        lcd.draw_str_6x10(92, 27, p3, false);
    }

    // CH4: Yaw / 4 (y = 37, text baseline was 43 -> top-left y = 35)
    let lbl4 = if is_general { "4" } else { "R" };
    lcd.draw_str_6x10(2, 35, lbl4, false);
    widgets::draw_channel_gauge(lcd, 12, 37, 76, 7, state.sticks.yaw, trims.values.yaw);
    let p4 = format_percent(state.sticks.yaw, &mut pct_buf);
    lcd.draw_str_6x10(92, 35, p4, false);

    // Switches Line with graphic arrows (y = 46..53, text baseline was 53 -> top-left y = 45)
    lcd.draw_str_6x10(2, 45, "A", false);
    draw_switch_arrow(lcd, 9, 46, state.switches.sa);

    lcd.draw_str_6x10(20, 45, "B", false);
    draw_switch_arrow(lcd, 27, 46, state.switches.sb);

    lcd.draw_str_6x10(38, 45, "C", false);
    draw_switch_arrow(lcd, 45, 46, state.switches.sc);

    lcd.draw_str_6x10(56, 45, "D", false);
    draw_switch_arrow(lcd, 63, 46, state.switches.sd);

    // Pots: Split bar on right (Top: VRa, Bottom: VRb) (text baseline was 52, 4x6 -> top-left y = 47)
    lcd.draw_str_4x6(74, 47, "VR", false);
    widgets::draw_split_pot_bar(lcd, 84, 46, 42, state.pots.vr1, state.pots.vr2);

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
