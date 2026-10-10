use crate::adc;
use crate::display::St7567;
use crate::input::InputState;
use crate::storage::{AdcInputMode, RadioStorage};
use crate::trim::{self, TrimController};
use crate::ui::format::{format_percent, format_throttle_percent, format_trim};
use crate::ui::glyphs::draw_switch_arrow;
use crate::ui::widgets;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ConfiguredPot {
    pub ch: usize,
    pub name: &'static str,
}

/// Dynamically collect all analog channels configured as potentiometers (continuous or detented).
pub fn collect_configured_pots(storage: &RadioStorage) -> ([ConfiguredPot; 6], usize) {
    let pot_names = ["A", "B", "C", "D", "V1", "V2"];
    let mut pots = [ConfiguredPot { ch: 0, name: "" }; 6];
    let mut count = 0;

    for (ch, &name) in pot_names.iter().enumerate() {
        let mode = AdcInputMode::resolve(ch, storage.radio.adc_modes[ch]);
        if mode.is_pot() {
            pots[count] = ConfiguredPot { ch, name };
            count += 1;
        }
    }
    (pots, count)
}

/// Map an auxiliary channel index (0..5) to its raw ADC sample index.
pub fn aux_channel_raw_adc(raw_adc: &[u16; adc::NUM_CHANNELS], ch: usize) -> u16 {
    raw_adc[crate::input::aux_index_to_raw_adc(ch)]
}

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

    // Switches & Pots Region (y = 46..53)
    let has_custom_modes = storage.radio.adc_modes.iter().any(|&m| m != 0);

    if !has_custom_modes {
        render_default_layout(lcd, state, storage);
    } else {
        render_adaptive_layout(lcd, state, storage);
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

/// Render the default switch and dual pot layout (y = 46..53) for standard stock models.
pub fn render_default_layout(lcd: &mut St7567, state: &InputState, storage: &RadioStorage) {
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

        // Pots: Split bar on right (Top: VRa, Bottom: VRb)
        lcd.draw_str_4x6(86, 47, "VR", false);
        widgets::draw_split_pot_bar(lcd, 96, 46, 30, state.pots.vr1, state.pots.vr2);
    }
}

/// Render the adaptive switch and pot layout (y = 46..53) based on dynamic channel mappings.
pub fn render_adaptive_layout(lcd: &mut St7567, state: &InputState, storage: &RadioStorage) {
    let ch_names = ["A", "B", "C", "D", "1", "2"];
    let mut sw_list: [(&str, crate::input::SwitchPos, bool, u8); 10] =
        [("", crate::input::SwitchPos::Up, false, 0); 10];
    let mut sw_count = 0usize;
    let mut pot_vals = [0i16; 6];
    let mut pot_count = 0usize;

    for (ch, &name) in ch_names.iter().enumerate() {
        let mode = AdcInputMode::resolve(ch, storage.radio.adc_modes[ch]);
        match mode {
            AdcInputMode::TwoPos | AdcInputMode::ThreePos | AdcInputMode::InstantTrim => {
                let pos = match ch {
                    0 => state.switches.sa,
                    1 => state.switches.sb,
                    2 => state.switches.sc,
                    3 => state.switches.sd,
                    _ => {
                        let p = state.aux_pots[ch];
                        if p < -333 {
                            crate::input::SwitchPos::Up
                        } else if p < 333 {
                            crate::input::SwitchPos::Mid
                        } else {
                            crate::input::SwitchPos::Down
                        }
                    }
                };
                sw_list[sw_count] = (name, pos, false, 0);
                sw_count += 1;
            }
            AdcInputMode::SixPos => {
                let raw_ch = aux_channel_raw_adc(&state.raw, ch);
                let mode_num = crate::input::decode_switch_6pos_num(raw_ch);
                sw_list[sw_count] = (name, crate::input::SwitchPos::Mid, true, mode_num);
                sw_count += 1;
            }
            AdcInputMode::Pot | AdcInputMode::PotDetent => {
                pot_vals[pot_count] = state.aux_pots[ch];
                pot_count += 1;
            }
            AdcInputMode::Default => unreachable!(),
        }
    }

    // Include rear tactile switches E & F
    if sw_count < 10 {
        sw_list[sw_count] = ("E", state.switches.swe, false, 0);
        sw_count += 1;
    }
    if sw_count < 10 {
        sw_list[sw_count] = ("F", state.switches.swf, false, 0);
        sw_count += 1;
    }

    if storage.radio.j4_sg_sh_en != 0 {
        if sw_count < 10 {
            sw_list[sw_count] = ("G", state.switches.sg, false, 0);
            sw_count += 1;
        }
        if sw_count < 10 {
            sw_list[sw_count] = ("H", state.switches.sh, false, 0);
            sw_count += 1;
        }
    }

    let (sw_region_w, pot_start_x, pot_w) = if pot_count == 0 {
        (124i32, 126i32, 0u32)
    } else if sw_count == 0 {
        (0i32, 2i32, 124u32)
    } else {
        let min_pot_w = match pot_count {
            1 => 28i32,
            2 => 36i32,
            _ => 44i32,
        };
        let max_sw_w = 124i32 - min_pot_w - 4;
        let desired_sw_w = (sw_count as i32 * 11).min(max_sw_w);
        let p_start = 2 + desired_sw_w + 4;
        let p_width = (126 - p_start).max(min_pot_w) as u32;
        (desired_sw_w, p_start, p_width)
    };

    if sw_count > 0 {
        let sw_spacing = if sw_count == 1 {
            sw_region_w
        } else {
            (sw_region_w / sw_count as i32).max(9)
        };

        for (i, &(name, pos, is_6pos, mode_num)) in sw_list.iter().take(sw_count).enumerate() {
            let x = 2 + (i as i32 * sw_spacing);
            draw_switch_slot(lcd, x, x + 5, name, pos, is_6pos, mode_num);
        }
    }

    if pot_count > 0 && pot_w > 0 {
        if pot_count == 1 {
            widgets::draw_single_pot_bar(lcd, pot_start_x, 46, pot_w, pot_vals[0]);
        } else if pot_count == 2 {
            widgets::draw_split_pot_bar(lcd, pot_start_x, 46, pot_w, pot_vals[0], pot_vals[1]);
        } else {
            widgets::draw_multi_pot_bar(lcd, pot_start_x, 45, pot_w, &pot_vals[..pot_count]);
        }
    }
}

pub fn draw_switch_slot(
    lcd: &mut St7567,
    text_x: i32,
    glyph_x: i32,
    name: &str,
    pos: crate::input::SwitchPos,
    is_6pos: bool,
    mode_num: u8,
) {
    lcd.draw_str_4x6(text_x + 1, 48, name, false);
    if is_6pos {
        let num_char = [b'0' + mode_num.clamp(1, 6)];
        let num_str = crate::ui::format::ascii_as_str(&num_char);
        lcd.draw_str_4x6(glyph_x + 1, 47, num_str, false);
    } else {
        draw_switch_arrow(lcd, glyph_x, 46, pos);
    }
}
