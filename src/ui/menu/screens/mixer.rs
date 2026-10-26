//! Dual Rate / Expo, Throttle Curve, Wing Templates, and Mixer Line Editor screens.

use crate::buzzer::Buzzer;
use crate::curve;
use crate::display::St7567;
use crate::menu::format::{
    ascii_as_str, i8_to_dec, u8_to_dec, AXIS_NAMES, DR_SWITCH_NAMES, MODE_NAMES, SOURCE_NAMES,
    SWITCH_COND_NAMES, TEMPLATE_NAMES, WEIGHT_SRC_NAMES,
};
use crate::menu::widgets;
use crate::menu::{MenuController, MenuState, NavKeys};
use crate::storage::{self, RadioStorage};

#[inline(never)]
pub fn update_dual_rate(
    ctrl: &mut MenuController,
    lcd: &mut St7567,
    keys: &NavKeys,
    storage: &mut RadioStorage,
    buzzer: &mut Buzzer,
) {
    const FIELD_COUNT: usize = 6;

    if keys.cancel {
        if ctrl.editing {
            ctrl.editing = false;
        } else {
            storage::save_active_model(storage);
            ctrl.return_to_main_menu();
            buzzer.click();
            return;
        }
    }

    let axis = ctrl.sub_idx.min(2); // 0: Roll, 1: Pitch, 2: Yaw

    if !ctrl.editing {
        widgets::navigate_4slot_list(
            &mut ctrl.selected_item,
            &mut ctrl.scroll_offset,
            FIELD_COUNT,
            keys.up,
            keys.down,
            buzzer,
        );

        if keys.ok {
            ctrl.editing = true;
            buzzer.click();
        }
    } else {
        // Editing value
        match ctrl.selected_item {
            0 => {
                // Switch (0..6)
                if let Some(dr) = keys.dr_change {
                    if storage.is_dr_switch_available(dr) {
                        storage.active_model.dr_switch = dr;
                        buzzer.play_tone(2400, 40);
                    }
                } else if keys.up {
                    storage.active_model.dr_switch =
                        storage.next_available_dr_switch(storage.active_model.dr_switch);
                    buzzer.play_tone(2200, 20);
                } else if keys.down {
                    storage.active_model.dr_switch =
                        storage.prev_available_dr_switch(storage.active_model.dr_switch);
                    buzzer.play_tone(2200, 20);
                }
            }
            1 => {
                // Axis (0..2)
                if keys.up || keys.down {
                    ctrl.sub_idx = (ctrl.sub_idx + 1) % 3;
                    buzzer.play_tone(2200, 20);
                }
            }
            2 => {
                // Hi Rate (50..100%, step 5%)
                let cur = storage.active_model.dr_high[axis];
                if keys.up && cur <= 95 {
                    storage.active_model.dr_high[axis] = cur + 5;
                    buzzer.play_tone(2200, 20);
                } else if keys.down && cur >= 55 {
                    storage.active_model.dr_high[axis] = cur - 5;
                    buzzer.play_tone(2200, 20);
                }
            }
            3 => {
                // Lo Rate (30..100%, step 5%)
                let cur = storage.active_model.dr_low[axis];
                if keys.up && cur <= 95 {
                    storage.active_model.dr_low[axis] = cur + 5;
                    buzzer.play_tone(2200, 20);
                } else if keys.down && cur >= 35 {
                    storage.active_model.dr_low[axis] = cur - 5;
                    buzzer.play_tone(2200, 20);
                }
            }
            4 => {
                // Hi Expo (-100..+100%, step 5%)
                let cur = storage.active_model.expo_high[axis];
                if keys.up && cur <= 95 {
                    storage.active_model.expo_high[axis] = cur + 5;
                    buzzer.play_tone(2200, 20);
                } else if keys.down && cur >= -95 {
                    storage.active_model.expo_high[axis] = cur - 5;
                    buzzer.play_tone(2200, 20);
                }
            }
            5 => {
                // Lo Expo (-100..+100%, step 5%)
                let cur = storage.active_model.expo_low[axis];
                if keys.up && cur <= 95 {
                    storage.active_model.expo_low[axis] = cur + 5;
                    buzzer.play_tone(2200, 20);
                } else if keys.down && cur >= -95 {
                    storage.active_model.expo_low[axis] = cur - 5;
                    buzzer.play_tone(2200, 20);
                }
            }
            _ => {}
        }

        if keys.ok {
            ctrl.editing = false;
            buzzer.click();
        }
    }

    // Render Header
    widgets::draw_header(lcd, "DUAL RATE / EXPO");

    let mut b5 = [0u8; 5];
    let mut b6 = [0u8; 6];
    for slot in 0..4 {
        let idx = ctrl.scroll_offset + slot;
        if idx >= FIELD_COUNT {
            break;
        }
        let is_sel = idx == ctrl.selected_item;

        match idx {
            0 => {
                let sw_name = DR_SWITCH_NAMES[(storage.active_model.dr_switch as usize).min(6)];
                widgets::draw_list_row(lcd, slot, is_sel, "Switch:", Some(sw_name), 60);
            }
            1 => {
                widgets::draw_list_row(lcd, slot, is_sel, "Channel:", Some(AXIS_NAMES[axis]), 60);
            }
            2 => {
                let str_val = u8_to_dec(storage.active_model.dr_high[axis], &mut b5);
                widgets::draw_list_row(lcd, slot, is_sel, "Hi Rate:", Some(str_val), 60);
            }
            3 => {
                let str_val = u8_to_dec(storage.active_model.dr_low[axis], &mut b5);
                widgets::draw_list_row(lcd, slot, is_sel, "Lo Rate:", Some(str_val), 60);
            }
            4 => {
                let str_val = i8_to_dec(storage.active_model.expo_high[axis], &mut b6);
                widgets::draw_list_row(lcd, slot, is_sel, "Hi Expo:", Some(str_val), 60);
            }
            5 => {
                let str_val = i8_to_dec(storage.active_model.expo_low[axis], &mut b6);
                widgets::draw_list_row(lcd, slot, is_sel, "Lo Expo:", Some(str_val), 60);
            }
            _ => {}
        }
    }

    let footer = if ctrl.editing {
        "[OK] Done   [UP/DN] Value"
    } else {
        "[OK] Edit   [ESC] Exit"
    };
    widgets::draw_footer(lcd, footer);
}

#[inline(never)]
pub fn update_throttle_curve(
    ctrl: &mut MenuController,
    lcd: &mut St7567,
    keys: &NavKeys,
    storage: &mut RadioStorage,
    buzzer: &mut Buzzer,
) {
    let pts_count = match storage.active_model.thr_curve_pts {
        9 => 9,
        7 => 7,
        _ => 5,
    };
    let max_items = 2 + pts_count; // Item 0: Pts mode, Item 1: Smooth, Item 2..(2+pts_count-1): Points

    if ctrl.editing {
        let pt_idx = ctrl.selected_item.saturating_sub(2).min(pts_count - 1);

        if keys.cancel {
            ctrl.editing = false;
            storage::save_active_model(storage);
            buzzer.click();
            return;
        }

        if keys.up {
            if storage.active_model.thr_curve[pt_idx] < 100 {
                storage.active_model.thr_curve[pt_idx] += 1;
            }
            buzzer.play_tone(2200, 20);
        }

        if keys.down {
            if storage.active_model.thr_curve[pt_idx] > 0 {
                storage.active_model.thr_curve[pt_idx] -= 1;
            }
            buzzer.play_tone(2200, 20);
        }

        if keys.ok {
            if ctrl.selected_item + 1 < max_items {
                ctrl.selected_item += 1;
            } else {
                ctrl.editing = false;
            }
            storage::save_active_model(storage);
            buzzer.click();
        } else if keys.bind {
            if ctrl.selected_item + 1 < max_items {
                ctrl.selected_item += 1;
            } else {
                ctrl.selected_item = 2; // Wrap back to P1
            }
            storage::save_active_model(storage);
            buzzer.click();
        }
    } else {
        if keys.cancel {
            storage::save_active_model(storage);
            ctrl.return_to_main_menu();
            buzzer.click();
            return;
        }

        if keys.down {
            ctrl.selected_item = (ctrl.selected_item + 1) % max_items;
            buzzer.play_tone(2200, 20);
        }

        if keys.up {
            ctrl.selected_item = if ctrl.selected_item == 0 {
                max_items - 1
            } else {
                ctrl.selected_item - 1
            };
            buzzer.play_tone(2200, 20);
        }

        if keys.bind {
            if ctrl.selected_item < 2 {
                ctrl.selected_item = 2;
            } else {
                ctrl.selected_item = (ctrl.selected_item + 1) % max_items;
                if ctrl.selected_item < 2 {
                    ctrl.selected_item = 2;
                }
            }
            buzzer.click();
        }

        if keys.ok {
            if ctrl.selected_item == 0 {
                let cur_pts = storage.active_model.thr_curve_pts;
                if cur_pts == 5 {
                    // Transition 5 -> 7
                    // Resample the 5-point curve at 7 evenly spaced fractional indices (0/6, 1/6, ..., 6/6)
                    let c = storage.active_model.thr_curve;
                    let mut new_crv = [0u8; 17];
                    for (i, pt) in new_crv.iter_mut().enumerate().take(7) {
                        let in_pct = (i as u16 * 1000) / 6;
                        *pt =
                            ((curve::evaluate_curve(in_pct, 5, false, &c) + 5) / 10).min(100) as u8;
                    }
                    storage.active_model.thr_curve = new_crv;
                    storage.active_model.thr_curve_pts = 7;
                } else if cur_pts == 7 {
                    // Transition 7 -> 9
                    // Resample the 7-point curve at 9 evenly spaced fractional indices (0/8, 1/8, ..., 8/8)
                    let c = storage.active_model.thr_curve;
                    let mut new_crv = [0u8; 17];
                    for (i, pt) in new_crv.iter_mut().enumerate().take(9) {
                        let in_pct = (i as u16 * 1000) / 8;
                        *pt =
                            ((curve::evaluate_curve(in_pct, 7, false, &c) + 5) / 10).min(100) as u8;
                    }
                    storage.active_model.thr_curve = new_crv;
                    storage.active_model.thr_curve_pts = 9;
                } else {
                    // Transition 9 -> 5 (sample evenly: indices 0, 2, 4, 6, 8)
                    let c = storage.active_model.thr_curve;
                    let mut new_crv = [0u8; 17];
                    new_crv[0] = c[0];
                    new_crv[1] = c[2];
                    new_crv[2] = c[4];
                    new_crv[3] = c[6];
                    new_crv[4] = c[8];
                    storage.active_model.thr_curve = new_crv;
                    storage.active_model.thr_curve_pts = 5;
                }
                storage::save_active_model(storage);
                buzzer.click();
            } else if ctrl.selected_item == 1 {
                storage.active_model.thr_curve_smooth =
                    if storage.active_model.thr_curve_smooth == 0 {
                        1
                    } else {
                        0
                    };
                storage::save_active_model(storage);
                buzzer.click();
            } else {
                ctrl.editing = true;
                buzzer.click();
            }
        }
    }

    widgets::draw_header(lcd, "THROTTLE CURVE");

    // Left side: Mode & active point info
    let mode_str = match storage.active_model.thr_curve_pts {
        9 => "9-PT",
        7 => "7-PT",
        _ => "5-PT",
    };
    let is_sel_pts = !ctrl.editing && ctrl.selected_item == 0;
    if is_sel_pts {
        lcd.fill_rect(2, 13, 70, 9, true);
    }
    lcd.draw_str_6x10(4, 13, "Pts:", is_sel_pts);
    lcd.draw_str_6x10(32, 13, mode_str, is_sel_pts);

    let smooth_str = if storage.active_model.thr_curve_smooth != 0 {
        "SMOOTH"
    } else {
        "LINEAR"
    };
    let is_sel_crv = !ctrl.editing && ctrl.selected_item == 1;
    if is_sel_crv {
        lcd.fill_rect(2, 23, 70, 9, true);
    }
    lcd.draw_str_6x10(4, 23, "Crv:", is_sel_crv);
    lcd.draw_str_6x10(32, 23, smooth_str, is_sel_crv);

    if ctrl.selected_item >= 2 {
        let pt_idx = ctrl.selected_item - 2;
        let val = storage.active_model.thr_curve[pt_idx];
        let mut p_buf = *b"P00: 000%";
        let p_num = (pt_idx + 1) as u8;
        p_buf[1] = b'0' + (p_num / 10);
        p_buf[2] = b'0' + (p_num % 10);
        if val >= 100 {
            p_buf[5] = b'1';
            p_buf[6] = b'0';
            p_buf[7] = b'0';
        } else if val >= 10 {
            p_buf[5] = b' ';
            p_buf[6] = b'0' + (val / 10);
            p_buf[7] = b'0' + (val % 10);
        } else {
            p_buf[5] = b' ';
            p_buf[6] = b' ';
            p_buf[7] = b'0' + val;
        }
        let p_str = ascii_as_str(&p_buf[..9]);

        if ctrl.editing {
            lcd.fill_rect(2, 35, 70, 11, true);
            lcd.draw_str_6x10(4, 37, p_str, true);
        } else {
            lcd.draw_rect(2, 35, 70, 11, true);
            lcd.draw_str_6x10(4, 37, p_str, false);
        }
    } else {
        let pts_range_str = match pts_count {
            9 => "Pts: 1..9",
            7 => "Pts: 1..7",
            _ => "Pts: 1..5",
        };
        lcd.draw_str_6x10(4, 37, pts_range_str, false);
    }

    // Right side: Graph box (x = 76..124, y = 13..49)
    lcd.draw_rect(76, 13, 49, 37, true);

    // Draw curve graph inside box (width 47, height 35)
    let pts_mode = storage.active_model.thr_curve_pts;
    let is_smooth = storage.active_model.thr_curve_smooth != 0;
    let curve_data = storage.active_model.thr_curve;

    for px in 0..47 {
        let input_pct = ((px as u32 * 1000) / 46) as u16;
        let out_pct = curve::evaluate_curve(input_pct, pts_mode, is_smooth, &curve_data);
        let py = 48 - ((out_pct as i32 * 34) / 1000);
        lcd.set_pixel(77 + px, py, true);
    }

    // Draw point indicator dot on graph for selected point (3x3 dot)
    if ctrl.selected_item >= 2 {
        let pt_idx = ctrl.selected_item - 2;
        let input_pct = (pt_idx as u32 * 1000) / (pts_count as u32 - 1);
        let val = storage.active_model.thr_curve[pt_idx];
        let dot_x = 77 + ((input_pct * 46) / 1000) as i32;
        let dot_y = 48 - (((val as i32 * 10) * 34) / 1000);

        lcd.fill_rect(dot_x - 1, dot_y - 1, 3, 3, true);
    }

    if ctrl.editing {
        widgets::draw_footer(lcd, "[OK] Next Pt   [ESC] Done");
    } else if ctrl.selected_item >= 2 {
        widgets::draw_footer(lcd, "[OK] Edit Pt   [ESC] Back");
    } else {
        widgets::draw_footer(lcd, "[OK] Toggle    [ESC] Back");
    }
}

#[inline(never)]
pub fn update_wing_mixer(
    ctrl: &mut MenuController,
    lcd: &mut St7567,
    keys: &NavKeys,
    storage: &mut RadioStorage,
    buzzer: &mut Buzzer,
) {
    let template = storage.active_model.wing_tail_mix;
    let has_diff = template == 1 || template == 3;
    let gvar_item_idx = if has_diff { 2 } else { 1 };
    let mix_items = if has_diff { 35 } else { 34 };

    if ctrl.editing {
        if keys.cancel {
            ctrl.editing = false;
            storage::save_active_model(storage);
            buzzer.click();
            return;
        }
        if keys.up && storage.active_model.template_diff <= 95 {
            storage.active_model.template_diff += 5;
            buzzer.play_tone(2200, 20);
        } else if keys.down && storage.active_model.template_diff >= -95 {
            storage.active_model.template_diff -= 5;
            buzzer.play_tone(2200, 20);
        }
        if keys.ok {
            ctrl.editing = false;
            storage::save_active_model(storage);
            buzzer.click();
        }
    } else {
        if keys.cancel {
            storage::save_active_model(storage);
            ctrl.return_to_main_menu();
            buzzer.click();
            return;
        }

        widgets::navigate_4slot_list(
            &mut ctrl.selected_item,
            &mut ctrl.scroll_offset,
            mix_items,
            keys.up,
            keys.down,
            buzzer,
        );

        if keys.ok {
            buzzer.click();
            if ctrl.selected_item == 0 {
                // Cycle Wing Template (0..3)
                storage.active_model.wing_tail_mix = (storage.active_model.wing_tail_mix + 1) % 4;
            } else if has_diff && ctrl.selected_item == 1 {
                ctrl.editing = true;
            } else if ctrl.selected_item == gvar_item_idx {
                // Open Global Variables Editor
                ctrl.selected_item = 0;
                ctrl.scroll_offset = 0;
                ctrl.editing = false;
                ctrl.state = MenuState::GlobalVars;
                ctrl.waiting_release = true;
                return;
            } else {
                // Open Mix Line Editor
                let m_idx = ctrl.selected_item - (gvar_item_idx + 1);
                ctrl.page_idx = m_idx.min(31);
                ctrl.selected_item = 0;
                ctrl.scroll_offset = 0;
                ctrl.editing = false;
                ctrl.state = MenuState::MixerLineEdit;
                ctrl.waiting_release = true;
                return;
            }
        }
    }

    widgets::draw_header(lcd, "WING & MIXER");

    let mut b6 = [0u8; 6];

    for slot in 0..4 {
        let idx = ctrl.scroll_offset + slot;
        if idx >= mix_items {
            break;
        }
        let y = 14 + (slot as i32 * 9);
        let is_sel = idx == ctrl.selected_item;
        if is_sel {
            lcd.fill_rect(2, y, 124, 9, true);
        }

        if idx == 0 {
            let t_idx = (storage.active_model.wing_tail_mix as usize).min(3);
            lcd.draw_str_6x10(4, y, "Wing:", is_sel);
            lcd.draw_str_6x10(36, y, TEMPLATE_NAMES[t_idx], is_sel);
        } else if has_diff && idx == 1 {
            let d_str = i8_to_dec(storage.active_model.template_diff, &mut b6);
            lcd.draw_str_6x10(4, y, "Diff:", is_sel);
            lcd.draw_str_6x10(36, y, d_str, is_sel);
        } else if idx == gvar_item_idx {
            lcd.draw_str_6x10(4, y, "GVars: [Edit GV1..5]", is_sel);
        } else {
            let m_idx = idx - (gvar_item_idx + 1);
            let mix = storage.active_model.mixes[m_idx];
            let mut m_buf = *b"M00: ";
            let m_num = (m_idx + 1) as u8;
            m_buf[1] = b'0' + (m_num / 10);
            m_buf[2] = b'0' + (m_num % 10);
            let m_label = ascii_as_str(&m_buf);
            lcd.draw_str_6x10(4, y, m_label, is_sel);

            if mix.target_ch == 0 {
                lcd.draw_str_6x10(32, y, "[DISABLED]", is_sel);
            } else {
                let mut ch_buf = *b"CH00";
                if mix.target_ch >= 10 {
                    ch_buf[2] = b'0' + (mix.target_ch / 10);
                    ch_buf[3] = b'0' + (mix.target_ch % 10);
                } else {
                    ch_buf[2] = b'0' + mix.target_ch;
                    ch_buf[3] = b' ';
                }
                let ch_str = ascii_as_str(&ch_buf);
                lcd.draw_str_6x10(30, y, ch_str, is_sel);

                lcd.draw_str_6x10(58, y, "<-", is_sel);
                let s_idx = (mix.source as usize).min(SOURCE_NAMES.len() - 1);
                lcd.draw_str_6x10(74, y, SOURCE_NAMES[s_idx], is_sel);
            }
        }
    }

    let footer = if ctrl.editing {
        "[OK] Done   [UP/DN] Diff"
    } else {
        "[OK] Select/Edit   [ESC] Back"
    };
    widgets::draw_footer(lcd, footer);
}

#[inline(never)]
pub fn update_mixer_line_edit(
    ctrl: &mut MenuController,
    lcd: &mut St7567,
    keys: &NavKeys,
    storage: &mut RadioStorage,
    buzzer: &mut Buzzer,
) {
    let mix_idx = ctrl.page_idx.min(31);
    const FIELD_COUNT: usize = 7;

    if keys.cancel {
        if ctrl.editing {
            ctrl.editing = false;
        } else {
            ctrl.state = MenuState::WingMixer;
            ctrl.selected_item = mix_idx + 1;
            ctrl.scroll_offset = (mix_idx + 1).saturating_sub(3);
            ctrl.waiting_release = true;
            buzzer.click();
            return;
        }
    }

    if !ctrl.editing {
        widgets::navigate_4slot_list(
            &mut ctrl.selected_item,
            &mut ctrl.scroll_offset,
            FIELD_COUNT,
            keys.up,
            keys.down,
            buzzer,
        );

        if keys.ok {
            ctrl.editing = true;
            buzzer.click();
        }
    } else {
        let mut mix = storage.active_model.mixes[mix_idx];
        match ctrl.selected_item {
            0 => {
                if keys.up {
                    mix.target_ch = (mix.target_ch + 1) % 19;
                    buzzer.play_tone(2200, 20);
                } else if keys.down {
                    mix.target_ch = if mix.target_ch == 0 {
                        18
                    } else {
                        mix.target_ch - 1
                    };
                    buzzer.play_tone(2200, 20);
                }
            }
            1 => {
                if let Some(pot) = keys.pot_change {
                    if storage.is_source_available(pot) {
                        mix.source = pot;
                        buzzer.play_tone(2400, 40);
                    }
                } else if keys.up {
                    mix.source = storage.next_available_source(mix.source, SOURCE_NAMES.len());
                    buzzer.play_tone(2200, 20);
                } else if keys.down {
                    mix.source = storage.prev_available_source(mix.source, SOURCE_NAMES.len());
                    buzzer.play_tone(2200, 20);
                }
            }
            2 => {
                // Weight Modulation Source: 0..=36 (WEIGHT_SRC_NAMES)
                if keys.up {
                    mix.weight_src =
                        storage.next_available_weight_src(mix.weight_src, WEIGHT_SRC_NAMES.len());
                    buzzer.play_tone(2200, 20);
                } else if keys.down {
                    mix.weight_src =
                        storage.prev_available_weight_src(mix.weight_src, WEIGHT_SRC_NAMES.len());
                    buzzer.play_tone(2200, 20);
                }
            }
            3 => {
                if keys.up && mix.weight <= 95 {
                    mix.weight += 5;
                    buzzer.play_tone(2200, 20);
                } else if keys.down && mix.weight >= -95 {
                    mix.weight -= 5;
                    buzzer.play_tone(2200, 20);
                }
            }
            4 => {
                if keys.up && mix.offset <= 95 {
                    mix.offset += 5;
                    buzzer.play_tone(2200, 20);
                } else if keys.down && mix.offset >= -95 {
                    mix.offset -= 5;
                    buzzer.play_tone(2200, 20);
                }
            }
            5 => {
                if let Some(sw) = keys.sw_change {
                    if storage.is_switch_cond_available(sw) {
                        mix.switch = sw;
                        buzzer.play_tone(2400, 40);
                    }
                } else if keys.up {
                    mix.switch =
                        storage.next_available_switch_cond(mix.switch, SWITCH_COND_NAMES.len());
                    buzzer.play_tone(2200, 20);
                } else if keys.down {
                    mix.switch =
                        storage.prev_available_switch_cond(mix.switch, SWITCH_COND_NAMES.len());
                    buzzer.play_tone(2200, 20);
                }
            }
            6 => {
                if keys.up {
                    mix.mode = (mix.mode + 1) % 3;
                    buzzer.play_tone(2200, 20);
                } else if keys.down {
                    mix.mode = if mix.mode == 0 { 2 } else { mix.mode - 1 };
                    buzzer.play_tone(2200, 20);
                }
            }
            _ => {}
        }
        storage.active_model.mixes[mix_idx] = mix;

        if keys.ok {
            ctrl.editing = false;
            buzzer.click();
        }
    }

    let mut title_buf = *b"EDIT MIX 00";
    let m_num = (mix_idx + 1) as u8;
    title_buf[9] = b'0' + (m_num / 10);
    title_buf[10] = b'0' + (m_num % 10);
    let title_str = ascii_as_str(&title_buf);
    widgets::draw_header(lcd, title_str);

    let mix = storage.active_model.mixes[mix_idx];
    let mut b6 = [0u8; 6];
    for slot in 0..4 {
        let idx = ctrl.scroll_offset + slot;
        if idx >= FIELD_COUNT {
            break;
        }
        let is_sel = idx == ctrl.selected_item;

        match idx {
            0 => {
                if mix.target_ch == 0 {
                    widgets::draw_list_row(lcd, slot, is_sel, "Target:", Some("Disabled"), 56);
                } else {
                    let mut ch_buf = *b"CH00";
                    if mix.target_ch >= 10 {
                        ch_buf[2] = b'0' + (mix.target_ch / 10);
                        ch_buf[3] = b'0' + (mix.target_ch % 10);
                    } else {
                        ch_buf[2] = b'0' + mix.target_ch;
                        ch_buf[3] = b' ';
                    }
                    let ch_str = ascii_as_str(&ch_buf);
                    widgets::draw_list_row(lcd, slot, is_sel, "Target:", Some(ch_str), 56);
                }
            }
            1 => {
                let s_idx = (mix.source as usize).min(SOURCE_NAMES.len() - 1);
                widgets::draw_list_row(lcd, slot, is_sel, "Source:", Some(SOURCE_NAMES[s_idx]), 56);
            }
            2 => {
                let ws_idx = (mix.weight_src as usize).min(WEIGHT_SRC_NAMES.len() - 1);
                widgets::draw_list_row(
                    lcd,
                    slot,
                    is_sel,
                    "Wt Src:",
                    Some(WEIGHT_SRC_NAMES[ws_idx]),
                    56,
                );
            }
            3 => {
                let w_str = i8_to_dec(mix.weight, &mut b6);
                widgets::draw_list_row(lcd, slot, is_sel, "Weight:", Some(w_str), 56);
            }
            4 => {
                let o_str = i8_to_dec(mix.offset, &mut b6);
                widgets::draw_list_row(lcd, slot, is_sel, "Offset:", Some(o_str), 56);
            }
            5 => {
                let sw_idx = (mix.switch as usize).min(SWITCH_COND_NAMES.len() - 1);
                widgets::draw_list_row(
                    lcd,
                    slot,
                    is_sel,
                    "Switch:",
                    Some(SWITCH_COND_NAMES[sw_idx]),
                    56,
                );
            }
            6 => {
                let m_idx = (mix.mode as usize).min(2);
                widgets::draw_list_row(lcd, slot, is_sel, "Mode:", Some(MODE_NAMES[m_idx]), 56);
            }
            _ => {}
        }
    }

    let footer = if ctrl.editing {
        "[OK] Done   [UP/DN] Value"
    } else {
        "[OK] Edit   [ESC] Back"
    };
    widgets::draw_footer(lcd, footer);
}

#[inline(never)]
pub fn update_global_vars(
    ctrl: &mut MenuController,
    lcd: &mut St7567,
    keys: &NavKeys,
    storage: &mut RadioStorage,
    buzzer: &mut Buzzer,
) {
    const GVAR_COUNT: usize = 5;

    if ctrl.editing {
        let gv_idx = ctrl.selected_item.min(GVAR_COUNT - 1);
        let cur = storage.active_model.gvars[gv_idx];
        if keys.up && cur <= 950 {
            storage.active_model.gvars[gv_idx] = cur + 50; // 5% step
            buzzer.play_tone(2200, 20);
        } else if keys.down && cur >= -950 {
            storage.active_model.gvars[gv_idx] = cur - 50; // 5% step
            buzzer.play_tone(2200, 20);
        }
        if keys.ok || keys.cancel {
            ctrl.editing = false;
            storage::save_active_model(storage);
            buzzer.click();
        }
    } else {
        if keys.cancel {
            storage::save_active_model(storage);
            ctrl.return_to_main_menu();
            buzzer.click();
            return;
        }

        widgets::navigate_4slot_list(
            &mut ctrl.selected_item,
            &mut ctrl.scroll_offset,
            GVAR_COUNT,
            keys.up,
            keys.down,
            buzzer,
        );

        if keys.ok {
            ctrl.editing = true;
            buzzer.click();
        }
    }

    widgets::draw_header(lcd, "GLOBAL VARIABLES");

    let mut b5 = [0u8; 5];
    for slot in 0..4 {
        let idx = ctrl.scroll_offset + slot;
        if idx >= GVAR_COUNT {
            break;
        }
        let is_sel = idx == ctrl.selected_item;
        let mut label_buf = *b"GV0:";
        label_buf[2] = b'1' + (idx as u8);
        let label_str = ascii_as_str(&label_buf);
        let val = storage.active_model.gvars[idx];
        let val_str = crate::ui::format::format_percent(val, &mut b5);
        widgets::draw_list_row(lcd, slot, is_sel, label_str, Some(val_str), 56);
    }

    let footer = if ctrl.editing {
        "[OK] Done   [UP/DN] Value"
    } else {
        "[OK] Edit   [ESC] Back"
    };
    widgets::draw_footer(lcd, footer);
}
