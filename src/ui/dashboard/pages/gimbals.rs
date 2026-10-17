//! Page 0 (P1/5): Primary Gimbals & Trims, Switches, Pots, and Trim status.

use crate::adc;
use crate::display::St7567;
use crate::input::InputState;
use crate::storage::{AdcInputMode, RadioStorage};
use crate::trim::{self, TrimController};
use crate::ui::format::{format_percent, format_throttle_percent, format_trim};
use crate::ui::glyphs::{draw_switch_arrow, draw_switch_arrow_compact};
use crate::ui::widgets;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ConfiguredPot {
    pub ch: usize,
    pub name: &'static str,
}

/// Dynamically collect all analog channels configured as potentiometers (continuous or detented).
pub fn collect_configured_pots(storage: &RadioStorage) -> ([ConfiguredPot; 10], usize) {
    let ext_adc = storage.radio.ext_adc != 0;
    let max_channels = if ext_adc { 10 } else { 6 };
    let pot_names = ["A", "B", "C", "D", "V1", "V2", "V3", "V4", "V5", "V6"];
    let mut pots = [ConfiguredPot { ch: 0, name: "" }; 10];
    let mut count = 0;

    for (ch, &name) in pot_names.iter().enumerate().take(max_channels) {
        let mode = AdcInputMode::resolve(ch, storage.radio.adc_modes[ch]);
        if mode.is_pot() {
            pots[count] = ConfiguredPot { ch, name };
            count += 1;
        }
    }
    (pots, count)
}

/// Map an auxiliary channel index (0..9) to its raw ADC sample index.
pub fn aux_channel_raw_adc(raw_adc: &[u16; adc::NUM_CHANNELS], ch: usize) -> u16 {
    raw_adc[crate::input::aux_index_to_raw_adc(ch)]
}

/// Primary orchestrator rendering Page 0.
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
    render_primary_sticks(lcd, state, storage, trims);

    let ext_adc = storage.radio.ext_adc != 0;
    let has_custom_modes = storage.radio.adc_modes.iter().any(|&m| m != 0);

    if !has_custom_modes && !ext_adc {
        render_default_layout(lcd, state, storage);
    } else {
        render_adaptive_layout(lcd, state, storage);
    }

    render_footer(lcd, trims, is_binding, timer_str, timer_expired, blink_on);
}

/// Render the 4 primary flight stick channels (Roll, Pitch, Throttle, Yaw) with gauges and percentages.
pub fn render_primary_sticks(
    lcd: &mut St7567,
    state: &InputState,
    storage: &RadioStorage,
    trims: &TrimController,
) {
    let mut pct_buf = [0u8; 5];
    let is_general = storage.active_model().model_type().is_general();

    let stick_mode = crate::safety::StickMode::from_u8(storage.radio.stick_mode);

    if is_general {
        // Physical Gimbals for General / Surface models: RH, RV, LV, LH
        // 1: RH (PA0, Right Horizontal / Steering)
        lcd.draw_str_6x10(2, 12, "RH", false);
        widgets::draw_channel_gauge(lcd, 16, 13, 72, 7, state.gimbals.rh, trims.values.roll);
        let p1 = format_percent(state.gimbals.rh, &mut pct_buf);
        lcd.draw_str_6x10(92, 12, p1, false);

        // 2: RV (PA1, Right Vertical)
        lcd.draw_str_6x10(2, 20, "RV", false);
        widgets::draw_channel_gauge(lcd, 16, 21, 72, 7, state.gimbals.rv, trims.values.pitch);
        let p2 = format_percent(state.gimbals.rv, &mut pct_buf);
        lcd.draw_str_6x10(92, 20, p2, false);

        // 3: LV (PA2, Left Vertical / Throttle)
        lcd.draw_str_6x10(2, 28, "LV", false);
        widgets::draw_channel_gauge(lcd, 16, 29, 72, 7, state.gimbals.lv, trims.values.throttle);
        let p3 = format_percent(state.gimbals.lv, &mut pct_buf);
        lcd.draw_str_6x10(92, 28, p3, false);

        // 4: LH (PA3, Left Horizontal)
        lcd.draw_str_6x10(2, 36, "LH", false);
        widgets::draw_channel_gauge(lcd, 16, 37, 72, 7, state.gimbals.lh, trims.values.yaw);
        let p4 = format_percent(state.gimbals.lh, &mut pct_buf);
        lcd.draw_str_6x10(92, 36, p4, false);
    } else {
        // Logical Flight Controls for Aircraft: A, E, T, R
        let controls = state.flight_controls(stick_mode);

        // CH1: Roll / Aileron (A)
        lcd.draw_str_6x10(2, 12, "A", false);
        widgets::draw_channel_gauge(lcd, 12, 13, 76, 7, controls.aileron, trims.values.roll);
        let p1 = format_percent(controls.aileron, &mut pct_buf);
        lcd.draw_str_6x10(92, 12, p1, false);

        // CH2: Pitch / Elevator (E)
        lcd.draw_str_6x10(2, 20, "E", false);
        widgets::draw_channel_gauge(lcd, 12, 21, 76, 7, controls.elevator, trims.values.pitch);
        let p2 = format_percent(controls.elevator, &mut pct_buf);
        lcd.draw_str_6x10(92, 20, p2, false);

        // CH3: Throttle (T)
        lcd.draw_str_6x10(2, 28, "T", false);
        let thr_trim = if storage.radio.throttle_trim != 0 {
            trims.values.throttle
        } else {
            0
        };
        widgets::draw_progress_bar(lcd, 12, 29, 76, 7, controls.throttle, thr_trim);
        let p3 = format_throttle_percent(controls.throttle, &mut pct_buf);
        lcd.draw_str_6x10(92, 28, p3, false);

        // CH4: Yaw / Rudder (R)
        lcd.draw_str_6x10(2, 36, "R", false);
        widgets::draw_channel_gauge(lcd, 12, 37, 76, 7, controls.rudder, trims.values.yaw);
        let p4 = format_percent(controls.rudder, &mut pct_buf);
        lcd.draw_str_6x10(92, 36, p4, false);
    }
}

/// Render the default switch and dual pot layout (y = 46..53) for standard stock models.
pub fn render_default_layout(lcd: &mut St7567, state: &InputState, storage: &RadioStorage) {
    crate::hw::render_default_layout(lcd, state, storage);
}

/// Render the adaptive switch and pot layout (y = 46..53) based on dynamic channel mappings.
pub fn render_adaptive_layout(lcd: &mut St7567, state: &InputState, storage: &RadioStorage) {
    let max_aux = if storage.radio.ext_adc != 0 { 10 } else { 6 };
    let mut sw_list: [(&str, crate::input::SwitchPos, bool, u8); 16] =
        [("", crate::input::SwitchPos::Up, false, 0); 16];
    let mut sw_count = 0usize;
    let mut pot_vals = [0i16; 10];
    let mut pot_count = 0usize;

    for ch in 0..max_aux {
        let mode = storage.radio.get_adc_mode(ch);
        let sw_name = crate::hw::ADAPTIVE_CH_NAMES[ch];
        match mode {
            AdcInputMode::TwoPos | AdcInputMode::ThreePos | AdcInputMode::InstantTrim => {
                let pos = match ch {
                    0 => state.switches.sa,
                    1 => state.switches.sb,
                    2 => state.switches.sc,
                    3 => state.switches.sd,
                    6 => state.switches.swi,
                    7 => state.switches.swj,
                    8 => state.switches.swk,
                    9 => state.switches.swl,
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
                if sw_count < sw_list.len() {
                    sw_list[sw_count] = (sw_name, pos, false, 0);
                    sw_count += 1;
                }
            }
            AdcInputMode::SixPos => {
                let raw_ch = aux_channel_raw_adc(&state.raw, ch);
                let mode_num = crate::input::decode_switch_6pos_num(raw_ch);
                if sw_count < sw_list.len() {
                    sw_list[sw_count] = (sw_name, crate::input::SwitchPos::Mid, true, mode_num);
                    sw_count += 1;
                }
            }
            AdcInputMode::Pot | AdcInputMode::PotDetent => {
                if pot_count < pot_vals.len() {
                    pot_vals[pot_count] = state.aux_pots[ch];
                    pot_count += 1;
                }
            }
            AdcInputMode::Default => unreachable!(),
        }
    }

    crate::hw::append_extra_switches(state, storage, &mut sw_list, &mut sw_count);

    let (sw_spacing, compact, pot_start_x, pot_w) = if pot_count == 0 {
        let spacing = if sw_count <= 1 {
            124
        } else {
            (124 / sw_count as i32).min(14)
        };
        (spacing, spacing < 10, 126, 0u32)
    } else if sw_count == 0 {
        (0, false, 2, 124u32)
    } else {
        let min_pot_w: i32 = match pot_count {
            1 => 24,
            2 => if sw_count >= 12 { 24 } else { 28 },
            _ => 36,
        };

        // If standard 10px per switch doesn't leave room for min_pot_w + 3px gap, switch to compact
        let compact = (sw_count as i32 * 10) + min_pot_w + 3 > 124;
        let slot_w: i32 = if compact { 8 } else { 10 };

        let max_sw_w = 124 - min_pot_w - 3;
        let natural_sw_w = sw_count as i32 * (if compact { 8 } else { 11 });
        let actual_sw_w = natural_sw_w.min(max_sw_w).max(sw_count as i32 * (slot_w - 1));

        let spacing = if sw_count <= 1 {
            actual_sw_w
        } else {
            actual_sw_w / sw_count as i32
        };

        let last_sw_end = 2 + (sw_count as i32 - 1) * spacing + if compact { 7 } else { 10 };
        let p_start = (last_sw_end + 3).max(2 + actual_sw_w + 3);
        let p_w = (126 - p_start).max(0) as u32;

        (spacing, compact, p_start, p_w)
    };

    if sw_count > 0 {
        for (i, &(name, pos, is_6pos, mode_num)) in sw_list.iter().take(sw_count).enumerate() {
            let x = 2 + (i as i32 * sw_spacing);
            draw_switch_slot(lcd, x, compact, name, pos, is_6pos, mode_num);
        }
    }

    if pot_count > 0 && pot_w > 0 {
        // Vertical divider separating switches from pots
        if sw_count > 0 {
            let div_x = pot_start_x - 2;
            if div_x > 2 && div_x < 126 {
                lcd.draw_vline(div_x, 46, 8, true);
            }
        }

        if pot_count == 1 {
            widgets::draw_single_pot_bar(lcd, pot_start_x, 46, pot_w, pot_vals[0]);
        } else if pot_count == 2 {
            widgets::draw_split_pot_bar(lcd, pot_start_x, 46, pot_w, pot_vals[0], pot_vals[1]);
        } else {
            widgets::draw_multi_pot_bar(lcd, pot_start_x, 45, pot_w, &pot_vals[..pot_count]);
        }
    }
}

/// Render the standardized footer (y = 55..63) with bind status, active trims, timer, or navigation hints.
pub fn render_footer(
    lcd: &mut St7567,
    trims: &TrimController,
    is_binding: bool,
    timer_str: Option<&str>,
    timer_expired: bool,
    blink_on: bool,
) {
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

pub fn draw_switch_slot(
    lcd: &mut St7567,
    x: i32,
    compact: bool,
    name: &str,
    pos: crate::input::SwitchPos,
    is_6pos: bool,
    mode_num: u8,
) {
    lcd.draw_str_4x6(x + 1, 48, name, false);
    let glyph_x = x + 5;
    if is_6pos {
        let num_char = [b'0' + mode_num.clamp(1, 6)];
        let num_str = crate::ui::format::ascii_as_str(&num_char);
        lcd.draw_str_4x6(glyph_x, 47, num_str, false);
    } else if compact {
        draw_switch_arrow_compact(lcd, glyph_x, 46, pos);
    } else {
        draw_switch_arrow(lcd, glyph_x, 46, pos);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adaptive_layout_no_overlap() {
        let mut storage = RadioStorage::default_factory();
        storage.radio.ext_adc = 1;
        storage.radio.ext_switches = 1;

        let state = InputState {
            gimbals: Default::default(),
            pots: Default::default(),
            switches: Default::default(),
            battery_mv: 4200,
            aux_pots: [0; 10],
            raw: [2048; adc::NUM_CHANNELS],
        };

        // Helper to find the continuous vertical divider column between switches and pots
        let find_divider = |lcd: &St7567| -> Option<usize> {
            (40..115).find(|&x| {
                (46..54).all(|y| lcd.framebuffer()[(y / 8) * 128 + x] & (1 << (y % 8)) != 0)
            })
        };

        // 1. Default (6 pots VRA..VRF, 6 switches SA..SF)
        let mut lcd1 = St7567::new();
        render_adaptive_layout(&mut lcd1, &state, &storage);
        let div1 = find_divider(&lcd1);
        assert!(div1.is_some(), "Divider must separate 6 switches and 6 pots");

        // 2. Extended channels as switches (2 pots, 10 switches)
        for ch in 6..10 {
            storage.radio.set_adc_mode_val(ch, 1);
        }
        let mut lcd2 = St7567::new();
        render_adaptive_layout(&mut lcd2, &state, &storage);
        let div2 = find_divider(&lcd2);
        assert!(div2.is_some(), "Divider must separate 10 switches and 2 pots");

        // 3. Extended switches + SG/SH (2 pots, up to 12 switches)
        storage.radio.j4_sg_sh_en = 1;
        let mut lcd3 = St7567::new();
        render_adaptive_layout(&mut lcd3, &state, &storage);
        let div3 = find_divider(&lcd3);
        assert!(div3.is_some(), "Divider must separate switches and 2 pots");

        // 4. 6 pots + up to 8 switches (VRA..VRF + SA..SH)
        let mut storage4 = RadioStorage::default_factory();
        storage4.radio.ext_adc = 1;
        storage4.radio.ext_switches = 1;
        storage4.radio.j4_sg_sh_en = 1;
        let mut lcd4 = St7567::new();
        render_adaptive_layout(&mut lcd4, &state, &storage4);
        let div4 = find_divider(&lcd4);
        assert!(div4.is_some(), "Divider must separate switches and 6 pots");
    }
}
