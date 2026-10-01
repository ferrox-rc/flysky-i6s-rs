//! Analog diagnostics and System Information screens.

use embedded_graphics::{
    mono_font::{ascii::FONT_4X6, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::Rectangle,
    text::Text,
};

use crate::adc;
use crate::buzzer::Buzzer;
use crate::chip;
use crate::display::St7567;
use crate::menu::format::{ascii_as_str, u16_to_dec_4};
use crate::menu::widgets;
use crate::menu::{MenuController, NavKeys};

#[inline(never)]
pub fn update_diag_anas(
    ctrl: &mut MenuController,
    lcd: &mut St7567,
    keys: &NavKeys,
    raw_adc: &[u16; adc::NUM_CHANNELS],
    switches: &crate::input::Switches,
    buzzer: &mut Buzzer,
) {
    if keys.cancel {
        ctrl.return_to_main_menu();
        buzzer.click();
        return;
    }

    if keys.up {
        ctrl.page_idx = if ctrl.page_idx == 0 { 3 } else { ctrl.page_idx - 1 };
        buzzer.play_tone(2200, 30);
    } else if keys.down {
        ctrl.page_idx = (ctrl.page_idx + 1) % 4;
        buzzer.play_tone(2200, 30);
    }

    let title = match ctrl.page_idx {
        0 => "ANALOG (1-6)",
        1 => "ANALOG (7-11)",
        2 => "EXT SWITCHES (SE/SF)",
        _ => "EXT ANALOG (P7)",
    };
    widgets::draw_header(lcd, title);

    let text_style_small = MonoTextStyle::new(&FONT_4X6, BinaryColor::On);

    if ctrl.page_idx < 2 {
        let start_idx = if ctrl.page_idx == 0 { 0 } else { 6 };
        let names: &[&str] = if ctrl.page_idx == 0 {
            &["RH:AIL", "RV:ELE", "LV:THR", "LH:RUD", "SW:SA ", "SW:SB "]
        } else {
            &["POT:V1", "POT:V2", "SW:SC ", "SW:SD ", "VBAT  "]
        };

        for (i, &name) in names.iter().enumerate() {
            let adc_idx = start_idx + i;
            let y = 12 + (i as i32 * 7);
            Text::new(name, Point::new(2, y + 5), text_style_small).draw(lcd).ok();

            let raw = raw_adc[adc_idx].min(4095);
            let fill_w = ((raw as u32 * 38) / 4095).min(38);
            widgets::draw_bar_gauge(lcd, Rectangle::new(Point::new(44, y + 1), Size::new(40, 5)), fill_w);

            let mut val_buf = [0u8; 4];
            u16_to_dec_4(raw, &mut val_buf);
            let val_str = ascii_as_str(&val_buf);
            Text::new(val_str, Point::new(90, y + 5), text_style_small).draw(lcd).ok();
        }
    } else if ctrl.page_idx == 2 {
        // Page 2: SE and SF Switch Diagnostics
        let ext_active = crate::input::is_ext_switches_enabled();

        let cfg_label = if ext_active {
            "EXT SW: ENABLED (PC12/15)"
        } else {
            "EXT SW: DISABLED (OFF)"
        };
        Text::new(cfg_label, Point::new(2, 18), text_style_small).draw(lcd).ok();

        // SE (PC12)
        Text::new("SW:SE", Point::new(2, 28), text_style_small).draw(lcd).ok();
        let se_state = if switches.se == crate::input::SwitchPos::Down {
            "DN (LOW)"
        } else {
            "UP (HIGH)"
        };
        let fill_se = if switches.se == crate::input::SwitchPos::Down { 30 } else { 0 };
        widgets::draw_bar_gauge(lcd, Rectangle::new(Point::new(32, 23), Size::new(30, 6)), fill_se);
        Text::new(se_state, Point::new(66, 28), text_style_small).draw(lcd).ok();

        // SF (PC15)
        Text::new("SW:SF", Point::new(2, 38), text_style_small).draw(lcd).ok();
        let sf_state = if switches.sf == crate::input::SwitchPos::Down {
            "DN (LOW)"
        } else {
            "UP (HIGH)"
        };
        let fill_sf = if switches.sf == crate::input::SwitchPos::Down { 30 } else { 0 };
        widgets::draw_bar_gauge(lcd, Rectangle::new(Point::new(32, 33), Size::new(30, 6)), fill_sf);
        Text::new(sf_state, Point::new(66, 38), text_style_small).draw(lcd).ok();

        Text::new("Active-LOW mod to GND", Point::new(2, 49), text_style_small).draw(lcd).ok();
    } else {
        // Page 3: P7 Header AD12..AD15 Analog Diagnostics
        let ext_active = crate::input::is_ext_adc_enabled();

        let cfg_label = if ext_active {
            "P7 ADC: ENABLED (15-CH)"
        } else {
            "P7 ADC: DISABLED (OFF)"
        };
        Text::new(cfg_label, Point::new(2, 18), text_style_small).draw(lcd).ok();

        let names: [&str; 4] = ["VRC:AD12", "VRD:AD13", "VRE:AD14", "VRF:AD15"];
        for (i, &name) in names.iter().enumerate() {
            let adc_idx = 11 + i;
            let y = 25 + (i as i32 * 7);
            Text::new(name, Point::new(2, y + 5), text_style_small).draw(lcd).ok();

            let raw = raw_adc[adc_idx].min(4095);
            let fill_w = if ext_active {
                ((raw as u32 * 38) / 4095).min(38)
            } else {
                0
            };
            widgets::draw_bar_gauge(lcd, Rectangle::new(Point::new(44, y + 1), Size::new(40, 5)), fill_w);

            let mut val_buf = [0u8; 4];
            if ext_active {
                u16_to_dec_4(raw, &mut val_buf);
            } else {
                val_buf = *b" OFF";
            }
            let val_str = ascii_as_str(&val_buf);
            Text::new(val_str, Point::new(90, y + 5), text_style_small).draw(lcd).ok();
        }
    }

    widgets::draw_footer(lcd, "[UP/DN] Page  [ESC] Back");
}

#[inline(never)]
pub fn update_system_info(
    ctrl: &mut MenuController,
    lcd: &mut St7567,
    keys: &NavKeys,
    buzzer: &mut Buzzer,
) {
    if keys.cancel {
        ctrl.return_to_main_menu();
        buzzer.click();
        return;
    }

    let profile = chip::get_mcu_profile();

    widgets::draw_header(lcd, "SYSTEM INFORMATION");

    let text_style = MonoTextStyle::new(&FONT_4X6, BinaryColor::On);

    Text::new("MCU:      ", Point::new(4, 18), text_style).draw(lcd).ok();
    Text::new(profile.name, Point::new(48, 18), text_style).draw(lcd).ok();

    Text::new("Firmware: ", Point::new(4, 25), text_style).draw(lcd).ok();
    Text::new(
        concat!(env!("FIRMWARE_VERSION"), " (", env!("GIT_HASH"), ")"),
        Point::new(48, 25),
        text_style,
    )
    .draw(lcd)
    .ok();

    Text::new("Flash:    128KB (64 Pages)", Point::new(4, 32), text_style).draw(lcd).ok();
    Text::new("SRAM:     16KB (Parity)", Point::new(4, 39), text_style).draw(lcd).ok();
    Text::new("Profiles: 20 Models", Point::new(4, 46), text_style).draw(lcd).ok();

    widgets::draw_footer(lcd, "[ESC] Back");
}
