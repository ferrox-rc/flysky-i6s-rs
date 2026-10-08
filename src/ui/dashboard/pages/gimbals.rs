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
    let is_general = storage.active_model().model_type().is_general();

    let stick_mode = crate::safety::StickMode::from_u8(storage.radio.stick_mode);

    if is_general {
        // Physical Raw Gimbals: RH, RV, LV, LH
        // 1: RH (PA0, Right Horizontal)
        lcd.draw_str_6x10(2, 11, "RH", false);
        widgets::draw_channel_gauge(lcd, 16, 13, 72, 7, state.gimbals.rh, trims.values.roll);
        let p1 = format_percent(state.gimbals.rh, &mut pct_buf);
        lcd.draw_str_6x10(92, 11, p1, false);

        // 2: RV (PA1, Right Vertical)
        lcd.draw_str_6x10(2, 19, "RV", false);
        widgets::draw_channel_gauge(lcd, 16, 21, 72, 7, state.gimbals.rv, trims.values.pitch);
        let p2 = format_percent(state.gimbals.rv, &mut pct_buf);
        lcd.draw_str_6x10(92, 19, p2, false);

        // 3: LV (PA2, Left Vertical)
        lcd.draw_str_6x10(2, 27, "LV", false);
        widgets::draw_channel_gauge(lcd, 16, 29, 72, 7, state.gimbals.lv, trims.values.throttle);
        let p3 = format_percent(state.gimbals.lv, &mut pct_buf);
        lcd.draw_str_6x10(92, 27, p3, false);

        // 4: LH (PA3, Left Horizontal)
        lcd.draw_str_6x10(2, 35, "LH", false);
        widgets::draw_channel_gauge(lcd, 16, 37, 72, 7, state.gimbals.lh, trims.values.yaw);
        let p4 = format_percent(state.gimbals.lh, &mut pct_buf);
        lcd.draw_str_6x10(92, 35, p4, false);
    } else {
        // Logical Flight Controls for Aircraft: A, E, T, R
        let controls = state.flight_controls(stick_mode);

        // CH1: Roll / Aileron (A)
        lcd.draw_str_6x10(2, 11, "A", false);
        widgets::draw_channel_gauge(lcd, 12, 13, 76, 7, controls.aileron, trims.values.roll);
        let p1 = format_percent(controls.aileron, &mut pct_buf);
        lcd.draw_str_6x10(92, 11, p1, false);

        // CH2: Pitch / Elevator (E)
        lcd.draw_str_6x10(2, 19, "E", false);
        widgets::draw_channel_gauge(lcd, 12, 21, 76, 7, controls.elevator, trims.values.pitch);
        let p2 = format_percent(controls.elevator, &mut pct_buf);
        lcd.draw_str_6x10(92, 19, p2, false);

        // CH3: Throttle (T)
        lcd.draw_str_6x10(2, 27, "T", false);
        let thr_trim = if storage.radio.throttle_trim != 0 {
            trims.values.throttle
        } else {
            0
        };
        widgets::draw_progress_bar(lcd, 12, 29, 76, 7, controls.throttle, thr_trim);
        let p3 = format_throttle_percent(controls.throttle, &mut pct_buf);
        lcd.draw_str_6x10(92, 27, p3, false);

        // CH4: Yaw / Rudder (R)
        lcd.draw_str_6x10(2, 35, "R", false);
        widgets::draw_channel_gauge(lcd, 12, 37, 76, 7, controls.rudder, trims.values.yaw);
        let p4 = format_percent(controls.rudder, &mut pct_buf);
        lcd.draw_str_6x10(92, 35, p4, false);
    }

    // Switches Line with graphic arrows (y = 46..53, text baseline was 53 -> top-left y = 45)
    let sg_sh_active = storage.radio.j4_sg_sh_en != 0;

    if sg_sh_active {
        // 8 switches: A, B, C, D, E, F, G, H with 4x6 labels and compact arrows
        let sw_list = [
            ("A", state.switches.sa),
            ("B", state.switches.sb),
            ("C", state.switches.sc),
            ("D", state.switches.sd),
            ("E", state.switches.swe),
            ("F", state.switches.swf),
            ("G", state.switches.sg),
            ("H", state.switches.sh),
        ];
        let mut x = 2;
        for (lbl, pos) in sw_list.iter() {
            lcd.draw_str_4x6(x, 47, lbl, false);
            draw_switch_arrow(lcd, x + 5, 46, *pos);
            x += 12;
        }

        // Pots: Compact split bar on right (x = 98..126)
        widgets::draw_split_pot_bar(lcd, 100, 46, 26, state.pots.vr1, state.pots.vr2);
    } else {
        // 6 switches: A, B, C, D, E, F
        let sw_list = [
            ("A", state.switches.sa),
            ("B", state.switches.sb),
            ("C", state.switches.sc),
            ("D", state.switches.sd),
            ("E", state.switches.swe),
            ("F", state.switches.swf),
        ];
        let mut x = 2;
        for (lbl, pos) in sw_list.iter() {
            lcd.draw_str_4x6(x, 47, lbl, false);
            draw_switch_arrow(lcd, x + 6, 46, *pos);
            x += 14;
        }

        // Pots: Split bar on right (Top: VRa, Bottom: VRb) (text baseline was 52, 4x6 -> top-left y = 47)
        lcd.draw_str_4x6(86, 47, "VR", false);
        widgets::draw_split_pot_bar(lcd, 96, 46, 30, state.pots.vr1, state.pots.vr2);
    }

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
