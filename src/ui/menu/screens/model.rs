//! Model selection and configuration screens.

use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
    text::Text,
};

use crate::buzzer::Buzzer;
use crate::display::St7567;
use crate::menu::format::{next_ascii, prev_ascii, u32_to_hex};
use crate::menu::widgets;
use crate::menu::{MenuController, MenuState, NavKeys};
use crate::rf;
use crate::storage::{self, ModelConfig, RadioStorage, NUM_MODELS};
use crate::trim::TrimController;
use crate::ui::format::format_timer;

pub fn update_select(
    ctrl: &mut MenuController,
    lcd: &mut St7567,
    keys: &NavKeys,
    storage: &mut RadioStorage,
    trims: &mut TrimController,
    buzzer: &mut Buzzer,
) {
    if keys.cancel {
        ctrl.state = MenuState::MainMenu;
        ctrl.selected_item = 0;
        ctrl.scroll_offset = 0;
        ctrl.waiting_release = true;
        buzzer.click();
        return;
    }

    widgets::navigate_4slot_list(
        &mut ctrl.selected_item,
        &mut ctrl.scroll_offset,
        NUM_MODELS,
        keys.up,
        keys.down,
        buzzer,
    );

    if keys.ok {
        // Save active trims into current model before switching
        let old_idx = storage.radio.active_model as usize;
        storage.models[old_idx].trims = [
            trims.values.roll,
            trims.values.pitch,
            trims.values.throttle,
            trims.values.yaw,
        ];

        // Switch active model
        let new_idx = ctrl.selected_item;
        storage.radio.active_model = new_idx as u8;

        // Load new model's trims
        trims.values.roll = storage.models[new_idx].trims[0];
        trims.values.pitch = storage.models[new_idx].trims[1];
        trims.values.throttle = storage.models[new_idx].trims[2];
        trims.values.yaw = storage.models[new_idx].trims[3];

        // Update RF driver receiver ID
        rf::set_rx_id(storage.models[new_idx].rx_id);

        // Persist to Flash
        storage::save_storage(storage);
        buzzer.play_tone_pattern(2400, 60, 40, 2);
        ctrl.waiting_release = true;
    }

    // Render Header
    widgets::draw_header(lcd, "SELECT MODEL (1-20)");

    for slot in 0..4 {
        let idx = ctrl.scroll_offset + slot;
        if idx >= NUM_MODELS {
            break;
        }
        let is_cursor = idx == ctrl.selected_item;
        let is_active = idx == (storage.radio.active_model as usize);

        let mut line_buf = [b' '; 18];
        line_buf[0] = b'M';
        line_buf[1] = b'0' + ((idx + 1) / 10) as u8;
        line_buf[2] = b'0' + ((idx + 1) % 10) as u8;
        line_buf[3] = b':';
        line_buf[4..14].copy_from_slice(&storage.models[idx].name);
        if is_active {
            line_buf[15] = b'[';
            line_buf[16] = b'*';
            line_buf[17] = b']';
        }
        let line_str = core::str::from_utf8(&line_buf).unwrap_or("M??:---");

        widgets::draw_list_row(lcd, slot, is_cursor, line_str, None, 0);
    }

    widgets::draw_footer(lcd, "[OK] Load     [ESC] Back");
}

pub fn update_setup(
    ctrl: &mut MenuController,
    lcd: &mut St7567,
    keys: &NavKeys,
    storage: &mut RadioStorage,
    buzzer: &mut Buzzer,
) {
    let active_idx = storage.radio.active_model as usize;
    const FIELD_COUNT: usize = 8;

    if !ctrl.editing {
        if keys.cancel {
            storage::save_active_model(storage);
            ctrl.state = MenuState::MainMenu;
            ctrl.selected_item = 1;
            ctrl.waiting_release = true;
            buzzer.click();
            return;
        }

        widgets::navigate_4slot_list(
            &mut ctrl.selected_item,
            &mut ctrl.scroll_offset,
            FIELD_COUNT,
            keys.up,
            keys.down || keys.bind,
            buzzer,
        );

        if keys.ok {
            match ctrl.selected_item {
                0 => {
                    ctrl.editing = true;
                    ctrl.sub_idx = 0;
                    buzzer.click();
                }
                1 => {
                    storage.models[active_idx].model_type = (storage.models[active_idx].model_type + 1) % 4;
                    storage::save_active_model(storage);
                    buzzer.click();
                }
                2 => {
                    // Arm switch edit
                    ctrl.editing = true;
                    buzzer.click();
                }
                3 => {
                    // Timer duration edit
                    ctrl.editing = true;
                    buzzer.click();
                }
                4 => {
                    // Timer trigger edit
                    ctrl.editing = true;
                    buzzer.click();
                }
                5 => {
                    // Model duplicate/copy edit
                    ctrl.editing = true;
                    ctrl.sub_idx = (active_idx + 1) % NUM_MODELS;
                    buzzer.click();
                }
                6 => {
                    ctrl.request_bind = true;
                    ctrl.state = MenuState::Closed;
                    buzzer.click();
                    return;
                }
                7 => {
                    storage.models[active_idx] = ModelConfig::default_for_index(active_idx);
                    storage::save_active_model(storage);
                    buzzer.play_tone_pattern(2200, 80, 50, 2);
                    ctrl.waiting_release = true;
                }
                _ => {}
            }
        }
    } else {
        match ctrl.selected_item {
            0 => {
                // Name editing mode
                if keys.cancel {
                    ctrl.editing = false;
                    storage::save_active_model(storage);
                    buzzer.click();
                } else if keys.bind {
                    ctrl.sub_idx = (ctrl.sub_idx + 1) % 10;
                    buzzer.click();
                } else if keys.ok {
                    if ctrl.sub_idx + 1 < 10 {
                        ctrl.sub_idx += 1;
                        buzzer.click();
                    } else {
                        ctrl.editing = false;
                        ctrl.selected_item = 1;
                        storage::save_active_model(storage);
                        buzzer.play_tone(2600, 40);
                    }
                } else if keys.up {
                    let c = storage.models[active_idx].name[ctrl.sub_idx];
                    storage.models[active_idx].name[ctrl.sub_idx] = next_ascii(c);
                    buzzer.play_tone(2200, 20);
                } else if keys.down {
                    let c = storage.models[active_idx].name[ctrl.sub_idx];
                    storage.models[active_idx].name[ctrl.sub_idx] = prev_ascii(c);
                    buzzer.play_tone(2200, 20);
                }
            }
            2 => {
                // Arm switch editing mode (with switch auto-detection)
                if let Some(sw) = keys.sw_change {
                    storage.models[active_idx].arm_switch = sw;
                    buzzer.chime_armed();
                } else if keys.up {
                    storage.models[active_idx].arm_switch = (storage.models[active_idx].arm_switch + 1) % 11;
                    buzzer.play_tone(2200, 20);
                } else if keys.down {
                    storage.models[active_idx].arm_switch = if storage.models[active_idx].arm_switch == 0 {
                        10
                    } else {
                        storage.models[active_idx].arm_switch - 1
                    };
                    buzzer.play_tone(2200, 20);
                }
                if keys.ok || keys.cancel {
                    ctrl.editing = false;
                    storage::save_active_model(storage);
                    buzzer.click();
                }
            }
            3 => {
                // Timer duration editing mode
                let cur = storage.models[active_idx].timer_secs;
                if keys.up && cur <= 3570 {
                    storage.models[active_idx].timer_secs = cur + 30;
                    buzzer.play_tone(2200, 20);
                } else if keys.down && cur >= 30 {
                    storage.models[active_idx].timer_secs = cur - 30;
                    buzzer.play_tone(2200, 20);
                }
                if keys.ok || keys.cancel {
                    ctrl.editing = false;
                    storage::save_active_model(storage);
                    buzzer.click();
                }
            }
            4 => {
                // Timer trigger editing mode (with switch auto-detection)
                if let Some(sw) = keys.sw_change {
                    storage.models[active_idx].timer_source = sw + 3; // 1..10 maps to 4..13
                    buzzer.play_tone(2400, 40);
                } else if keys.up {
                    storage.models[active_idx].timer_source = (storage.models[active_idx].timer_source + 1) % 14;
                    buzzer.play_tone(2200, 20);
                } else if keys.down {
                    storage.models[active_idx].timer_source = if storage.models[active_idx].timer_source == 0 {
                        13
                    } else {
                        storage.models[active_idx].timer_source - 1
                    };
                    buzzer.play_tone(2200, 20);
                }
                if keys.ok || keys.cancel {
                    ctrl.editing = false;
                    storage::save_active_model(storage);
                    buzzer.click();
                }
            }
            5 => {
                // Model duplicate target selection
                if keys.up {
                    ctrl.sub_idx = (ctrl.sub_idx + 1) % NUM_MODELS;
                    buzzer.play_tone(2200, 20);
                } else if keys.down {
                    ctrl.sub_idx = if ctrl.sub_idx == 0 { NUM_MODELS - 1 } else { ctrl.sub_idx - 1 };
                    buzzer.play_tone(2200, 20);
                }
                if keys.cancel {
                    ctrl.editing = false;
                    buzzer.click();
                } else if keys.ok {
                    let dst = ctrl.sub_idx.min(NUM_MODELS - 1);
                    storage.models[dst] = storage.models[active_idx];
                    storage::save_storage(storage);
                    buzzer.play_tone_pattern(2400, 60, 40, 2);
                    ctrl.editing = false;
                }
            }
            _ => {
                ctrl.editing = false;
            }
        }
    }

    widgets::draw_header(lcd, "MODEL SETUP");

    let fill_style = PrimitiveStyle::with_fill(BinaryColor::On);
    let text_style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
    let mut t_buf = [0u8; 8];

    for slot in 0..4 {
        let idx = ctrl.scroll_offset + slot;
        if idx >= FIELD_COUNT {
            break;
        }
        let y = 14 + (slot as i32 * 9);
        let is_sel = !ctrl.editing && idx == ctrl.selected_item;
        let is_edit = ctrl.editing && idx == ctrl.selected_item;
        let style = if is_sel || is_edit {
            Rectangle::new(Point::new(2, y), Size::new(124, 9)).into_styled(fill_style).draw(lcd).ok();
            MonoTextStyle::new(&FONT_6X10, BinaryColor::Off)
        } else {
            text_style
        };

        match idx {
            0 => {
                Text::new("Name:", Point::new(4, y + 7), style).draw(lcd).ok();
                let name_str = core::str::from_utf8(&storage.models[active_idx].name).unwrap_or("----------");
                Text::new(name_str, Point::new(40, y + 7), style).draw(lcd).ok();

                if ctrl.editing {
                    let char_x = 40 + (ctrl.sub_idx as i32 * 6);
                    Rectangle::new(Point::new(char_x - 1, y), Size::new(8, 9)).into_styled(fill_style).draw(lcd).ok();
                    let inv_style = MonoTextStyle::new(&FONT_6X10, BinaryColor::Off);
                    let single_char = [storage.models[active_idx].name[ctrl.sub_idx]];
                    let c_str = core::str::from_utf8(&single_char).unwrap_or("?");
                    Text::new(c_str, Point::new(char_x, y + 7), inv_style).draw(lcd).ok();
                }
            }
            1 => {
                let type_str = match storage.models[active_idx].model_type {
                    0 => "AIRPLANE",
                    1 => "GLIDER",
                    2 => "HELICOPTER",
                    _ => "MULTI / QUAD",
                };
                Text::new("Type:", Point::new(4, y + 7), style).draw(lcd).ok();
                Text::new(type_str, Point::new(40, y + 7), style).draw(lcd).ok();
            }
            2 => {
                let arm_idx = (storage.models[active_idx].arm_switch as usize).min(10);
                let arm_str = if arm_idx == 0 { "NONE" } else { crate::ui::format::SWITCH_COND_NAMES[arm_idx] };
                Text::new("Arm Sw:", Point::new(4, y + 7), style).draw(lcd).ok();
                Text::new(arm_str, Point::new(52, y + 7), style).draw(lcd).ok();
            }
            3 => {
                let t_secs = storage.models[active_idx].timer_secs;
                let t_str = if t_secs == 0 { "OFF" } else { format_timer(t_secs, false, &mut t_buf) };
                Text::new("Timer:", Point::new(4, y + 7), style).draw(lcd).ok();
                Text::new(t_str, Point::new(52, y + 7), style).draw(lcd).ok();
            }
            4 => {
                let trig_str = match storage.models[active_idx].timer_source {
                    0 => "OFF",
                    1 => "THs (RUN)",
                    2 => "THt (LTCH)",
                    3 => "ALWAYS ON",
                    s if (4..=13).contains(&s) => crate::ui::format::SWITCH_COND_NAMES[(s - 3) as usize],
                    _ => "OFF",
                };
                Text::new("T-Trig:", Point::new(4, y + 7), style).draw(lcd).ok();
                Text::new(trig_str, Point::new(52, y + 7), style).draw(lcd).ok();
            }
            5 => {
                if ctrl.editing && ctrl.selected_item == 5 {
                    let mut cp_buf = *b"Copy -> M00";
                    let target_num = (ctrl.sub_idx + 1) as u8;
                    cp_buf[9] = b'0' + (target_num / 10);
                    cp_buf[10] = b'0' + (target_num % 10);
                    let cp_str = core::str::from_utf8(&cp_buf).unwrap_or("Copy -> M??");
                    Text::new(cp_str, Point::new(4, y + 7), style).draw(lcd).ok();
                } else {
                    Text::new("Copy: [OK Duplicate]", Point::new(4, y + 7), style).draw(lcd).ok();
                }
            }
            6 => {
                let mut rx_buf = [b'0'; 8];
                u32_to_hex(storage.models[active_idx].rx_id, &mut rx_buf);
                let rx_hex_str = core::str::from_utf8(&rx_buf).unwrap_or("00000000");
                Text::new("Rx:", Point::new(4, y + 7), style).draw(lcd).ok();
                Text::new(rx_hex_str, Point::new(24, y + 7), style).draw(lcd).ok();
                Text::new("[OK Bind]", Point::new(74, y + 7), style).draw(lcd).ok();
            }
            7 => {
                Text::new("Reset: [OK Defaults]", Point::new(4, y + 7), style).draw(lcd).ok();
            }
            _ => {}
        }
    }

    if ctrl.editing {
        if ctrl.selected_item == 0 {
            widgets::draw_footer(lcd, "[OK] Next Char [ESC] Done");
        } else if ctrl.selected_item == 5 {
            widgets::draw_footer(lcd, "[OK] Duplicate [ESC] Cancel");
        } else {
            widgets::draw_footer(lcd, "[OK] Done   [UP/DN] Value");
        }
    } else {
        widgets::draw_footer(lcd, "[OK] Select    [ESC] Back");
    }
}
