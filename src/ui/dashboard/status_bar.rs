//! Top status bar widget for the flight dashboard.

use embedded_graphics::{
    mono_font::{ascii::FONT_4X6, ascii::FONT_6X10, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    text::Text,
};
use crate::ui::glyphs::draw_battery_gauge;

use crate::buzzer::Buzzer;
use crate::display::St7567;
use crate::rf::{self, afhds2a::TelemetryData};
use crate::storage::RadioStorage;
use crate::ui::format::{ascii_as_str, format_vbat, write_dec2};
use crate::usb;

const HEX_CHARS: &[u8; 16] = b"0123456789ABCDEF";

#[inline(never)]
#[allow(clippy::too_many_arguments)]
pub fn render(
    lcd: &mut St7567,
    storage: &RadioStorage,
    battery_mv: u16,
    rf_ok: bool,
    is_binding: bool,
    telem: &TelemetryData,
    buzzer: &mut Buzzer,
    blink_phase: u8,
    vbat_alarm_timer: &mut u32,
    rssi_alarm_timer: &mut u32,
    min_rssi: &mut u8,
    min_rx_v_mv: &mut u16,
    telem_seen: &mut bool,
) {
    let text_style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
    let text_style_small = MonoTextStyle::new(&FONT_4X6, BinaryColor::On);
    let is_crsf = storage.active_model().rf_protocol == 1;

    // --- Model Name (x = 2..60, y = 9) ---
    let act_idx = storage.radio.active_model as usize;
    let raw_name = ascii_as_str(&storage.active_model().name).trim();
    let mut m_buf = *b"M00";
    write_dec2((act_idx + 1) as u8, &mut m_buf[1..3]);
    let m_fallback = ascii_as_str(&m_buf);
    let m_display = if raw_name.is_empty() { m_fallback } else { raw_name };
    Text::new(m_display, Point::new(2, 9), text_style).draw(lcd).ok();

    // --- RF / Telemetry / Link Status (x = 64..84, y = 8) ---
    // Using FONT_4X6 (4x6 px), max 5 chars = 20px wide (e.g. "RF:OK", "NO RF", "R:100", "U:SIM").
    // Perfectly centered in the middle zone (x = 64..84) between model name and right-aligned battery block.
    if !rf_ok && !is_crsf {
        let id = rf::get_last_chip_id();
        let mut err_buf = *b"E:00";
        err_buf[2] = HEX_CHARS[((id >> 4) & 0x0F) as usize];
        err_buf[3] = HEX_CHARS[(id & 0x0F) as usize];
        let err_str = ascii_as_str(&err_buf);
        Text::new(err_str, Point::new(64, 8), text_style_small).draw(lcd).ok();
    } else if is_binding {
        Text::new("BIND", Point::new(64, 8), text_style_small).draw(lcd).ok();
    } else if usb::is_sim_mode() {
        Text::new("U:SIM", Point::new(64, 8), text_style_small).draw(lcd).ok();
    } else if telem.connected {
        let mut rssi_buf = *b"R:  %";
        let r = telem.rssi.min(100);
        if r >= 100 {
            Text::new("R:100", Point::new(64, 8), text_style_small).draw(lcd).ok();
        } else {
            write_dec2(r, &mut rssi_buf[2..4]);
            let r_str = ascii_as_str(&rssi_buf);
            Text::new(r_str, Point::new(64, 8), text_style_small).draw(lcd).ok();
        }
    } else if is_crsf {
        Text::new("CRSF", Point::new(64, 8), text_style_small).draw(lcd).ok();
    } else if rf::is_bound() {
        Text::new("RF:OK", Point::new(64, 8), text_style_small).draw(lcd).ok();
    } else {
        Text::new("NO RF", Point::new(64, 8), text_style_small).draw(lcd).ok();
    }

    // --- Battery Voltage Alarm & Display (Right-aligned, x = 93..126, y = 8) ---
    // Battery text: 5 chars with FONT_4X6 (20px wide, e.g. "4.12V"), Point(93, 8) -> x = 93..112.
    // Battery gauge: x = 116..126, y = 2..8 (11x7 px with tip on left at 116, body at 117..126).
    let mut vbat_buf = [0u8; 6];
    let vbat_str = format_vbat(battery_mv, &mut vbat_buf);
    let vbat_warn_mv = (storage.radio.vbat_warn_deci as u16) * 100;
    let vbat_is_low = battery_mv < vbat_warn_mv;

    if vbat_is_low {
        if *vbat_alarm_timer >= 8000 {
            *vbat_alarm_timer = 0;
            buzzer.warn_battery();
        } else {
            *vbat_alarm_timer += 33;
        }

        if (blink_phase & 0x10) != 0 {
            lcd.fill_rect(92, 1, 22, 8, true);
            let inv_style = MonoTextStyle::new(&FONT_4X6, BinaryColor::Off);
            Text::new(vbat_str, Point::new(93, 8), inv_style).draw(lcd).ok();
        } else {
            draw_battery_gauge(lcd, 116, 2, battery_mv, vbat_warn_mv, vbat_warn_mv.max(6000));
            Text::new(vbat_str, Point::new(93, 8), text_style_small).draw(lcd).ok();
        }
    } else {
        *vbat_alarm_timer = 7000;
        let vbat_max_mv = 6000u16.max(vbat_warn_mv.saturating_add(800));
        draw_battery_gauge(lcd, 116, 2, battery_mv, vbat_warn_mv, vbat_max_mv);
        Text::new(vbat_str, Point::new(93, 8), text_style_small).draw(lcd).ok();
    }

    // --- Telemetry RSSI Range Alarms & Stats Tracking ---
    if telem.connected {
        if !*telem_seen {
            *telem_seen = true;
            *min_rssi = telem.rssi;
            if telem.rx_voltage_mv > 0 {
                *min_rx_v_mv = telem.rx_voltage_mv;
            }
        } else {
            if telem.rssi < *min_rssi {
                *min_rssi = telem.rssi;
            }
            if telem.rx_voltage_mv > 0 && telem.rx_voltage_mv < *min_rx_v_mv {
                *min_rx_v_mv = telem.rx_voltage_mv;
            }
        }

        if telem.rssi < 20 {
            if *rssi_alarm_timer >= 3000 {
                *rssi_alarm_timer = 0;
                buzzer.warn_rssi_critical();
            } else {
                *rssi_alarm_timer += 33;
            }
        } else if telem.rssi < 40 {
            if *rssi_alarm_timer >= 6000 {
                *rssi_alarm_timer = 0;
                buzzer.warn_rssi_low();
            } else {
                *rssi_alarm_timer += 33;
            }
        } else {
            *rssi_alarm_timer = 0;
        }
    } else {
        *rssi_alarm_timer = 0;
    }

    // Header divider line (y = 11)
    lcd.draw_hline(0, 11, 128, true);
}
