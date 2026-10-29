//! Analog diagnostics and System Information screens.

use crate::adc;
use crate::buzzer::Buzzer;
use crate::chip;
use crate::display::St7567;
use crate::menu::format::ascii_as_str;
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
        ctrl.page_idx = if ctrl.page_idx == 0 {
            4
        } else {
            ctrl.page_idx - 1
        };
        buzzer.play_tone(2200, 30);
    } else if keys.down || keys.ok {
        ctrl.page_idx = (ctrl.page_idx + 1) % 5;
        buzzer.play_tone(2200, 30);
    }

    let title = crate::hw::diag_page_title(ctrl.page_idx);
    widgets::draw_header(lcd, title);

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
            let raw = raw_adc[adc_idx].min(4095);
            let fill_w = ((raw as u32 * 38) / 4095).min(38);
            widgets::draw_gauge_row(lcd, y, name, fill_w, raw);
        }
    } else {
        crate::hw::draw_diag_ext_page(lcd, ctrl.page_idx, raw_adc, switches);
    }

    widgets::draw_footer(lcd, "[UP/DN/OK] Page  [ESC] Back");
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

    lcd.draw_str_4x6(4, 12, "MCU:      ", false);
    lcd.draw_str_4x6(48, 12, profile.name, false);

    lcd.draw_str_4x6(4, 18, "Firmware: ", false);
    lcd.draw_str_4x6(
        48,
        18,
        concat!(env!("FIRMWARE_VERSION"), " (", env!("GIT_HASH"), ")"),
        false,
    );

    lcd.draw_str_4x6(4, 25, "Flash: 128KB | SRAM: 16KB", false);

    let status = crate::storage::get_eeprom_status();
    let mut st_buf = *b"Storage:  xxxx @ 0x00 (OK) ";
    if status.detected {
        let bus = match status.bus {
            2 => b"I2C2",
            1 => b"I2C1",
            _ => b"I2C?",
        };
        st_buf[10..14].copy_from_slice(bus);
        st_buf[19] = b"0123456789ABCDEF"[(status.addr >> 4) as usize];
        st_buf[20] = b"0123456789ABCDEF"[(status.addr & 0x0F) as usize];
        let state = if status.write_err {
            b"(ERR)"
        } else if status.read_ok {
            b"(OK) "
        } else {
            b"(ERR)"
        };
        st_buf[22..27].copy_from_slice(state);
        lcd.draw_str_4x6(4, 32, ascii_as_str(&st_buf[..27]), false);
    } else {
        lcd.draw_str_4x6(4, 32, "Storage:  None (No EEPROM)", false);
    }

    lcd.draw_str_4x6(4, 39, "Profiles: 20 Models (16KB)", false);
    lcd.draw_str_4x6(4, 46, "Bootloader: [OK] Reboot DFU", false);

    if keys.ok {
        buzzer.play_tone(2400, 100);
        crate::boot::reboot_to_dfu();
    }

    widgets::draw_footer(lcd, "[OK] DFU Mode  [ESC] Back");
}
