//! Page 3 (P4/4): Telemetry and RF Diagnostics (CRSF & AFHDS 2A).

use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    text::Text,
};

use crate::crsf;
use crate::display::St7567;
use crate::rf::afhds2a::TelemetryData;
use crate::ui::format::{ascii_as_str, format_vbat, u32_to_dec_5};
use crate::ui::widgets;

#[inline(never)]
#[allow(clippy::too_many_arguments)]
pub fn render(
    lcd: &mut St7567,
    is_crsf: bool,
    telem: &TelemetryData,
    battery_mv: u16,
    min_rssi: u8,
    min_rx_v_mv: u16,
    telem_seen: bool,
    is_binding: bool,
) {
    let text_style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);

    if is_crsf {
        let ct = crsf::get_telemetry();

        // Left Column (x = 2..62)
        // Row 1 (y = 21): Link Quality (0..100%)
        let mut lq_buf = *b"LQ:   %";
        let lq = ct.uplink_link_quality.min(100);
        if lq == 100 {
            lq_buf = *b"LQ:100%";
        } else if lq >= 10 {
            lq_buf[3] = b' ';
            lq_buf[4] = b'0' + (lq / 10);
            lq_buf[5] = b'0' + (lq % 10);
        } else {
            lq_buf[3] = b' ';
            lq_buf[4] = b' ';
            lq_buf[5] = b'0' + lq;
        }
        let lq_str = if ct.connected { ascii_as_str(&lq_buf) } else { "LQ: ---%" };
        Text::new(lq_str, Point::new(2, 21), text_style).draw(lcd).ok();

        // Row 2 (y = 31): Uplink RSSI 1 (dBm)
        let mut rssi_buf = *b"RS: -000dB";
        let rssi_val = ct.uplink_rssi_1.unsigned_abs();
        rssi_buf[5] = b'0' + (rssi_val / 100);
        rssi_buf[6] = b'0' + ((rssi_val / 10) % 10);
        rssi_buf[7] = b'0' + (rssi_val % 10);
        let rssi_str = if ct.connected { ascii_as_str(&rssi_buf) } else { "RS: ---dB" };
        Text::new(rssi_str, Point::new(2, 31), text_style).draw(lcd).ok();

        // Row 3 (y = 41): Uplink SNR (dB)
        let mut snr_buf = *b"SNR:+00dB";
        let snr_sign = if ct.uplink_snr >= 0 { b'+' } else { b'-' };
        let snr_mag = ct.uplink_snr.unsigned_abs();
        snr_buf[4] = snr_sign;
        snr_buf[5] = b'0' + ((snr_mag / 10) % 10);
        snr_buf[6] = b'0' + (snr_mag % 10);
        let snr_str = if ct.connected { ascii_as_str(&snr_buf) } else { "SNR: --dB" };
        Text::new(snr_str, Point::new(2, 41), text_style).draw(lcd).ok();

        // Row 4 (y = 51): Active Antenna
        let ant_str = if ct.connected {
            if ct.active_antenna == 0 { "ANT: 1" } else { "ANT: 2" }
        } else {
            "ANT: -"
        };
        Text::new(ant_str, Point::new(2, 51), text_style).draw(lcd).ok();

        // Vertical divider line between columns
        lcd.draw_vline(64, 12, 42, true);

        // Right Column (x = 66..126)
        // Row 1 (y = 21): TX Power (mW)
        let mut pwr_buf = *b"PWR:    mW";
        let p = ct.tx_power_mw;
        if p >= 1000 {
            pwr_buf[4] = b'0' + ((p / 1000) as u8);
            pwr_buf[5] = b'0' + (((p / 100) % 10) as u8);
            pwr_buf[6] = b'0' + (((p / 10) % 10) as u8);
            pwr_buf[7] = b'0' + ((p % 10) as u8);
        } else if p >= 100 {
            pwr_buf[4] = b' ';
            pwr_buf[5] = b'0' + ((p / 100) as u8);
            pwr_buf[6] = b'0' + (((p / 10) % 10) as u8);
            pwr_buf[7] = b'0' + ((p % 10) as u8);
        } else if p >= 10 {
            pwr_buf[4] = b' ';
            pwr_buf[5] = b' ';
            pwr_buf[6] = b'0' + ((p / 10) as u8);
            pwr_buf[7] = b'0' + ((p % 10) as u8);
        }
        let pwr_str = if ct.connected && ct.tx_power_mw > 0 { ascii_as_str(&pwr_buf) } else { "PWR: ---" };
        Text::new(pwr_str, Point::new(66, 21), text_style).draw(lcd).ok();

        // Row 2 (y = 31): RF Mode / Packet Rate
        let rate_name = if ct.connected { crsf::protocol::rf_mode_to_str(ct.rf_mode) } else { "---" };
        Text::new("RATE:", Point::new(66, 31), text_style).draw(lcd).ok();
        Text::new(rate_name, Point::new(98, 31), text_style).draw(lcd).ok();

        // Row 3 (y = 41): Flight Battery Voltage
        Text::new("BAT:", Point::new(66, 41), text_style).draw(lcd).ok();
        if ct.connected && ct.rx_battery_mv > 0 {
            let mut rxv_buf = [0u8; 6];
            let rxv_str = format_vbat(ct.rx_battery_mv, &mut rxv_buf);
            Text::new(rxv_str, Point::new(92, 41), text_style).draw(lcd).ok();
        } else {
            Text::new("----", Point::new(92, 41), text_style).draw(lcd).ok();
        }

        // Row 4 (y = 51): Consumed Capacity (mAh)
        Text::new("CAP:", Point::new(66, 51), text_style).draw(lcd).ok();
        if ct.connected && ct.rx_capacity_mah > 0 {
            let mut cap_buf = [b' '; 5];
            u32_to_dec_5(ct.rx_capacity_mah.min(99999), &mut cap_buf);
            let cap_str = ascii_as_str(&cap_buf);
            Text::new(cap_str, Point::new(92, 51), text_style).draw(lcd).ok();
        } else {
            Text::new("----", Point::new(92, 51), text_style).draw(lcd).ok();
        }

        // Standardized Footer
        widgets::draw_footer_split(lcd, "P5/5", "CRSF LINK DIAG");
    } else {
        // Left Column (x = 2..62)
        // Row 1 (y = 21): RSSI
        if telem.connected {
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
            Text::new(r_str, Point::new(2, 21), text_style).draw(lcd).ok();
        } else {
            Text::new("RSSI: --%", Point::new(2, 21), text_style).draw(lcd).ok();
        }

        // Row 2 (y = 31): Link Status
        let link_str = if telem.connected { "LINK: OK" } else { "LINK: DISC" };
        Text::new(link_str, Point::new(2, 31), text_style).draw(lcd).ok();

        // Row 3 (y = 41): Packets Sent
        let mut tx_p_buf = [b' '; 5];
        u32_to_dec_5(telem.packets_sent, &mut tx_p_buf);
        let tx_p_str = ascii_as_str(&tx_p_buf);
        Text::new("TX:", Point::new(2, 41), text_style).draw(lcd).ok();
        Text::new(tx_p_str, Point::new(24, 41), text_style).draw(lcd).ok();

        // Row 4 (y = 51): Packets Received
        let mut rx_p_buf = [b' '; 5];
        u32_to_dec_5(telem.packets_received, &mut rx_p_buf);
        let rx_p_str = ascii_as_str(&rx_p_buf);
        Text::new("RX:", Point::new(2, 51), text_style).draw(lcd).ok();
        Text::new(rx_p_str, Point::new(24, 51), text_style).draw(lcd).ok();

        // Vertical divider line between columns
        lcd.draw_vline(64, 12, 42, true);

        // Right Column (x = 66..126)
        // Row 1 (y = 21): RX Battery Voltage
        Text::new("RX:", Point::new(66, 21), text_style).draw(lcd).ok();
        if telem.connected {
            let mut rxv_buf = [0u8; 6];
            let rxv_str = format_vbat(telem.rx_voltage_mv, &mut rxv_buf);
            Text::new(rxv_str, Point::new(88, 21), text_style).draw(lcd).ok();
        } else {
            Text::new("----", Point::new(88, 21), text_style).draw(lcd).ok();
        }

        // Row 2 (y = 31): TX Battery Voltage
        Text::new("TX:", Point::new(66, 31), text_style).draw(lcd).ok();
        let mut txv_buf = [0u8; 6];
        let txv_str = format_vbat(battery_mv, &mut txv_buf);
        Text::new(txv_str, Point::new(88, 31), text_style).draw(lcd).ok();

        // Row 3 (y = 41): Min Session RSSI
        Text::new("mRSS:", Point::new(66, 41), text_style).draw(lcd).ok();
        if telem_seen {
            let mut mr_buf = *b"    ";
            let r = min_rssi.min(100);
            if r == 100 {
                mr_buf = *b"100%";
            } else if r >= 10 {
                mr_buf[0] = b' ';
                mr_buf[1] = b'0' + (r / 10);
                mr_buf[2] = b'0' + (r % 10);
                mr_buf[3] = b'%';
            } else {
                mr_buf[0] = b' ';
                mr_buf[1] = b' ';
                mr_buf[2] = b'0' + r;
                mr_buf[3] = b'%';
            }
            let mr_str = ascii_as_str(&mr_buf);
            Text::new(mr_str, Point::new(98, 41), text_style).draw(lcd).ok();
        } else {
            Text::new(" --%", Point::new(98, 41), text_style).draw(lcd).ok();
        }

        // Row 4 (y = 51): Min Session RX Voltage
        Text::new("mRX:", Point::new(66, 51), text_style).draw(lcd).ok();
        if min_rx_v_mv != 0xFFFF && min_rx_v_mv > 0 {
            let mut mrxv_buf = [0u8; 6];
            let mrxv_str = format_vbat(min_rx_v_mv, &mut mrxv_buf);
            Text::new(mrxv_str, Point::new(92, 51), text_style).draw(lcd).ok();
        } else {
            Text::new("----", Point::new(92, 51), text_style).draw(lcd).ok();
        }

        // Standardized Footer
        if is_binding {
            widgets::draw_footer(lcd, "[ESC] Finish Bind");
        } else {
            widgets::draw_footer_split(lcd, "P5/5", "TELEMETRY SENSORS");
        }
    }
}
