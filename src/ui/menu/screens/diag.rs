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
    buzzer: &mut Buzzer,
) {
    if keys.cancel {
        ctrl.return_to_main_menu();
        buzzer.click();
        return;
    }

    if keys.up || keys.down {
        ctrl.page_idx = if ctrl.page_idx == 0 { 1 } else { 0 };
        buzzer.play_tone(2200, 30);
    }

    let title = if ctrl.page_idx == 0 { "ANALOG (1-6)" } else { "ANALOG (7-11)" };
    widgets::draw_header(lcd, title);

    let text_style_small = MonoTextStyle::new(&FONT_4X6, BinaryColor::On);

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
