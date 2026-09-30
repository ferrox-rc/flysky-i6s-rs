//! Radio settings and Protocol configuration screens.

use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    text::Text,
};

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
        ctrl.state = MenuState::MainMenu;
        ctrl.selected_item = 7;
        ctrl.scroll_offset = 4;
        ctrl.waiting_release = true;
        buzzer.click();
        return;
    }
    const SETUP_ITEMS: usize = 12;

    widgets::navigate_4slot_list(
        &mut ctrl.selected_item,
        &mut ctrl.scroll_offset,
        SETUP_ITEMS,
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
                storage.radio.audio_enabled = if storage.radio.audio_enabled == 0 { 1 } else { 0 };
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
                crate::usb::init(storage.radio.usb_mode);
            }
            8 => {
                buzzer.click();
                storage.radio.ext_module_pwr = if storage.radio.ext_module_pwr == 0 { 1 } else { 0 };
                crate::crsf::set_power_polarity(storage.radio.ext_module_pwr == 0);
                storage::save_radio_config(storage);
            }
            9 => {
                buzzer.click();
                storage.radio.servo_rate_hz = match storage.radio.servo_rate_hz {
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
                    storage.radio.servo_rate_hz,
                    storage.radio.rx_out_mode,
                    storage.radio.rx_serial_proto,
                );
                storage::save_radio_config(storage);
            }
            10 => {
                buzzer.click();
                storage.radio.rx_out_mode = if storage.radio.rx_out_mode == 0 { 1 } else { 0 };
                crate::rf::set_rx_settings(
                    storage.radio.servo_rate_hz,
                    storage.radio.rx_out_mode,
                    storage.radio.rx_serial_proto,
                );
                storage::save_radio_config(storage);
            }
            11 => {
                buzzer.click();
                storage.radio.rx_serial_proto = if storage.radio.rx_serial_proto == 0 { 1 } else { 0 };
                crate::rf::set_rx_settings(
                    storage.radio.servo_rate_hz,
                    storage.radio.rx_out_mode,
                    storage.radio.rx_serial_proto,
                );
                storage::save_radio_config(storage);
            }
            _ => {}
        }
    }

    widgets::draw_header(lcd, "RADIO SETUP");

    for slot in 0..4 {
        let idx = ctrl.scroll_offset + slot;
        if idx >= SETUP_ITEMS {
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
                widgets::draw_list_row(lcd, slot, is_sel, "Thr Trim:", Some(val_str), 62);
            }
            1 => {
                let beeper_str = if storage.radio.audio_enabled != 0 { "ENABLED" } else { "MUTED" };
                widgets::draw_list_row(lcd, slot, is_sel, "Beeper:", Some(beeper_str), 62);
            }
            2 => {
                let tone_str = if storage.radio.tone_style != 0 { "RICH" } else { "SIMPLE" };
                widgets::draw_list_row(lcd, slot, is_sel, "Tones:", Some(tone_str), 62);
            }
            3 => {
                let timer_str = match storage.radio.backlight_timeout {
                    1 => "15 SEC",
                    2 => "30 SEC",
                    3 => "60 SEC",
                    _ => "ALWAYS ON",
                };
                widgets::draw_list_row(lcd, slot, is_sel, "BL Timer:", Some(timer_str), 62);
            }
            4 => {
                let mut b_buf = [0u8; 4];
                let pct = (storage.radio.backlight_brightness * 10).min(100);
                let b_str = format_pct_3(pct, &mut b_buf);
                widgets::draw_list_row(lcd, slot, is_sel, "BL Level:", Some(b_str), 62);
            }
            5 => {
                let mut c_buf = [0u8; 2];
                let c_str = format_u8_2(storage.radio.lcd_contrast, &mut c_buf);
                widgets::draw_list_row(lcd, slot, is_sel, "Contrast:", Some(c_str), 62);
            }
            6 => {
                let mut v_buf = [0u8; 4];
                let v_str = format_deci_volt(storage.radio.vbat_warn_deci, &mut v_buf);
                widgets::draw_list_row(lcd, slot, is_sel, "Bat Warn:", Some(v_str), 62);
            }
            7 => {
                let usb_str = match storage.radio.usb_mode {
                    1 => "JOYSTICK",
                    2 => "SERIAL",
                    3 => "COMPOSITE",
                    _ => "OFF",
                };
                widgets::draw_list_row(lcd, slot, is_sel, "USB Mode:", Some(usb_str), 62);
            }
            8 => {
                let pwr_str = if storage.radio.ext_module_pwr == 0 { "HIGH (N)" } else { "LOW (P)" };
                widgets::draw_list_row(lcd, slot, is_sel, "PC13 Pwr:", Some(pwr_str), 62);
            }
            9 => {
                let mut hz_buf = [0u8; 8];
                let hz_str = format_servo_hz(storage.radio.servo_rate_hz, &mut hz_buf);
                widgets::draw_list_row(lcd, slot, is_sel, "Servo Hz:", Some(hz_str), 62);
            }
            10 => {
                let out_str = if storage.radio.rx_out_mode == 0 { "PWM" } else { "PPM" };
                widgets::draw_list_row(lcd, slot, is_sel, "RX Out:", Some(out_str), 62);
            }
            11 => {
                let serial_str = if storage.radio.rx_serial_proto == 0 { "i-BUS" } else { "S.BUS" };
                widgets::draw_list_row(lcd, slot, is_sel, "Serial:", Some(serial_str), 62);
            }
            _ => {}
        }
    }

    widgets::draw_footer(lcd, "[OK] Toggle/Cycle   [ESC] Back");
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
    // 0: AFHDS 2A -> 2 selectable items (0: Proto, 1: [Bind Receiver])
    // 1: CRSF / ELRS -> 3 selectable items (0: Proto, 1: Baud, 2: [Configure Module])
    let item_count = if proto == 0 { 2 } else { 3 };

    if keys.cancel {
        if ctrl.editing {
            ctrl.editing = false;
            buzzer.click();
        } else {
            ctrl.state = MenuState::MainMenu;
            ctrl.selected_item = 8;
            ctrl.scroll_offset = 5;
            ctrl.waiting_release = true;
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
            match ctrl.selected_item {
                0 => {
                    // Enter edit mode for Protocol
                    ctrl.editing = true;
                    buzzer.click();
                }
                1 => {
                    if proto == 0 {
                        // AFHDS 2A: Trigger Bind
                        ctrl.request_bind = true;
                        ctrl.state = MenuState::Closed;
                        buzzer.click();
                        return;
                    } else {
                        // CRSF: Enter edit mode for Baud Rate
                        ctrl.editing = true;
                        buzzer.click();
                    }
                }
                2 => {
                    // CRSF: Enter ELRS Configurator
                    ctrl.state = MenuState::ElrsSetup;
                    ctrl.return_state = MenuState::RxSetup;
                    ctrl.selected_item = 0;
                    ctrl.scroll_offset = 0;
                    ctrl.waiting_release = true;
                    crate::crsf::start_config();
                    buzzer.click();
                    return;
                }
                _ => {}
            }
        }
    } else {
        // Edit Phase: UP/DOWN modifies the selected parameter
        match ctrl.selected_item {
            0 => {
                // Protocol: 0 = AFHDS 2A, 1 = CRSF/ELRS
                if keys.up || keys.down {
                    let new_proto = if proto == 0 { 1 } else { 0 };
                    storage.models[active_idx].rf_protocol = new_proto;
                    buzzer.play_tone(2200, 30);
                }
            }
            1 => {
                // Baud Rate (CRSF only): 0 = 420k, 1 = 416.6k, 2 = 115.2k, 3 = 921.6k
                if keys.up {
                    storage.models[active_idx].crsf_baud = if storage.models[active_idx].crsf_baud > 0 {
                        storage.models[active_idx].crsf_baud - 1
                    } else {
                        3
                    };
                    buzzer.play_tone(2200, 30);
                } else if keys.down {
                    storage.models[active_idx].crsf_baud = (storage.models[active_idx].crsf_baud + 1) % 4;
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

    let text_style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
    let current_proto = storage.models[active_idx].rf_protocol;
    let mut line_buf = [b' '; 26];

    if current_proto == 0 {
        // AFHDS 2A Display
        let sel_proto = ctrl.selected_item == 0;
        let sel_bind = ctrl.selected_item == 1;

        let p_arrow = if sel_proto { b'>' } else { b' ' };
        let b_arrow = if sel_bind { b'>' } else { b' ' };

        let proto_val = if ctrl.editing && sel_proto { "[AFHDS 2A]" } else { "AFHDS 2A" };
        line_buf[0] = p_arrow;
        line_buf[1..8].copy_from_slice(b"Proto: ");
        let pv_bytes = proto_val.as_bytes();
        line_buf[8..8 + pv_bytes.len()].copy_from_slice(pv_bytes);
        let p_str = ascii_as_str(&line_buf[..8 + pv_bytes.len()]);
        Text::new(p_str, Point::new(2, 22), text_style).draw(lcd).ok();

        line_buf[0] = b' ';
        line_buf[1] = b'M';
        line_buf[2] = b'0' + ((active_idx + 1) / 10) as u8;
        line_buf[3] = b'0' + ((active_idx + 1) % 10) as u8;
        line_buf[4] = b':';
        line_buf[5] = b' ';
        let pre_str = ascii_as_str(&line_buf[..6]);
        Text::new(pre_str, Point::new(2, 32), text_style).draw(lcd).ok();
        let m_str = ascii_as_str(&storage.models[active_idx].name);
        Text::new(m_str, Point::new(38, 32), text_style).draw(lcd).ok();

        let mut rx_buf = [b'0'; 8];
        u32_to_hex(storage.models[active_idx].rx_id, &mut rx_buf);
        let rx_str = ascii_as_str(&rx_buf);
        line_buf[0] = b_arrow;
        line_buf[1..8].copy_from_slice(b"[Bind: ");
        line_buf[8..16].copy_from_slice(rx_str.as_bytes());
        line_buf[16] = b']';
        let b_str = ascii_as_str(&line_buf[..17]);
        Text::new(b_str, Point::new(2, 42), text_style).draw(lcd).ok();

        let footer = if ctrl.editing {
            "[OK] Save   [UP/DN] Change"
        } else if sel_bind {
            "[OK] Start Bind   [ESC] Exit"
        } else {
            "[OK] Edit   [ESC] Exit"
        };
        widgets::draw_footer(lcd, footer);
    } else {
        // CRSF / ELRS Display
        let sel_proto = ctrl.selected_item == 0;
        let sel_baud = ctrl.selected_item == 1;
        let sel_cfg = ctrl.selected_item == 2;

        let p_arrow = if sel_proto { b'>' } else { b' ' };
        let b_arrow = if sel_baud { b'>' } else { b' ' };
        let c_arrow = if sel_cfg { b'>' } else { b' ' };

        let proto_val = if ctrl.editing && sel_proto { "[CRSF/ELRS]" } else { "CRSF/ELRS" };
        line_buf[0] = p_arrow;
        line_buf[1..8].copy_from_slice(b"Proto: ");
        let pv_bytes = proto_val.as_bytes();
        line_buf[8..8 + pv_bytes.len()].copy_from_slice(pv_bytes);
        let p_str = ascii_as_str(&line_buf[..8 + pv_bytes.len()]);
        Text::new(p_str, Point::new(2, 22), text_style).draw(lcd).ok();

        let baud_val = match storage.models[active_idx].crsf_baud {
            0 => "420k (ELRS)",
            1 => "416.6k (TBS)",
            2 => "115.2k (Low)",
            3 => "921.6k (Fast)",
            _ => "420k (ELRS)",
        };
        line_buf[0] = b_arrow;
        line_buf[1..7].copy_from_slice(b"Baud: ");
        let bv_bytes = baud_val.as_bytes();
        let b_start = if ctrl.editing && sel_baud {
            line_buf[7] = b'[';
            line_buf[8..8 + bv_bytes.len()].copy_from_slice(bv_bytes);
            line_buf[8 + bv_bytes.len()] = b']';
            9 + bv_bytes.len()
        } else {
            line_buf[7..7 + bv_bytes.len()].copy_from_slice(bv_bytes);
            7 + bv_bytes.len()
        };
        let full_b_str = ascii_as_str(&line_buf[..b_start]);
        Text::new(full_b_str, Point::new(2, 32), text_style).draw(lcd).ok();

        line_buf[0] = c_arrow;
        line_buf[1..19].copy_from_slice(b"[Configure Module]");
        let full_c_str = ascii_as_str(&line_buf[..19]);
        Text::new(full_c_str, Point::new(2, 42), text_style).draw(lcd).ok();

        let footer = if ctrl.editing {
            "[OK] Save   [UP/DN] Change"
        } else if sel_cfg {
            "[OK] Open Config  [ESC] Exit"
        } else {
            "[OK] Edit   [ESC] Exit"
        };
        widgets::draw_footer(lcd, footer);
    }
}
