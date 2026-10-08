//! Analog diagnostics and System Information screens.

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

    let start_idx = if ctrl.page_idx == 0 { 0 } else { 6 };
    let names: &[&str] = if ctrl.page_idx == 0 {
        &["RH:AIL", "RV:ELE", "LV:THR", "LH:RUD", "SW:SA ", "SW:SB "]
    } else {
        &["POT:V1", "POT:V2", "SW:SC ", "SW:SD ", "VBAT  "]
    };

    for (i, &name) in names.iter().enumerate() {
        let adc_idx = start_idx + i;
        let y = 12 + (i as i32 * 7);
        lcd.draw_str_4x6(2, y, name, false);

        let raw = raw_adc[adc_idx].min(4095);
        let fill_w = ((raw as u32 * 38) / 4095).min(38);
        widgets::draw_bar_gauge(lcd, 44, y + 1, 40, 5, fill_w);

        let mut val_buf = [0u8; 4];
        u16_to_dec_4(raw, &mut val_buf);
        let val_str = ascii_as_str(&val_buf);
        lcd.draw_str_4x6(90, y, val_str, false);
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

    lcd.draw_str_4x6(4, 14, "MCU:      ", false);
    lcd.draw_str_4x6(48, 14, profile.name, false);

    lcd.draw_str_4x6(4, 21, "Firmware: ", false);
    lcd.draw_str_4x6(
        48,
        21,
        concat!(env!("FIRMWARE_VERSION"), " (", env!("GIT_HASH"), ")"),
        false,
    );

    lcd.draw_str_4x6(4, 28, "Flash:    128KB (64 Pages)", false);
    lcd.draw_str_4x6(4, 35, "SRAM:     16KB (Parity)", false);
    lcd.draw_str_4x6(4, 42, "Profiles: 20 Models", false);
    lcd.draw_str_4x6(4, 49, "Bootloader: [OK] Reboot DFU", false);

    if keys.ok {
        buzzer.play_tone(2400, 100);
        crate::boot::reboot_to_dfu();
    }

    widgets::draw_footer(lcd, "[OK] DFU Mode  [ESC] Back");
}

