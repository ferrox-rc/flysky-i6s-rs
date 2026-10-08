//! Model selection and configuration screens.

use crate::buzzer::Buzzer;
use crate::display::St7567;
use crate::menu::format::{ascii_as_str, next_ascii, prev_ascii, u32_to_hex};
use crate::menu::widgets;
use crate::menu::{MenuController, MenuState, NavKeys};
use crate::rf;
use crate::storage::{self, ModelConfig, RadioStorage, NUM_MODELS};
use crate::trim::TrimController;
use crate::ui::format::format_timer;

#[inline(never)]
pub fn update_select(
    ctrl: &mut MenuController,
    lcd: &mut St7567,
    keys: &NavKeys,
    storage: &mut RadioStorage,
    trims: &mut TrimController,
    buzzer: &mut Buzzer,
) {
    if keys.cancel {
        ctrl.return_to_main_menu();
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

        // Update RF driver receiver ID and receiver settings
        rf::set_rx_id(storage.models[new_idx].rx_id);
        rf::set_rx_settings(
            storage.models[new_idx].servo_rate_hz,
            storage.models[new_idx].rx_out_mode,
            storage.models[new_idx].rx_serial_proto,
        );

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
        let line_str = ascii_as_str(&line_buf);

        widgets::draw_list_row(lcd, slot, is_cursor, line_str, None, 0);
    }

    widgets::draw_footer(lcd, "[OK] Load     [ESC] Back");
}

#[inline(never)]
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
            ctrl.return_to_main_menu();
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
                    let next_type = storage.models[active_idx].model_type().next();
                    storage.models[active_idx].set_model_type(next_type);
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
                    storage.models[active_idx].arm_switch = (storage.models[active_idx].arm_switch + 1) % 15;
                    buzzer.play_tone(2200, 20);
                } else if keys.down {
                    storage.models[active_idx].arm_switch = if storage.models[active_idx].arm_switch == 0 {
                        14
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
                    storage.models[active_idx].timer_source = sw + 3; // 1..14 maps to 4..17
                    buzzer.play_tone(2400, 40);
                } else if keys.up {
                    storage.models[active_idx].timer_source = (storage.models[active_idx].timer_source + 1) % 18;
                    buzzer.play_tone(2200, 20);
                } else if keys.down {
                    storage.models[active_idx].timer_source = if storage.models[active_idx].timer_source == 0 {
                        17
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

    let mut t_buf = [0u8; 8];

    for slot in 0..4 {
        let idx = ctrl.scroll_offset + slot;
        if idx >= FIELD_COUNT {
            break;
        }
        let y = 14 + (slot as i32 * 9);
        let is_sel = !ctrl.editing && idx == ctrl.selected_item;
        let is_edit = ctrl.editing && idx == ctrl.selected_item;
        let inverted = is_sel || is_edit;
        if inverted {
            lcd.fill_rect(2, y, 124, 9, true);
        }

        match idx {
            0 => {
                lcd.draw_str_6x10(4, y, "Name:", inverted);
                let name_str = ascii_as_str(&storage.models[active_idx].name);
                lcd.draw_str_6x10(40, y, name_str, inverted);

                if ctrl.editing && ctrl.selected_item == 0 {
                    let char_x = 40 + (ctrl.sub_idx as i32 * 6);
                    lcd.fill_rect(char_x - 1, y, 8, 9, true);
                    let single_char = [storage.models[active_idx].name[ctrl.sub_idx]];
                    let c_str = ascii_as_str(&single_char);
                    lcd.draw_str_6x10(char_x, y, c_str, true);
                }
            }
            1 => {
                let type_str = storage.models[active_idx].model_type().as_str();
                lcd.draw_str_6x10(4, y, "Type:", inverted);
                lcd.draw_str_6x10(40, y, type_str, inverted);
            }
            2 => {
                let arm_idx = (storage.models[active_idx].arm_switch as usize).min(14);
                let arm_str = if arm_idx == 0 { "NONE" } else { crate::ui::format::SWITCH_COND_NAMES[arm_idx] };
                lcd.draw_str_6x10(4, y, "Arm Sw:", inverted);
                lcd.draw_str_6x10(52, y, arm_str, inverted);
            }
            3 => {
                let t_secs = storage.models[active_idx].timer_secs;
                let t_str = if t_secs == 0 { "OFF" } else { format_timer(t_secs, false, &mut t_buf) };
                lcd.draw_str_6x10(4, y, "Timer:", inverted);
                lcd.draw_str_6x10(52, y, t_str, inverted);
            }
            4 => {
                let trig_str = match storage.models[active_idx].timer_source {
                    0 => "OFF",
                    1 => "THs (RUN)",
                    2 => "THt (LTCH)",
                    3 => "ALWAYS ON",
                    s if (4..=17).contains(&s) => crate::ui::format::SWITCH_COND_NAMES[(s - 3) as usize],
                    _ => "OFF",
                };
                lcd.draw_str_6x10(4, y, "T-Trig:", inverted);
                lcd.draw_str_6x10(52, y, trig_str, inverted);
            }
            5 => {
                if ctrl.editing && ctrl.selected_item == 5 {
                    let mut cp_buf = *b"Copy -> M00";
                    let target_num = (ctrl.sub_idx + 1) as u8;
                    cp_buf[9] = b'0' + (target_num / 10);
                    cp_buf[10] = b'0' + (target_num % 10);
                    let cp_str = ascii_as_str(&cp_buf);
                    lcd.draw_str_6x10(4, y, cp_str, inverted);
                } else {
                    lcd.draw_str_6x10(4, y, "Copy: [OK Duplicate]", inverted);
                }
            }
            6 => {
                let mut rx_buf = [b'0'; 8];
                u32_to_hex(storage.models[active_idx].rx_id, &mut rx_buf);
                let rx_hex_str = ascii_as_str(&rx_buf);
                lcd.draw_str_6x10(4, y, "Rx:", inverted);
                lcd.draw_str_6x10(24, y, rx_hex_str, inverted);
                lcd.draw_str_6x10(74, y, "[OK Bind]", inverted);
            }
            7 => {
                lcd.draw_str_6x10(4, y, "Reset: [OK Defaults]", inverted);
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
