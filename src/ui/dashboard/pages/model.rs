//! Page 2 (P3/4): Model Dashboard, Profile, and Telemetry Summary.

use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    text::Text,
};

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
    let text_style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
    let active = storage.active_model();
    let is_crsf = active.rf_protocol == 1;

    // Line 1 (y = 21): Model Name & Type
    let m_name = ascii_as_str(&active.name);
    Text::new(m_name, Point::new(2, 21), text_style).draw(lcd).ok();

    let type_str = match active.model_type {
        0 => "AIRPLANE",
        1 => "GLIDER",
        2 => "HELI",
        _ => "QUAD",
    };
    Text::new(type_str, Point::new(74, 21), text_style).draw(lcd).ok();

    // Line 2 (y = 31): Receiver ID
    let mut rx_buf = [b'0'; 8];
    u32_to_hex(active.rx_id, &mut rx_buf);
    let rx_str = ascii_as_str(&rx_buf);
    Text::new("RxID:", Point::new(2, 31), text_style).draw(lcd).ok();
    Text::new(rx_str, Point::new(36, 31), text_style).draw(lcd).ok();

    // Line 3 (y = 41): Throttle Curve info
    let c_pts = if active.thr_curve_pts == 9 { "9-PT" } else { "5-PT" };
    let c_sm = if active.thr_curve_smooth != 0 { "SMOOTH" } else { "LINEAR" };
    Text::new("TCrv:", Point::new(2, 41), text_style).draw(lcd).ok();
    Text::new(c_pts, Point::new(36, 41), text_style).draw(lcd).ok();
    Text::new(c_sm, Point::new(74, 41), text_style).draw(lcd).ok();

    // Line 4 (y = 51): Telemetry Voltage & RSSI
    if telem.connected {
        let mut rxv_buf = [0u8; 6];
        let rxv_str = format_vbat(telem.rx_voltage_mv, &mut rxv_buf);
        Text::new("RX:", Point::new(2, 51), text_style).draw(lcd).ok();
        Text::new(rxv_str, Point::new(22, 51), text_style).draw(lcd).ok();

        let mut r_buf = *b"RSSI:   %";
        let r = telem.rssi.min(100);
        if r >= 10 {
            r_buf[5] = b'0' + (r / 10);
            r_buf[6] = b'0' + (r % 10);
        } else {
            r_buf[5] = b' ';
            r_buf[6] = b'0' + r;
        }
        let r_str = ascii_as_str(&r_buf);
        Text::new(r_str, Point::new(64, 51), text_style).draw(lcd).ok();
    } else if is_crsf {
        Text::new("CRSF: DISCONNECTED", Point::new(2, 51), text_style).draw(lcd).ok();
    } else {
        Text::new("AFHDS2A: DISCONNECTED", Point::new(2, 51), text_style).draw(lcd).ok();
    }

    // Standardized Footer (y = 55..63)
    if is_binding {
        widgets::draw_footer(lcd, "[ESC] Finish Bind");
    } else {
        widgets::draw_footer_split(lcd, "P3/4", "MODEL DASHBOARD");
    }
}
