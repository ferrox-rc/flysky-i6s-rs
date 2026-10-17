//! Radio settings and Protocol configuration screens.

use crate::buzzer::Buzzer;
use crate::display::St7567;
use crate::menu::format::{
    ascii_as_str, format_deci_volt, format_pct_3, format_servo_hz, format_u8_2, u32_to_hex,
};
use crate::menu::widgets;
use crate::menu::{MenuController, MenuState, NavKeys};
use crate::storage::{self, RadioStorage};
use crate::trim::TrimController;

#[inline(never)]
pub fn update_radio_setup(
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
        crate::hw::SETUP_ITEMS,
        keys.up,
        keys.down,
        buzzer,
    );

    if keys.ok {
        match ctrl.selected_item {
            0 => {
                buzzer.click();
                storage.radio.throttle_trim = (storage.radio.throttle_trim + 1) % 3;
                trims.throttle_enabled = storage.radio.throttle_trim != 0;
                storage::save_radio_config(storage);
            }
            1 => {
                storage.radio.audio_enabled = if storage.radio.audio_enabled == 0 {
                    1
                } else {
                    0
                };
                buzzer.enabled = storage.radio.audio_enabled != 0;
                if buzzer.enabled {
                    buzzer.click();
                }
                storage::save_radio_config(storage);
            }
            2 => {
                // Toggle Tone Style: 0=Simple, 1=Rich
                storage.radio.tone_style = if storage.radio.tone_style == 0 { 1 } else { 0 };
                buzzer.tone_style = crate::buzzer::ToneStyle::from_u8(storage.radio.tone_style);
                storage::save_radio_config(storage);
                if buzzer.tone_style == crate::buzzer::ToneStyle::Rich {
                    buzzer.chime_armed();
                } else {
                    buzzer.click();
                }
            }
            3 => {
                buzzer.click();
                storage.radio.backlight_timeout = (storage.radio.backlight_timeout + 1) % 4;
                storage::save_radio_config(storage);
            }
            4 => {
                buzzer.click();
                storage.radio.backlight_brightness = if storage.radio.backlight_brightness >= 10 {
                    1
                } else {
                    storage.radio.backlight_brightness + 1
                };
                lcd.set_backlight_level(storage.radio.backlight_brightness * 10);
                storage::save_radio_config(storage);
            }
            5 => {
                buzzer.click();
                storage.radio.lcd_contrast = if storage.radio.lcd_contrast >= 50 {
                    20
                } else {
                    (storage.radio.lcd_contrast + 3).min(50)
                };
                lcd.set_contrast(storage.radio.lcd_contrast);
                storage::save_radio_config(storage);
            }
            6 => {
                buzzer.click();
                storage.radio.vbat_warn_deci = if storage.radio.vbat_warn_deci >= 50 {
                    40
                } else {
                    storage.radio.vbat_warn_deci + 1
                };
                storage::save_radio_config(storage);
            }
            7 => {
                buzzer.click();
                storage.radio.usb_mode = (storage.radio.usb_mode + 1) % 4;
                storage::save_radio_config(storage);
                crate::usb::request_init(storage.radio.usb_mode);
            }
            idx => {
                crate::hw::handle_board_setup_ok(idx, ctrl, storage, trims, buzzer);
                if ctrl.state != MenuState::RadioSetup {
                    return;
                }
            }
        }
    }

    widgets::draw_header(lcd, "RADIO SETUP");

    for slot in 0..4 {
        let idx = ctrl.scroll_offset + slot;
        if idx >= crate::hw::SETUP_ITEMS {
            break;
        }
        let is_sel = idx == ctrl.selected_item;

        match idx {
            0 => {
                let val_str = match storage.radio.throttle_trim {
                    1 => "IDLE",
                    2 => "LINEAR",
                    _ => "OFF (Lock)",
                };
                widgets::draw_list_row_right(lcd, slot, is_sel, "Thr Trim:", Some(val_str));
            }
            1 => {
                let beeper_str = if storage.radio.audio_enabled != 0 {
                    "ENABLED"
                } else {
                    "MUTED"
                };
                widgets::draw_list_row_right(lcd, slot, is_sel, "Beeper:", Some(beeper_str));
            }
            2 => {
                let tone_str = if storage.radio.tone_style != 0 {
                    "RICH"
                } else {
                    "SIMPLE"
                };
                widgets::draw_list_row_right(lcd, slot, is_sel, "Tones:", Some(tone_str));
            }
            3 => {
                let timer_str = match storage.radio.backlight_timeout {
                    1 => "15 SEC",
                    2 => "30 SEC",
                    3 => "60 SEC",
                    _ => "ALWAYS ON",
                };
                widgets::draw_list_row_right(lcd, slot, is_sel, "BL Timer:", Some(timer_str));
            }
            4 => {
                let mut b_buf = [0u8; 4];
                let pct = (storage.radio.backlight_brightness * 10).min(100);
                let b_str = format_pct_3(pct, &mut b_buf);
                widgets::draw_list_row_right(lcd, slot, is_sel, "BL Level:", Some(b_str));
            }
            5 => {
                let mut c_buf = [0u8; 2];
                let c_str = format_u8_2(storage.radio.lcd_contrast, &mut c_buf);
                widgets::draw_list_row_right(lcd, slot, is_sel, "Contrast:", Some(c_str));
            }
            6 => {
                let mut v_buf = [0u8; 4];
                let v_str = format_deci_volt(storage.radio.vbat_warn_deci, &mut v_buf);
                widgets::draw_list_row_right(lcd, slot, is_sel, "Bat Warn:", Some(v_str));
            }
            7 => {
                let usb_str = match storage.radio.usb_mode {
                    1 => "JOYSTICK",
                    2 => "SERIAL",
                    3 => "COMPOSITE",
                    _ => "OFF",
                };
                widgets::draw_list_row_right(lcd, slot, is_sel, "USB Mode:", Some(usb_str));
            }
            idx => {
                crate::hw::draw_board_setup_row(lcd, slot, idx, is_sel, storage);
            }
        }
    }

    widgets::draw_footer(lcd, "[OK] Toggle/Cycle   [ESC] Back");
}

const MODE_STRS: [&str; 7] = [
    "DEFAULT",
    "2-POS",
    "3-POS",
    "6-POS",
    "POT",
    "POT-D",
    "INST-TRIM",
];

#[inline(never)]
pub fn update_input_setup(
    ctrl: &mut MenuController,
    lcd: &mut St7567,
    keys: &NavKeys,
    storage: &mut RadioStorage,
    buzzer: &mut Buzzer,
) {
    if keys.cancel {
        ctrl.state = MenuState::RadioSetup;
        ctrl.selected_item = crate::hw::INPUT_SETUP_RETURN.0;
        ctrl.scroll_offset = crate::hw::INPUT_SETUP_RETURN.1;
        buzzer.click();
        return;
    }

    let input_items = crate::hw::input_setup_items(storage);

    widgets::navigate_4slot_list(
        &mut ctrl.selected_item,
        &mut ctrl.scroll_offset,
        input_items,
        keys.up,
        keys.down,
        buzzer,
    );

    if keys.ok {
        let ch = ctrl.selected_item;
        if ch < input_items {
            buzzer.click();
            let current = storage.radio.get_adc_mode_val(ch);
            storage
                .radio
                .set_adc_mode_val(ch, (current + 1) % (MODE_STRS.len() as u8));
            crate::input::apply_calibration(&storage.radio);
            storage::save_radio_config(storage);
        }
    }

    widgets::draw_header(lcd, "INPUT MODES");

    for slot in 0..4 {
        let idx = ctrl.scroll_offset + slot;
        if idx >= input_items {
            break;
        }
        let is_sel = idx == ctrl.selected_item;
        let mode_idx = (storage.radio.get_adc_mode_val(idx) as usize).min(MODE_STRS.len() - 1);
        let mode_str = MODE_STRS[mode_idx];
        widgets::draw_list_row_right(lcd, slot, is_sel, crate::hw::AUX_NAMES[idx], Some(mode_str));
    }

    widgets::draw_footer(lcd, "[OK] Cycle Mode    [ESC] Back");
}

pub const TELEM_SENSOR_NAMES: [&str; 13] = [
    "RX BAT",
    "LQ/RSSI",
    "TX BAT",
    "CURRENT",
    "CAP mAh",
    "TX PWR",
    "SNR",
    "PKT RATE",
    "ANTENNA",
    "PKTS TX",
    "PKTS RX",
    "TIMER",
    "OFF",
];

pub const SLOT_NAMES: [&str; 4] = [
    "Top-L:",
    "Top-R:",
    "Btm-L:",
    "Btm-R:",
];

#[inline(never)]
pub fn update_telem_setup(
    ctrl: &mut MenuController,
    lcd: &mut St7567,
    keys: &NavKeys,
    storage: &mut RadioStorage,
    buzzer: &mut Buzzer,
) {
    if keys.cancel {
        ctrl.state = MenuState::RadioSetup;
        ctrl.selected_item = crate::hw::TELEM_SETUP_RETURN.0;
        ctrl.scroll_offset = crate::hw::TELEM_SETUP_RETURN.1;
        buzzer.click();
        return;
    }

    const SLOT_COUNT: usize = 4;

    widgets::navigate_4slot_list(
        &mut ctrl.selected_item,
        &mut ctrl.scroll_offset,
        SLOT_COUNT,
        keys.up,
        keys.down,
        buzzer,
    );

    if keys.ok {
        let idx = ctrl.selected_item;
        if idx < SLOT_COUNT {
            buzzer.click();
            let current = storage.radio.telem_slots[idx];
            storage.radio.telem_slots[idx] = (current + 1) % (TELEM_SENSOR_NAMES.len() as u8);
            storage::save_radio_config(storage);
        }
    }

    widgets::draw_header(lcd, "TELEM DASH SETUP");

    for slot in 0..4 {
        let idx = ctrl.scroll_offset + slot;
        if idx >= SLOT_COUNT {
            break;
        }
        let is_sel = idx == ctrl.selected_item;
        let sensor_idx = (storage.radio.telem_slots[idx] as usize).min(TELEM_SENSOR_NAMES.len() - 1);
        widgets::draw_list_row_right(
            lcd,
            slot,
            is_sel,
            SLOT_NAMES[idx],
            Some(TELEM_SENSOR_NAMES[sensor_idx]),
        );
    }

    widgets::draw_footer(lcd, "[OK] Cycle Sensor  [ESC] Back");
}

#[inline(never)]
pub fn update_rx_setup(
    ctrl: &mut MenuController,
    lcd: &mut St7567,
    keys: &NavKeys,
    storage: &mut RadioStorage,
    buzzer: &mut Buzzer,
) {
    let active_idx = storage.radio.active_model as usize;
    let proto = storage.models[active_idx].rf_protocol;
    // 0: AFHDS 2A -> 6 items (0: Proto, 1: [Bind Receiver], 2: RF Power, 3: Servo Hz, 4: RX Out, 5: Serial)
    // 1: CRSF -> 4 items (0: Proto, 1: Baud, 2: Duplex, 3: [Configure Module])
    let item_count = if proto == 0 { 6 } else { 4 };

    if keys.cancel {
        if ctrl.editing {
            ctrl.editing = false;
            buzzer.click();
        } else {
            ctrl.return_to_main_menu();
            buzzer.click();
            return;
        }
    }

    if !ctrl.editing {
        // Navigation Phase: UP/DOWN moves selection
        widgets::navigate_4slot_list(
            &mut ctrl.selected_item,
            &mut ctrl.scroll_offset,
            item_count,
            keys.up,
            keys.down,
            buzzer,
        );

        if keys.ok {
            match (proto, ctrl.selected_item) {
                (_, 0) => {
                    // Enter edit mode for Protocol
                    ctrl.editing = true;
                    buzzer.click();
                }
                (0, 1) => {
                    // AFHDS 2A: Trigger Bind
                    ctrl.request_bind = true;
                    ctrl.state = MenuState::Closed;
                    buzzer.click();
                    return;
                }
                (1, 1) => {
                    // CRSF: Enter edit mode for Baud Rate
                    ctrl.editing = true;
                    buzzer.click();
                }
                (1, 2) => {
                    // CRSF: Toggle Duplex mode (Full / Half)
                    buzzer.click();
                    storage.models[active_idx].crsf_half_duplex =
                        if storage.models[active_idx].crsf_half_duplex == 0 {
                            1
                        } else {
                            0
                        };
                    storage::save_storage(storage);
                }
                (1, 3) => {
                    // CRSF: Enter Configurator
                    ctrl.state = MenuState::CrsfSetup;
                    ctrl.return_state = MenuState::RxSetup;
                    ctrl.selected_item = 0;
                    ctrl.scroll_offset = 0;
                    ctrl.waiting_release = true;
                    crate::crsf::start_config();
                    buzzer.click();
                    return;
                }
                (0, 2) => {
                    // AFHDS 2A: Cycle RF Power (0: 100mW, 1: 10mW, 2: 1mW, 3: 100uW)
                    buzzer.click();
                    storage.models[active_idx].rf_power =
                        (storage.models[active_idx].rf_power + 1) % 4;
                    crate::rf::set_rf_power(storage.models[active_idx].rf_power);
                    storage::save_storage(storage);
                }
                (0, 3) => {
                    // AFHDS 2A: Cycle Servo Hz
                    buzzer.click();
                    storage.models[active_idx].servo_rate_hz =
                        match storage.models[active_idx].servo_rate_hz {
                            50 => 60,
                            60 => 100,
                            100 => 150,
                            150 => 200,
                            200 => 250,
                            250 => 300,
                            300 => 350,
                            350 => 400,
                            _ => 50,
                        };
                    crate::rf::set_rx_settings(
                        storage.models[active_idx].servo_rate_hz,
                        storage.models[active_idx].rx_out_mode,
                        storage.models[active_idx].rx_serial_proto,
                    );
                    storage::save_storage(storage);
                }
                (0, 4) => {
                    // AFHDS 2A: Toggle RX Out (PWM/PPM)
                    buzzer.click();
                    storage.models[active_idx].rx_out_mode =
                        if storage.models[active_idx].rx_out_mode == 0 {
                            1
                        } else {
                            0
                        };
                    crate::rf::set_rx_settings(
                        storage.models[active_idx].servo_rate_hz,
                        storage.models[active_idx].rx_out_mode,
                        storage.models[active_idx].rx_serial_proto,
                    );
                    storage::save_storage(storage);
                }
                (0, 5) => {
                    // AFHDS 2A: Toggle Serial Proto (i-BUS/S.BUS)
                    buzzer.click();
                    storage.models[active_idx].rx_serial_proto =
                        if storage.models[active_idx].rx_serial_proto == 0 {
                            1
                        } else {
                            0
                        };
                    crate::rf::set_rx_settings(
                        storage.models[active_idx].servo_rate_hz,
                        storage.models[active_idx].rx_out_mode,
                        storage.models[active_idx].rx_serial_proto,
                    );
                    storage::save_storage(storage);
                }
                _ => {}
            }
        }
    } else {
        // Edit Phase: UP/DOWN modifies the selected parameter
        match ctrl.selected_item {
            0 => {
                // Protocol: 0 = AFHDS 2A, 1 = CRSF
                if keys.up || keys.down {
                    let new_proto = if proto == 0 { 1 } else { 0 };
                    storage.models[active_idx].rf_protocol = new_proto;
                    buzzer.play_tone(2200, 30);
                }
            }
            1 => {
                // Baud Rate (CRSF only): 0 = 115.2k, 1 = 416.6k, 2 = 420k, 3 = 921.6k, 4 = 1.875M
                if keys.up {
                    storage.models[active_idx].crsf_baud =
                        (storage.models[active_idx].crsf_baud + 1) % 5;
                    buzzer.play_tone(2200, 30);
                } else if keys.down {
                    storage.models[active_idx].crsf_baud =
                        if storage.models[active_idx].crsf_baud > 0 {
                            storage.models[active_idx].crsf_baud - 1
                        } else {
                            4
                        };
                    buzzer.play_tone(2200, 30);
                }
            }
            _ => {}
        }

        // OK saves selection to Flash and returns to navigation phase
        if keys.ok {
            ctrl.editing = false;
            storage::save_storage(storage);
            buzzer.click();
        }
    }

    widgets::draw_header(lcd, "PROTOCOL SETUP");

    let current_proto = storage.models[active_idx].rf_protocol;

    if current_proto == 0 {
        // AFHDS 2A Display (4-slot scrollable list)
        for slot in 0..4 {
            let idx = ctrl.scroll_offset + slot;
            if idx >= item_count {
                break;
            }
            let is_sel = idx == ctrl.selected_item;

            match idx {
                0 => {
                    let val_str = if ctrl.editing && is_sel {
                        "[AFHDS 2A]"
                    } else {
                        "AFHDS 2A"
                    };
                    widgets::draw_list_row_right(lcd, slot, is_sel, "Proto:", Some(val_str));
                }
                1 => {
                    let mut rx_buf = [b'0'; 8];
                    u32_to_hex(storage.models[active_idx].rx_id, &mut rx_buf);
                    let rx_str = ascii_as_str(&rx_buf);
                    let mut bind_buf = [b' '; 18];
                    bind_buf[0..7].copy_from_slice(b"[Bind: ");
                    bind_buf[7..15].copy_from_slice(rx_str.as_bytes());
                    bind_buf[15] = b']';
                    let bind_str = ascii_as_str(&bind_buf[..16]);
                    widgets::draw_list_row_right(lcd, slot, is_sel, bind_str, None);
                }
                2 => {
                    let pwr_str = match storage.models[active_idx].rf_power {
                        0 => "100mW (HI)",
                        1 => "10mW (MID)",
                        2 => "1mW (LOW)",
                        3 => "100uW (TST)",
                        _ => "100mW (HI)",
                    };
                    widgets::draw_list_row_right(lcd, slot, is_sel, "Power:", Some(pwr_str));
                }
                3 => {
                    let mut hz_buf = [0u8; 8];
                    let hz_str =
                        format_servo_hz(storage.models[active_idx].servo_rate_hz, &mut hz_buf);
                    widgets::draw_list_row_right(lcd, slot, is_sel, "Servo Hz:", Some(hz_str));
                }
                4 => {
                    let out_str = if storage.models[active_idx].rx_out_mode == 0 {
                        "PWM"
                    } else {
                        "PPM"
                    };
                    widgets::draw_list_row_right(lcd, slot, is_sel, "RX Out:", Some(out_str));
                }
                5 => {
                    let serial_str = if storage.models[active_idx].rx_serial_proto == 0 {
                        "i-BUS"
                    } else {
                        "S.BUS"
                    };
                    widgets::draw_list_row_right(lcd, slot, is_sel, "Serial:", Some(serial_str));
                }
                _ => {}
            }
        }

        let footer = if ctrl.editing {
            "[OK] Save   [UP/DN] Change"
        } else if ctrl.selected_item == 1 {
            "[OK] Start Bind   [ESC] Exit"
        } else if ctrl.selected_item >= 2 {
            "[OK] Toggle/Cycle [ESC] Exit"
        } else {
            "[OK] Edit   [ESC] Exit"
        };
        widgets::draw_footer(lcd, footer);
    } else {
        // CRSF Display (4 items: Proto, Baud, Duplex, Configure Module)
        let sel_proto = ctrl.selected_item == 0;
        let sel_baud = ctrl.selected_item == 1;
        let sel_duplex = ctrl.selected_item == 2;
        let sel_cfg = ctrl.selected_item == 3;

        let proto_val = if ctrl.editing && sel_proto {
            "[CRSF]"
        } else {
            "CRSF"
        };
        widgets::draw_list_row_right(lcd, 0, sel_proto, "Proto:", Some(proto_val));

        let baud_val = match (
            storage.models[active_idx].crsf_baud,
            ctrl.editing && sel_baud,
        ) {
            (0, false) => "115.2k (Low)",
            (0, true) => "[115.2k (Low)]",
            (1, false) => "416.6k (TBS)",
            (1, true) => "[416.6k (TBS)]",
            (2, false) => "420k (ELRS)",
            (2, true) => "[420k (ELRS)]",
            (3, false) => "921.6k (Fast)",
            (3, true) => "[921.6k (Fast)]",
            (4, false) => "1.875M (Max)",
            (4, true) => "[1.875M (Max)]",
            _ => "420k (ELRS)",
        };
        widgets::draw_list_row_right(lcd, 1, sel_baud, "Baud:", Some(baud_val));

        let duplex_val = if storage.models[active_idx].crsf_half_duplex == 0 {
            "Full (2W)"
        } else {
            "Half (1W)"
        };
        widgets::draw_list_row_right(lcd, 2, sel_duplex, "Duplex:", Some(duplex_val));

        widgets::draw_list_row_right(lcd, 3, sel_cfg, "[Configure Module]", None);

        let footer = if ctrl.editing {
            "[OK] Save   [UP/DN] Change"
        } else if sel_duplex {
            "[OK] Toggle Mode  [ESC] Exit"
        } else if sel_cfg {
            "[OK] Open Config  [ESC] Exit"
        } else {
            "[OK] Edit   [ESC] Exit"
        };
        widgets::draw_footer(lcd, footer);
    }
}
