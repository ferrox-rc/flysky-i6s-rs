//! Dual Rate / Expo, Throttle Curve, Wing Templates, and Mixer Line Editor screens.

use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
    text::Text,
};

use crate::buzzer::Buzzer;
use crate::curve;
use crate::display::St7567;
use crate::menu::format::{
    ascii_as_str, i8_to_dec, u8_to_dec, AXIS_NAMES, DR_SWITCH_NAMES, MODE_NAMES, SOURCE_NAMES, SWITCH_COND_NAMES,
    TEMPLATE_NAMES,
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
    let active_idx = storage.radio.active_model as usize;
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
                // Switch (0..4)
                if let Some(dr) = keys.dr_change {
                    storage.models[active_idx].dr_switch = dr;
                    buzzer.play_tone(2400, 40);
                } else if keys.up {
                    storage.models[active_idx].dr_switch =
                        (storage.models[active_idx].dr_switch + 1) % 5;
                    buzzer.play_tone(2200, 20);
                } else if keys.down {
                    storage.models[active_idx].dr_switch =
                        if storage.models[active_idx].dr_switch == 0 {
                            4
                        } else {
                            storage.models[active_idx].dr_switch - 1
                        };
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
                let cur = storage.models[active_idx].dr_high[axis];
                if keys.up && cur <= 95 {
                    storage.models[active_idx].dr_high[axis] = cur + 5;
                    buzzer.play_tone(2200, 20);
                } else if keys.down && cur >= 55 {
                    storage.models[active_idx].dr_high[axis] = cur - 5;
                    buzzer.play_tone(2200, 20);
                }
            }
            3 => {
                // Lo Rate (30..100%, step 5%)
                let cur = storage.models[active_idx].dr_low[axis];
                if keys.up && cur <= 95 {
                    storage.models[active_idx].dr_low[axis] = cur + 5;
                    buzzer.play_tone(2200, 20);
                } else if keys.down && cur >= 35 {
                    storage.models[active_idx].dr_low[axis] = cur - 5;
                    buzzer.play_tone(2200, 20);
                }
            }
            4 => {
                // Hi Expo (-100..+100%, step 5%)
                let cur = storage.models[active_idx].expo_high[axis];
                if keys.up && cur <= 95 {
                    storage.models[active_idx].expo_high[axis] = cur + 5;
                    buzzer.play_tone(2200, 20);
                } else if keys.down && cur >= -95 {
                    storage.models[active_idx].expo_high[axis] = cur - 5;
                    buzzer.play_tone(2200, 20);
                }
            }
            5 => {
                // Lo Expo (-100..+100%, step 5%)
                let cur = storage.models[active_idx].expo_low[axis];
                if keys.up && cur <= 95 {
                    storage.models[active_idx].expo_low[axis] = cur + 5;
                    buzzer.play_tone(2200, 20);
                } else if keys.down && cur >= -95 {
                    storage.models[active_idx].expo_low[axis] = cur - 5;
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
                let sw_name =
                    DR_SWITCH_NAMES[(storage.models[active_idx].dr_switch as usize).min(4)];
                widgets::draw_list_row(lcd, slot, is_sel, "Switch:", Some(sw_name), 60);
            }
            1 => {
                widgets::draw_list_row(lcd, slot, is_sel, "Channel:", Some(AXIS_NAMES[axis]), 60);
            }
            2 => {
                let str_val = u8_to_dec(storage.models[active_idx].dr_high[axis], &mut b5);
                widgets::draw_list_row(lcd, slot, is_sel, "Hi Rate:", Some(str_val), 60);
            }
            3 => {
                let str_val = u8_to_dec(storage.models[active_idx].dr_low[axis], &mut b5);
                widgets::draw_list_row(lcd, slot, is_sel, "Lo Rate:", Some(str_val), 60);
            }
            4 => {
                let str_val = i8_to_dec(storage.models[active_idx].expo_high[axis], &mut b6);
                widgets::draw_list_row(lcd, slot, is_sel, "Hi Expo:", Some(str_val), 60);
            }
            5 => {
                let str_val = i8_to_dec(storage.models[active_idx].expo_low[axis], &mut b6);
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
    let active_idx = storage.radio.active_model as usize;
    let pts_count = if storage.models[active_idx].thr_curve_pts == 9 {
        9
    } else {
        5
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
            if storage.models[active_idx].thr_curve[pt_idx] < 100 {
                storage.models[active_idx].thr_curve[pt_idx] += 1;
            }
            buzzer.play_tone(2200, 20);
        }

        if keys.down {
            if storage.models[active_idx].thr_curve[pt_idx] > 0 {
                storage.models[active_idx].thr_curve[pt_idx] -= 1;
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
                if storage.models[active_idx].thr_curve_pts == 5 {
                    let c = storage.models[active_idx].thr_curve;
                    storage.models[active_idx].thr_curve = [
                        c[0],
                        ((c[0] as u16 + c[1] as u16) / 2) as u8,
                        c[1],
                        ((c[1] as u16 + c[2] as u16) / 2) as u8,
                        c[2],
                        ((c[2] as u16 + c[3] as u16) / 2) as u8,
                        c[3],
                        ((c[3] as u16 + c[4] as u16) / 2) as u8,
                        c[4],
                    ];
                    storage.models[active_idx].thr_curve_pts = 9;
                } else {
                    let c = storage.models[active_idx].thr_curve;
                    storage.models[active_idx].thr_curve =
                        [c[0], c[2], c[4], c[6], c[8], 0, 0, 0, 0];
                    storage.models[active_idx].thr_curve_pts = 5;
                }
                storage::save_active_model(storage);
                buzzer.click();
            } else if ctrl.selected_item == 1 {
                storage.models[active_idx].thr_curve_smooth =
                    if storage.models[active_idx].thr_curve_smooth == 0 {
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

    let text_style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
    let fill_style = PrimitiveStyle::with_fill(BinaryColor::On);
    let border_style = PrimitiveStyle::with_stroke(BinaryColor::On, 1);

    // Left side: Mode & active point info
    let mode_str = if storage.models[active_idx].thr_curve_pts == 9 {
        "9-PT"
    } else {
        "5-PT"
    };
    let is_sel_pts = !ctrl.editing && ctrl.selected_item == 0;
    if is_sel_pts {
        Rectangle::new(Point::new(2, 13), Size::new(70, 9))
            .into_styled(fill_style)
            .draw(lcd)
            .ok();
        let inv_style = MonoTextStyle::new(&FONT_6X10, BinaryColor::Off);
        Text::new("Pts:", Point::new(4, 20), inv_style)
            .draw(lcd)
            .ok();
        Text::new(mode_str, Point::new(32, 20), inv_style)
            .draw(lcd)
            .ok();
    } else {
        Text::new("Pts:", Point::new(4, 20), text_style)
            .draw(lcd)
            .ok();
        Text::new(mode_str, Point::new(32, 20), text_style)
            .draw(lcd)
            .ok();
    }

    let smooth_str = if storage.models[active_idx].thr_curve_smooth != 0 {
        "SMOOTH"
    } else {
        "LINEAR"
    };
    let is_sel_crv = !ctrl.editing && ctrl.selected_item == 1;
    if is_sel_crv {
        Rectangle::new(Point::new(2, 23), Size::new(70, 9))
            .into_styled(fill_style)
            .draw(lcd)
            .ok();
        let inv_style = MonoTextStyle::new(&FONT_6X10, BinaryColor::Off);
        Text::new("Crv:", Point::new(4, 30), inv_style)
            .draw(lcd)
            .ok();
        Text::new(smooth_str, Point::new(32, 30), inv_style)
            .draw(lcd)
            .ok();
    } else {
        Text::new("Crv:", Point::new(4, 30), text_style)
            .draw(lcd)
            .ok();
        Text::new(smooth_str, Point::new(32, 30), text_style)
            .draw(lcd)
            .ok();
    }

    if ctrl.selected_item >= 2 {
        let pt_idx = ctrl.selected_item - 2;
        let val = storage.models[active_idx].thr_curve[pt_idx];
        let mut p_buf = *b"P1:  0%";
        p_buf[1] = b'1' + pt_idx as u8;
        if val >= 100 {
            p_buf[3] = b'1';
            p_buf[4] = b'0';
            p_buf[5] = b'0';
        } else {
            p_buf[3] = b' ';
            p_buf[4] = b'0' + (val / 10);
            p_buf[5] = b'0' + (val % 10);
        }
        let p_str = ascii_as_str(&p_buf);

        if ctrl.editing {
            Rectangle::new(Point::new(2, 35), Size::new(70, 11))
                .into_styled(fill_style)
                .draw(lcd)
                .ok();
            let inv_style = MonoTextStyle::new(&FONT_6X10, BinaryColor::Off);
            Text::new(p_str, Point::new(4, 44), inv_style)
                .draw(lcd)
                .ok();
        } else {
            Rectangle::new(Point::new(2, 35), Size::new(70, 11))
                .into_styled(border_style)
                .draw(lcd)
                .ok();
            Text::new(p_str, Point::new(4, 44), text_style)
                .draw(lcd)
                .ok();
        }
    } else {
        let pts_range_str = if pts_count == 9 {
            "Pts: 1..9"
        } else {
            "Pts: 1..5"
        };
        Text::new(pts_range_str, Point::new(4, 44), text_style)
            .draw(lcd)
            .ok();
    }

    // Right side: Graph box (x = 76..124, y = 13..49)
    Rectangle::new(Point::new(76, 13), Size::new(49, 37))
        .into_styled(border_style)
        .draw(lcd)
        .ok();

    // Draw curve graph inside box (width 47, height 35)
    let pts_mode = storage.models[active_idx].thr_curve_pts;
    let is_smooth = storage.models[active_idx].thr_curve_smooth != 0;
    let curve_data = storage.models[active_idx].thr_curve;

    for px in 0..47 {
        let input_pct = ((px as u32 * 1000) / 46) as u16;
        let out_pct = curve::evaluate_curve(input_pct, pts_mode, is_smooth, &curve_data);
        let py = 48 - ((out_pct as i32 * 34) / 1000);
        Pixel(Point::new(77 + px, py), BinaryColor::On)
            .draw(lcd)
            .ok();
    }

    // Draw point indicator dot on graph for selected point
    if ctrl.selected_item >= 2 {
        let pt_idx = ctrl.selected_item - 2;
        let input_pct = (pt_idx as u32 * 1000) / (pts_count as u32 - 1);
        let val = storage.models[active_idx].thr_curve[pt_idx];
        let dot_x = 77 + ((input_pct * 46) / 1000) as i32;
        let dot_y = 48 - (((val as i32 * 10) * 34) / 1000);

        for dy in -1..=1 {
            for dx in -1..=1 {
                Pixel(Point::new(dot_x + dx, dot_y + dy), BinaryColor::On)
                    .draw(lcd)
                    .ok();
            }
        }
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
    let active_idx = storage.radio.active_model as usize;
    let template = storage.models[active_idx].wing_tail_mix;
    let has_diff = template == 1 || template == 3;
    let mix_items = if has_diff { 10 } else { 9 };

    if ctrl.editing {
        if keys.cancel {
            ctrl.editing = false;
            storage::save_active_model(storage);
            buzzer.click();
            return;
        }
        if keys.up && storage.models[active_idx].template_diff <= 95 {
            storage.models[active_idx].template_diff += 5;
            buzzer.play_tone(2200, 20);
        } else if keys.down && storage.models[active_idx].template_diff >= -95 {
            storage.models[active_idx].template_diff -= 5;
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
                storage.models[active_idx].wing_tail_mix =
                    (storage.models[active_idx].wing_tail_mix + 1) % 4;
            } else if has_diff && ctrl.selected_item == 1 {
                ctrl.editing = true;
            } else {
                // Open Mix Line Editor
                let m_idx = if has_diff {
                    ctrl.selected_item - 2
                } else {
                    ctrl.selected_item - 1
                };
                ctrl.page_idx = m_idx.min(7);
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

    let fill_style = PrimitiveStyle::with_fill(BinaryColor::On);
    let text_style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
    let mut b6 = [0u8; 6];

    for slot in 0..4 {
        let idx = ctrl.scroll_offset + slot;
        if idx >= mix_items {
            break;
        }
        let y = 14 + (slot as i32 * 9);
        let is_sel = idx == ctrl.selected_item;
        let style = if is_sel {
            Rectangle::new(Point::new(2, y), Size::new(124, 9))
                .into_styled(fill_style)
                .draw(lcd)
                .ok();
            MonoTextStyle::new(&FONT_6X10, BinaryColor::Off)
        } else {
            text_style
        };

        if idx == 0 {
            let t_idx = (storage.models[active_idx].wing_tail_mix as usize).min(3);
            Text::new("Wing:", Point::new(4, y + 7), style)
                .draw(lcd)
                .ok();
            Text::new(TEMPLATE_NAMES[t_idx], Point::new(36, y + 7), style)
                .draw(lcd)
                .ok();
        } else if has_diff && idx == 1 {
            let d_str = i8_to_dec(storage.models[active_idx].template_diff, &mut b6);
            Text::new("Diff:", Point::new(4, y + 7), style)
                .draw(lcd)
                .ok();
            Text::new(d_str, Point::new(36, y + 7), style)
                .draw(lcd)
                .ok();
        } else {
            let m_idx = if has_diff { idx - 2 } else { idx - 1 };
            let mix = storage.models[active_idx].mixes[m_idx];
            let mut m_buf = *b"M0: ";
            m_buf[1] = b'1' + m_idx as u8;
            let m_label = ascii_as_str(&m_buf);
            Text::new(m_label, Point::new(4, y + 7), style)
                .draw(lcd)
                .ok();

            if mix.target_ch == 0 {
                Text::new("[DISABLED]", Point::new(28, y + 7), style)
                    .draw(lcd)
                    .ok();
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
                Text::new(ch_str, Point::new(24, y + 7), style)
                    .draw(lcd)
                    .ok();

                Text::new("<-", Point::new(54, y + 7), style).draw(lcd).ok();
                let s_idx = (mix.source as usize).min(30);
                Text::new(SOURCE_NAMES[s_idx], Point::new(70, y + 7), style)
                    .draw(lcd)
                    .ok();
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
    let active_idx = storage.radio.active_model as usize;
    let mix_idx = ctrl.page_idx.min(7);
    const FIELD_COUNT: usize = 6;

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
        let mix = &mut storage.models[active_idx].mixes[mix_idx];
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
                if keys.up {
                    mix.source = (mix.source + 1) % 31;
                    buzzer.play_tone(2200, 20);
                } else if keys.down {
                    mix.source = if mix.source == 0 { 30 } else { mix.source - 1 };
                    buzzer.play_tone(2200, 20);
                }
            }
            2 => {
                if keys.up && mix.weight <= 95 {
                    mix.weight += 5;
                    buzzer.play_tone(2200, 20);
                } else if keys.down && mix.weight >= -95 {
                    mix.weight -= 5;
                    buzzer.play_tone(2200, 20);
                }
            }
            3 => {
                if keys.up && mix.offset <= 95 {
                    mix.offset += 5;
                    buzzer.play_tone(2200, 20);
                } else if keys.down && mix.offset >= -95 {
                    mix.offset -= 5;
                    buzzer.play_tone(2200, 20);
                }
            }
            4 => {
                if let Some(sw) = keys.sw_change {
                    mix.switch = sw;
                    buzzer.play_tone(2400, 40);
                } else if keys.up {
                    mix.switch = (mix.switch + 1) % 11;
                    buzzer.play_tone(2200, 20);
                } else if keys.down {
                    mix.switch = if mix.switch == 0 { 10 } else { mix.switch - 1 };
                    buzzer.play_tone(2200, 20);
                }
            }
            5 => {
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

        if keys.ok {
            ctrl.editing = false;
            buzzer.click();
        }
    }

    let mut title_buf = *b"EDIT MIX 0";
    title_buf[9] = b'1' + mix_idx as u8;
    let title_str = ascii_as_str(&title_buf);
    widgets::draw_header(lcd, title_str);

    let mix = storage.models[active_idx].mixes[mix_idx];
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
                let s_idx = (mix.source as usize).min(30);
                widgets::draw_list_row(lcd, slot, is_sel, "Source:", Some(SOURCE_NAMES[s_idx]), 56);
            }
            2 => {
                let w_str = i8_to_dec(mix.weight, &mut b6);
                widgets::draw_list_row(lcd, slot, is_sel, "Weight:", Some(w_str), 56);
            }
            3 => {
                let o_str = i8_to_dec(mix.offset, &mut b6);
                widgets::draw_list_row(lcd, slot, is_sel, "Offset:", Some(o_str), 56);
            }
            4 => {
                let sw_idx = (mix.switch as usize).min(10);
                widgets::draw_list_row(
                    lcd,
                    slot,
                    is_sel,
                    "Switch:",
                    Some(SWITCH_COND_NAMES[sw_idx]),
                    56,
                );
            }
            5 => {
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
