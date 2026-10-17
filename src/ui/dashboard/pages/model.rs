//! Page 2 (P3/4): Model Dashboard, Profile, and Telemetry Summary.

use crate::display::St7567;
use crate::rf::afhds2a::TelemetryData;
use crate::storage::RadioStorage;
use crate::ui::format::{ascii_as_str, format_vbat, u32_to_hex};
use crate::ui::widgets;

#[inline(never)]
pub fn render(
    lcd: &mut St7567,
    storage: &RadioStorage,
    telem: &TelemetryData,
    is_binding: bool,
) {
    let active = storage.active_model();
    let is_crsf = active.rf_protocol == 1;

    // Line 1 (y = 14): Model Name & Type
    let m_name = ascii_as_str(&active.name);
    lcd.draw_str_6x10(2, 14, m_name, false);

    let type_str = active.model_type().as_str();
    lcd.draw_str_6x10(74, 14, type_str, false);

    // Line 2 (y = 24): Receiver ID
    let mut rx_buf = [b'0'; 8];
    u32_to_hex(active.rx_id, &mut rx_buf);
    let rx_str = ascii_as_str(&rx_buf);
    lcd.draw_str_6x10(2, 24, "RxID:", false);
    lcd.draw_str_6x10(36, 24, rx_str, false);

    // Line 3 (y = 34): Throttle Curve info
    let c_pts = if active.thr_curve_pts == 9 { "9-PT" } else { "5-PT" };
    let c_sm = if active.thr_curve_smooth != 0 { "SMOOTH" } else { "LINEAR" };
    lcd.draw_str_6x10(2, 34, "TCrv:", false);
    lcd.draw_str_6x10(36, 34, c_pts, false);
    lcd.draw_str_6x10(74, 34, c_sm, false);

    // Line 4 (y = 44): Telemetry Voltage & RSSI
    if telem.connected {
        lcd.draw_str_6x10(2, 44, "RX:", false);
        if telem.rx_voltage_mv > 0 {
            let mut rxv_buf = [0u8; 6];
            let rxv_str = format_vbat(telem.rx_voltage_mv, &mut rxv_buf);
            lcd.draw_str_6x10(22, 44, rxv_str, false);
        } else {
            lcd.draw_str_6x10(22, 44, "----", false);
        }

        if is_crsf {
            let mut lq_buf = *b"LQ:   %";
            let lq = telem.rssi.min(100);
            if lq >= 100 {
                lq_buf[3] = b'1';
                lq_buf[4] = b'0';
                lq_buf[5] = b'0';
            } else if lq >= 10 {
                lq_buf[4] = b'0' + (lq / 10);
                lq_buf[5] = b'0' + (lq % 10);
            } else {
                lq_buf[4] = b' ';
                lq_buf[5] = b'0' + lq;
            }
            let lq_str = ascii_as_str(&lq_buf);
            lcd.draw_str_6x10(70, 44, lq_str, false);
        } else {
            let mut r_buf = *b"RSSI:   %";
            let r = telem.rssi.min(100);
            if r >= 100 {
                r_buf[5] = b'1';
                r_buf[6] = b'0';
                r_buf[7] = b'0';
            } else if r >= 10 {
                r_buf[5] = b'0' + (r / 10);
                r_buf[6] = b'0' + (r % 10);
            } else {
                r_buf[5] = b' ';
                r_buf[6] = b'0' + r;
            }
            let r_str = ascii_as_str(&r_buf);
            lcd.draw_str_6x10(64, 44, r_str, false);
        }
    } else if is_crsf {
        lcd.draw_str_6x10(2, 44, "CRSF: DISCONNECTED", false);
    } else {
        lcd.draw_str_6x10(2, 44, "AFHDS2A: DISCONNECTED", false);
    }

    // Standardized Footer (y = 55..63)
    if is_binding {
        widgets::draw_footer(lcd, "[ESC] Finish Bind");
    } else {
        widgets::draw_footer_split(lcd, "P4/5", "MODEL DASHBOARD");
    }
}
