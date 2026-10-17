//! Page 3 (P4/4): Telemetry and RF Diagnostics (CRSF & AFHDS 2A).

use crate::crsf;
use crate::display::St7567;
use crate::rf::afhds2a::TelemetryData;
use crate::storage::RadioStorage;
use crate::ui::format::{ascii_as_str, format_vbat, u32_to_dec_5};
use crate::ui::widgets;

#[allow(clippy::too_many_arguments)]
fn render_sensor_tile(
    lcd: &mut St7567,
    sensor_id: u8,
    x: i32,
    y: i32,
    is_crsf: bool,
    telem: &TelemetryData,
    battery_mv: u16,
    timer_str: Option<&str>,
) {
    let ct = crsf::get_telemetry();
    match sensor_id {
        0 => {
            // RX BAT
            lcd.draw_str_4x6(x, y, "RX BAT", false);
            if is_crsf {
                if ct.connected && ct.rx_battery_mv > 0 {
                    let mut buf = [0u8; 6];
                    let s = format_vbat(ct.rx_battery_mv, &mut buf);
                    lcd.draw_str_6x10(x, y + 7, s, false);
                } else {
                    lcd.draw_str_6x10(x, y + 7, "----", false);
                }
            } else if telem.connected && telem.rx_voltage_mv > 0 {
                let mut buf = [0u8; 6];
                let s = format_vbat(telem.rx_voltage_mv, &mut buf);
                lcd.draw_str_6x10(x, y + 7, s, false);
            } else {
                lcd.draw_str_6x10(x, y + 7, "----", false);
            }
        }
        1 => {
            // LQ / RSSI
            if is_crsf {
                lcd.draw_str_4x6(x, y, "LINK Q", false);
                if ct.connected {
                    let mut buf = *b"100%";
                    let lq = ct.uplink_link_quality.min(100);
                    if lq == 100 {
                        buf = *b"100%";
                    } else if lq >= 10 {
                        buf[0] = b' ';
                        buf[1] = b'0' + (lq / 10);
                        buf[2] = b'0' + (lq % 10);
                        buf[3] = b'%';
                    } else {
                        buf[0] = b' ';
                        buf[1] = b' ';
                        buf[2] = b'0' + lq;
                        buf[3] = b'%';
                    }
                    lcd.draw_str_6x10(x, y + 7, ascii_as_str(&buf), false);
                } else {
                    lcd.draw_str_6x10(x, y + 7, "---%", false);
                }
            } else {
                lcd.draw_str_4x6(x, y, "RSSI", false);
                if telem.connected {
                    let mut buf = *b"100%";
                    let r = telem.rssi.min(100);
                    if r == 100 {
                        buf = *b"100%";
                    } else if r >= 10 {
                        buf[0] = b' ';
                        buf[1] = b'0' + (r / 10);
                        buf[2] = b'0' + (r % 10);
                        buf[3] = b'%';
                    } else {
                        buf[0] = b' ';
                        buf[1] = b' ';
                        buf[2] = b'0' + r;
                        buf[3] = b'%';
                    }
                    lcd.draw_str_6x10(x, y + 7, ascii_as_str(&buf), false);
                } else {
                    lcd.draw_str_6x10(x, y + 7, "---%", false);
                }
            }
        }
        2 => {
            // TX BAT
            lcd.draw_str_4x6(x, y, "TX BAT", false);
            let mut buf = [0u8; 6];
            let s = format_vbat(battery_mv, &mut buf);
            lcd.draw_str_6x10(x, y + 7, s, false);
        }
        3 => {
            // CURRENT (CRSF) or LINK (AFHDS 2A)
            if is_crsf {
                lcd.draw_str_4x6(x, y, "CURRENT", false);
                if ct.connected && ct.rx_current_ma > 0 {
                    let mut buf = [b' '; 6];
                    let a = ct.rx_current_ma / 10;
                    let da = ct.rx_current_ma % 10;
                    buf[0] = b'0' + ((a / 10) % 10) as u8;
                    buf[1] = b'0' + (a % 10) as u8;
                    buf[2] = b'.';
                    buf[3] = b'0' + da as u8;
                    buf[4] = b'A';
                    lcd.draw_str_6x10(x, y + 7, ascii_as_str(&buf[..5]), false);
                } else {
                    lcd.draw_str_6x10(x, y + 7, "0.0A", false);
                }
            } else {
                lcd.draw_str_4x6(x, y, "LINK", false);
                let link_str = if telem.connected { "OK" } else { "DISC" };
                lcd.draw_str_6x10(x, y + 7, link_str, false);
            }
        }
        4 => {
            // CAPACITY
            lcd.draw_str_4x6(x, y, "CAP mAh", false);
            if is_crsf && ct.connected && ct.rx_capacity_mah > 0 {
                let mut buf = [b' '; 5];
                u32_to_dec_5(ct.rx_capacity_mah.min(99999), &mut buf);
                lcd.draw_str_6x10(x, y + 7, ascii_as_str(&buf), false);
            } else {
                lcd.draw_str_6x10(x, y + 7, "0", false);
            }
        }
        5 => {
            // TX POWER
            lcd.draw_str_4x6(x, y, "PWR mW", false);
            if is_crsf && ct.connected && ct.tx_power_mw > 0 {
                let mut buf = [b' '; 5];
                u32_to_dec_5(ct.tx_power_mw as u32, &mut buf);
                lcd.draw_str_6x10(x, y + 7, ascii_as_str(&buf), false);
            } else {
                lcd.draw_str_6x10(x, y + 7, "---", false);
            }
        }
        6 => {
            // SNR
            lcd.draw_str_4x6(x, y, "SNR dB", false);
            if is_crsf && ct.connected {
                let mut buf = *b"+00dB";
                let sign = if ct.uplink_snr >= 0 { b'+' } else { b'-' };
                let mag = ct.uplink_snr.unsigned_abs();
                buf[0] = sign;
                buf[1] = b'0' + ((mag / 10) % 10);
                buf[2] = b'0' + (mag % 10);
                lcd.draw_str_6x10(x, y + 7, ascii_as_str(&buf), false);
            } else {
                lcd.draw_str_6x10(x, y + 7, "--dB", false);
            }
        }
        7 => {
            // PKT RATE
            lcd.draw_str_4x6(x, y, "RATE", false);
            let rate_name = if is_crsf && ct.connected {
                crsf::protocol::rf_mode_to_str(ct.rf_mode)
            } else {
                "---"
            };
            lcd.draw_str_6x10(x, y + 7, rate_name, false);
        }
        8 => {
            // ANTENNA
            lcd.draw_str_4x6(x, y, "ANTENNA", false);
            let ant = if is_crsf && ct.connected {
                if ct.active_antenna == 0 { "ANT 1" } else { "ANT 2" }
            } else {
                "ANT -"
            };
            lcd.draw_str_6x10(x, y + 7, ant, false);
        }
        9 => {
            // PACKETS TX
            lcd.draw_str_4x6(x, y, "PKTS TX", false);
            let mut buf = [b' '; 5];
            u32_to_dec_5(telem.packets_sent, &mut buf);
            lcd.draw_str_6x10(x, y + 7, ascii_as_str(&buf), false);
        }
        10 => {
            // PACKETS RX
            lcd.draw_str_4x6(x, y, "PKTS RX", false);
            let mut buf = [b' '; 5];
            u32_to_dec_5(telem.packets_received, &mut buf);
            lcd.draw_str_6x10(x, y + 7, ascii_as_str(&buf), false);
        }
        11 => {
            // TIMER
            lcd.draw_str_4x6(x, y, "TIMER", false);
            lcd.draw_str_6x10(x, y + 7, timer_str.unwrap_or("00:00"), false);
        }
        _ => {
            // OFF
            lcd.draw_str_4x6(x, y, "SLOT OFF", false);
        }
    }
}

#[inline(never)]
#[allow(clippy::too_many_arguments)]
pub fn render(
    lcd: &mut St7567,
    storage: &RadioStorage,
    is_crsf: bool,
    telem: &TelemetryData,
    battery_mv: u16,
    min_rssi: u8,
    min_rx_v_mv: u16,
    telem_seen: bool,
    is_binding: bool,
    timer_str: Option<&str>,
) {
    let slots = storage.radio.telem_slots;
    let is_custom = slots != [0, 1, 2, 3];

    if is_custom {
        // Render 4 custom configurable telemetry tiles (2x2 grid)
        // Tile 0 (Top-Left): x=4, y=14
        render_sensor_tile(lcd, slots[0], 4, 14, is_crsf, telem, battery_mv, timer_str);
        // Tile 1 (Top-Right): x=68, y=14
        render_sensor_tile(lcd, slots[1], 68, 14, is_crsf, telem, battery_mv, timer_str);
        // Tile 2 (Bottom-Left): x=4, y=33
        render_sensor_tile(lcd, slots[2], 4, 33, is_crsf, telem, battery_mv, timer_str);
        // Tile 3 (Bottom-Right): x=68, y=33
        render_sensor_tile(lcd, slots[3], 68, 33, is_crsf, telem, battery_mv, timer_str);

        // Grid divider lines
        lcd.draw_vline(64, 12, 42, true);
        lcd.draw_hline(2, 31, 124, true);

        // Standardized Footer
        if is_binding {
            widgets::draw_footer(lcd, "[ESC] Finish Bind");
        } else {
            widgets::draw_footer_split(lcd, "P5/5", "CUSTOM TELEM");
        }
        return;
    }
    if is_crsf {
        let ct = crsf::get_telemetry();

        // Left Column (x = 2..62)
        // Row 1 (y = 14): Link Quality (0..100%)
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
        lcd.draw_str_6x10(2, 14, lq_str, false);

        // Row 2 (y = 24): Uplink RSSI 1 (dBm)
        let mut rssi_buf = *b"RS: -000dB";
        let rssi_val = ct.uplink_rssi_1.unsigned_abs();
        rssi_buf[5] = b'0' + (rssi_val / 100);
        rssi_buf[6] = b'0' + ((rssi_val / 10) % 10);
        rssi_buf[7] = b'0' + (rssi_val % 10);
        let rssi_str = if ct.connected { ascii_as_str(&rssi_buf) } else { "RS: ---dB" };
        lcd.draw_str_6x10(2, 24, rssi_str, false);

        // Row 3 (y = 34): Uplink SNR (dB)
        let mut snr_buf = *b"SNR:+00dB";
        let snr_sign = if ct.uplink_snr >= 0 { b'+' } else { b'-' };
        let snr_mag = ct.uplink_snr.unsigned_abs();
        snr_buf[4] = snr_sign;
        snr_buf[5] = b'0' + ((snr_mag / 10) % 10);
        snr_buf[6] = b'0' + (snr_mag % 10);
        let snr_str = if ct.connected { ascii_as_str(&snr_buf) } else { "SNR: --dB" };
        lcd.draw_str_6x10(2, 34, snr_str, false);

        // Row 4 (y = 44): Active Antenna
        let ant_str = if ct.connected {
            if ct.active_antenna == 0 { "ANT: 1" } else { "ANT: 2" }
        } else {
            "ANT: -"
        };
        lcd.draw_str_6x10(2, 44, ant_str, false);

        // Vertical divider line between columns
        lcd.draw_vline(64, 12, 42, true);

        // Right Column (x = 66..126)
        // Row 1 (y = 14): TX Power (mW)
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
        lcd.draw_str_6x10(66, 14, pwr_str, false);

        // Row 2 (y = 24): RF Mode / Packet Rate
        let rate_name = if ct.connected { crsf::protocol::rf_mode_to_str(ct.rf_mode) } else { "---" };
        lcd.draw_str_6x10(66, 24, "RATE:", false);
        lcd.draw_str_6x10(98, 24, rate_name, false);

        // Row 3 (y = 34): Flight Battery Voltage
        lcd.draw_str_6x10(66, 34, "BAT:", false);
        if ct.connected && ct.rx_battery_mv > 0 {
            let mut rxv_buf = [0u8; 6];
            let rxv_str = format_vbat(ct.rx_battery_mv, &mut rxv_buf);
            lcd.draw_str_6x10(92, 34, rxv_str, false);
        } else {
            lcd.draw_str_6x10(92, 34, "----", false);
        }

        // Row 4 (y = 44): Consumed Capacity (mAh)
        lcd.draw_str_6x10(66, 44, "CAP:", false);
        if ct.connected && ct.rx_capacity_mah > 0 {
            let mut cap_buf = [b' '; 5];
            u32_to_dec_5(ct.rx_capacity_mah.min(99999), &mut cap_buf);
            let cap_str = ascii_as_str(&cap_buf);
            lcd.draw_str_6x10(92, 44, cap_str, false);
        } else {
            lcd.draw_str_6x10(92, 44, "----", false);
        }

        // Standardized Footer
        widgets::draw_footer_split(lcd, "P5/5", "CRSF LINK DIAG");
    } else {
        // Left Column (x = 2..62)
        // Row 1 (y = 14): RSSI
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
            lcd.draw_str_6x10(2, 14, r_str, false);
        } else {
            lcd.draw_str_6x10(2, 14, "RSSI: --%", false);
        }

        // Row 2 (y = 24): Link Status
        let link_str = if telem.connected { "LINK: OK" } else { "LINK: DISC" };
        lcd.draw_str_6x10(2, 24, link_str, false);

        // Row 3 (y = 34): Packets Sent
        let mut tx_p_buf = [b' '; 5];
        u32_to_dec_5(telem.packets_sent, &mut tx_p_buf);
        let tx_p_str = ascii_as_str(&tx_p_buf);
        lcd.draw_str_6x10(2, 34, "TX:", false);
        lcd.draw_str_6x10(24, 34, tx_p_str, false);

        // Row 4 (y = 44): Packets Received
        let mut rx_p_buf = [b' '; 5];
        u32_to_dec_5(telem.packets_received, &mut rx_p_buf);
        let rx_p_str = ascii_as_str(&rx_p_buf);
        lcd.draw_str_6x10(2, 44, "RX:", false);
        lcd.draw_str_6x10(24, 44, rx_p_str, false);

        // Vertical divider line between columns
        lcd.draw_vline(64, 12, 42, true);

        // Right Column (x = 66..126)
        // Row 1 (y = 14): RX Battery Voltage
        lcd.draw_str_6x10(66, 14, "RX:", false);
        if telem.connected {
            let mut rxv_buf = [0u8; 6];
            let rxv_str = format_vbat(telem.rx_voltage_mv, &mut rxv_buf);
            lcd.draw_str_6x10(88, 14, rxv_str, false);
        } else {
            lcd.draw_str_6x10(88, 14, "----", false);
        }

        // Row 2 (y = 24): TX Battery Voltage
        lcd.draw_str_6x10(66, 24, "TX:", false);
        let mut txv_buf = [0u8; 6];
        let txv_str = format_vbat(battery_mv, &mut txv_buf);
        lcd.draw_str_6x10(88, 24, txv_str, false);

        // Row 3 (y = 34): Min Session RSSI
        lcd.draw_str_6x10(66, 34, "mRSS:", false);
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
            lcd.draw_str_6x10(98, 34, mr_str, false);
        } else {
            lcd.draw_str_6x10(98, 34, " --%", false);
        }

        // Row 4 (y = 44): Min Session RX Voltage
        lcd.draw_str_6x10(66, 44, "mRX:", false);
        if min_rx_v_mv != 0xFFFF && min_rx_v_mv > 0 {
            let mut mrxv_buf = [0u8; 6];
            let mrxv_str = format_vbat(min_rx_v_mv, &mut mrxv_buf);
            lcd.draw_str_6x10(92, 44, mrxv_str, false);
        } else {
            lcd.draw_str_6x10(92, 44, "----", false);
        }

        // Standardized Footer
        if is_binding {
            widgets::draw_footer(lcd, "[ESC] Finish Bind");
        } else {
            widgets::draw_footer_split(lcd, "P5/5", "TELEMETRY SENSORS");
        }
    }
}
