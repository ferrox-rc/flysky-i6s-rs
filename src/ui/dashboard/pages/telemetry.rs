//! Page 3 (P4/4): Telemetry and RF Diagnostics (CRSF & AFHDS 2A).

use crate::crsf;
use crate::display::St7567;
use crate::rf::afhds2a::TelemetryData;
use crate::storage::RadioStorage;
use crate::ui::format::{ascii_as_str, format_vbat, u32_to_dec_5};
use crate::ui::widgets;

#[inline(always)]
fn check_sensor_status(last_ms: u32, now_ms: u32, connected: bool) -> (bool, bool) {
    if last_ms == 0 {
        (false, false)
    } else {
        let elapsed = now_ms.wrapping_sub(last_ms);
        let stale = !connected || (elapsed >= 2500);
        (true, stale)
    }
}

#[inline(always)]
fn append_stale<'a>(s: &'a str, is_stale: bool, buf: &'a mut [u8; 12]) -> &'a str {
    if !is_stale {
        return s;
    }
    let bytes = s.as_bytes();
    let copy_len = bytes.len().min(buf.len() - 1);
    buf[..copy_len].copy_from_slice(&bytes[..copy_len]);
    buf[copy_len] = b'*';
    ascii_as_str(&buf[..copy_len + 1])
}

fn format_u32_append<'a>(val: u32, suffix: &str, buf: &'a mut [u8; 12]) -> &'a str {
    let mut temp = [0u8; 10];
    let mut count = 0;
    let mut v = val;
    if v == 0 {
        temp[0] = b'0';
        count = 1;
    } else {
        while v > 0 && count < temp.len() {
            temp[count] = b'0' + (v % 10) as u8;
            v /= 10;
            count += 1;
        }
    }
    let mut pos = 0;
    for i in 0..count {
        buf[pos] = temp[count - 1 - i];
        pos += 1;
    }
    let s_bytes = suffix.as_bytes();
    let copy_len = s_bytes.len().min(buf.len() - pos);
    buf[pos..pos + copy_len].copy_from_slice(&s_bytes[..copy_len]);
    pos += copy_len;
    ascii_as_str(&buf[..pos])
}

fn format_temp(temp_x10: i16, buf: &mut [u8; 12]) -> &str {
    let mut pos = 0;
    let abs_val = if temp_x10 < 0 {
        buf[pos] = b'-';
        pos += 1;
        temp_x10.unsigned_abs()
    } else {
        temp_x10 as u16
    };
    let whole = abs_val / 10;
    let frac = (abs_val % 10) as u8;
    let mut n_buf = [0u8; 12];
    let w_str = format_u32_append(whole as u32, "", &mut n_buf);
    for &b in w_str.as_bytes() {
        buf[pos] = b;
        pos += 1;
    }
    buf[pos] = b'.';
    pos += 1;
    buf[pos] = b'0' + frac;
    pos += 1;
    buf[pos] = b'C';
    pos += 1;
    ascii_as_str(&buf[..pos])
}

fn format_angle(angle_deg: i16, buf: &mut [u8; 12]) -> &str {
    let mut pos = 0;
    let (sign, abs_deg) = if angle_deg < 0 {
        (b'-', angle_deg.unsigned_abs() as u32)
    } else {
        (b'+', angle_deg as u32)
    };
    buf[pos] = sign;
    pos += 1;
    let mut n_buf = [0u8; 12];
    let s = format_u32_append(abs_deg, "D", &mut n_buf);
    for &b in s.as_bytes() {
        if pos < buf.len() {
            buf[pos] = b;
            pos += 1;
        }
    }
    ascii_as_str(&buf[..pos])
}

#[inline(never)]
fn render_tile(lcd: &mut St7567, x: i32, y: i32, label: &str, val: &str, is_stale: bool) {
    lcd.draw_str_4x6(x, y, label, false);
    if is_stale {
        lcd.draw_str_4x6(x + 50, y, "*", false);
    }
    let mut s_buf = [0u8; 12];
    let final_s = append_stale(val, is_stale, &mut s_buf);
    lcd.draw_str_6x10(x, y + 7, final_s, false);
}

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
    now_ms: u32,
) {
    let ct = crsf::get_telemetry();
    let mut buf = [0u8; 12];
    match sensor_id {
        0 => {
            let last_ms = if is_crsf {
                ct.last_vbat_ms
            } else {
                telem.last_rx_voltage_ms
            };
            let connected = if is_crsf {
                ct.connected
            } else {
                telem.connected
            };
            let mv = if is_crsf {
                ct.rx_battery_mv
            } else {
                telem.rx_voltage_mv
            };
            let (has_data, is_stale) = check_sensor_status(last_ms, now_ms, connected);
            let val = if has_data && mv > 0 {
                let mut v_buf = [0u8; 6];
                format_vbat(mv, &mut v_buf);
                v_buf
            } else {
                *b"----\0\0"
            };
            render_tile(lcd, x, y, "RX BAT", ascii_as_str(&val[..4.max(val.iter().position(|&b| b == 0).unwrap_or(val.len()))]), is_stale);
        }
        1 => {
            if is_crsf {
                let (has_data, is_stale) =
                    check_sensor_status(ct.last_link_ms, now_ms, ct.connected);
                let val = if has_data {
                    let mut b = *b"100%";
                    let lq = ct.uplink_link_quality.min(100);
                    if lq < 100 {
                        b[0] = b' ';
                        b[1] = if lq >= 10 { b'0' + (lq / 10) } else { b' ' };
                        b[2] = b'0' + (lq % 10);
                    }
                    b
                } else {
                    *b"---%"
                };
                render_tile(lcd, x, y, "LINK Q", ascii_as_str(&val), is_stale);
            } else {
                let (has_data, is_stale) =
                    check_sensor_status(telem.last_rx_voltage_ms, now_ms, telem.connected);
                let val = if has_data {
                    let mut b = *b"100%";
                    let r = telem.rssi.min(100);
                    if r < 100 {
                        b[0] = b' ';
                        b[1] = if r >= 10 { b'0' + (r / 10) } else { b' ' };
                        b[2] = b'0' + (r % 10);
                    }
                    b
                } else {
                    *b"---%"
                };
                render_tile(lcd, x, y, "RSSI", ascii_as_str(&val), is_stale);
            }
        }
        2 => {
            let mut v_buf = [0u8; 6];
            let s = format_vbat(battery_mv, &mut v_buf);
            render_tile(lcd, x, y, "TX BAT", s, false);
        }
        3 => {
            if is_crsf {
                let (has_data, is_stale) =
                    check_sensor_status(ct.last_vbat_ms, now_ms, ct.connected);
                if has_data && ct.rx_current_ma > 0 {
                    let a = ct.rx_current_ma / 10;
                    let da = ct.rx_current_ma % 10;
                    buf[0] = b'0' + ((a / 10) % 10) as u8;
                    buf[1] = b'0' + (a % 10) as u8;
                    buf[2] = b'.';
                    buf[3] = b'0' + da as u8;
                    buf[4] = b'A';
                    render_tile(lcd, x, y, "CURRENT", ascii_as_str(&buf[..5]), is_stale);
                } else {
                    render_tile(lcd, x, y, "CURRENT", "0.0A", false);
                }
            } else {
                let link_str = if telem.connected { "OK" } else { "DISC" };
                render_tile(lcd, x, y, "LINK", link_str, false);
            }
        }
        4 => {
            let (has_data, is_stale) = check_sensor_status(ct.last_vbat_ms, now_ms, ct.connected);
            if is_crsf && has_data && ct.rx_capacity_mah > 0 {
                let s = format_u32_append(ct.rx_capacity_mah.min(99999), "", &mut buf);
                render_tile(lcd, x, y, "CAP mAh", s, is_stale);
            } else {
                render_tile(lcd, x, y, "CAP mAh", "0", false);
            }
        }
        5 => {
            let (has_data, is_stale) = check_sensor_status(ct.last_link_ms, now_ms, ct.connected);
            if is_crsf && has_data && ct.tx_power_mw > 0 {
                let s = format_u32_append(ct.tx_power_mw as u32, "", &mut buf);
                render_tile(lcd, x, y, "PWR mW", s, is_stale);
            } else {
                render_tile(lcd, x, y, "PWR mW", "---", false);
            }
        }
        6 => {
            let (has_data, is_stale) = check_sensor_status(ct.last_link_ms, now_ms, ct.connected);
            if is_crsf && has_data {
                let sign = if ct.uplink_snr >= 0 { b'+' } else { b'-' };
                let mag = ct.uplink_snr.unsigned_abs();
                buf[0] = sign;
                buf[1] = b'0' + ((mag / 10) % 10);
                buf[2] = b'0' + (mag % 10);
                buf[3] = b'd';
                buf[4] = b'B';
                render_tile(lcd, x, y, "SNR dB", ascii_as_str(&buf[..5]), is_stale);
            } else {
                render_tile(lcd, x, y, "SNR dB", "--dB", false);
            }
        }
        7 => {
            let (has_data, is_stale) = check_sensor_status(ct.last_link_ms, now_ms, ct.connected);
            let s = if is_crsf && has_data {
                crsf::protocol::rf_mode_to_str(ct.rf_mode)
            } else {
                "---"
            };
            render_tile(lcd, x, y, "RATE", s, is_stale);
        }
        8 => {
            let (has_data, is_stale) = check_sensor_status(ct.last_link_ms, now_ms, ct.connected);
            let s = if is_crsf && has_data {
                if ct.active_antenna == 0 { "ANT 1" } else { "ANT 2" }
            } else {
                "ANT -"
            };
            render_tile(lcd, x, y, "ANTENNA", s, is_stale);
        }
        9 => {
            let s = format_u32_append(telem.packets_sent, "", &mut buf);
            render_tile(lcd, x, y, "PKTS TX", s, false);
        }
        10 => {
            let s = format_u32_append(telem.packets_received, "", &mut buf);
            render_tile(lcd, x, y, "PKTS RX", s, false);
        }
        11 => {
            render_tile(lcd, x, y, "TIMER", timer_str.unwrap_or("00:00"), false);
        }
        12 => {
            let last_ms = if is_crsf {
                ct.last_vspd_ms
            } else {
                telem.last_vspd_ms
            };
            let connected = if is_crsf {
                ct.connected
            } else {
                telem.connected
            };
            let (has_data, is_stale) = check_sensor_status(last_ms, now_ms, connected);
            if has_data {
                let mut b = *b"+0.0m/s";
                let vspeed = telem.vspeed_cms;
                b[0] = if vspeed >= 0 { b'+' } else { b'-' };
                let mag = (vspeed.unsigned_abs()).min(999);
                b[1] = b'0' + (mag / 100) as u8;
                b[2] = b'.';
                b[3] = b'0' + ((mag % 100) / 10) as u8;
                render_tile(lcd, x, y, "VSPD", ascii_as_str(&b), is_stale);
            } else {
                render_tile(lcd, x, y, "VSPD", "---m/s", false);
            }
        }
        13 => {
            let last_ms = if is_crsf {
                ct.last_alt_ms
            } else {
                telem.last_alt_ms
            };
            let connected = if is_crsf {
                ct.connected
            } else {
                telem.connected
            };
            let (has_data, is_stale) = check_sensor_status(last_ms, now_ms, connected);
            if has_data {
                let alt = telem.alt_m;
                let mut pos = 0;
                if alt < 0 {
                    buf[pos] = b'-';
                    pos += 1;
                }
                let mut n_buf = [0u8; 12];
                let s = format_u32_append(alt.unsigned_abs() as u32, "m", &mut n_buf);
                for &b in s.as_bytes() {
                    if pos < buf.len() {
                        buf[pos] = b;
                        pos += 1;
                    }
                }
                render_tile(lcd, x, y, "ALT", ascii_as_str(&buf[..pos]), is_stale);
            } else {
                render_tile(lcd, x, y, "ALT", "---m", false);
            }
        }
        14 => {
            let last_ms = if is_crsf {
                ct.last_cells_ms
            } else {
                telem.last_rx_voltage_ms
            };
            let connected = if is_crsf {
                ct.connected
            } else {
                telem.connected
            };
            let (has_data, is_stale) = check_sensor_status(last_ms, now_ms, connected);
            if is_crsf {
                if has_data && ct.cell_count > 0 && ct.cell_min_mv > 0 {
                    let mut pos = 0;
                    if ct.cell_count > 1 && ct.cell_count < 10 {
                        buf[pos] = b'0' + ct.cell_count;
                        pos += 1;
                        buf[pos] = b'S';
                        pos += 1;
                        buf[pos] = b' ';
                        pos += 1;
                    }
                    let whole = ct.cell_min_mv / 1000;
                    let frac = (ct.cell_min_mv % 1000) / 10;
                    buf[pos] = b'0' + (whole as u8);
                    pos += 1;
                    buf[pos] = b'.';
                    pos += 1;
                    buf[pos] = b'0' + ((frac / 10) as u8);
                    pos += 1;
                    buf[pos] = b'0' + ((frac % 10) as u8);
                    pos += 1;
                    buf[pos] = b'V';
                    pos += 1;
                    render_tile(lcd, x, y, "CELLS", ascii_as_str(&buf[..pos]), is_stale);
                } else if has_data && ct.rx_battery_mv > 0 {
                    let mut v_buf = [0u8; 6];
                    let s = format_vbat(ct.rx_battery_mv, &mut v_buf);
                    render_tile(lcd, x, y, "CELLS", s, is_stale);
                } else {
                    render_tile(lcd, x, y, "CELLS", "---V", false);
                }
            } else if has_data && telem.rx_voltage_mv > 0 {
                let mut v_buf = [0u8; 6];
                let s = format_vbat(telem.rx_voltage_mv, &mut v_buf);
                render_tile(lcd, x, y, "CELLS", s, is_stale);
            } else {
                render_tile(lcd, x, y, "CELLS", "---V", false);
            }
        }
        15 => {
            let last_ms = if is_crsf {
                ct.last_rpm_ms
            } else {
                telem.last_rpm_ms
            };
            let connected = if is_crsf {
                ct.connected
            } else {
                telem.connected
            };
            let (has_data, is_stale) = check_sensor_status(last_ms, now_ms, connected);
            let rpm = if is_crsf { ct.rpm } else { telem.rpm };
            if has_data {
                let s = if rpm >= 100_000 {
                    format_u32_append(rpm / 1000, "k", &mut buf)
                } else {
                    format_u32_append(rpm, "", &mut buf)
                };
                render_tile(lcd, x, y, "RPM", s, is_stale);
            } else {
                render_tile(lcd, x, y, "RPM", "----", false);
            }
        }
        16 => {
            let last_ms = if is_crsf {
                ct.last_temp_ms
            } else {
                telem.last_temp_ms
            };
            let connected = if is_crsf {
                ct.connected
            } else {
                telem.connected
            };
            let (has_data, is_stale) = check_sensor_status(last_ms, now_ms, connected);
            let temp_x10 = if is_crsf {
                ct.temp_c_x10
            } else {
                telem.temp_c_x10
            };
            if has_data {
                let s = format_temp(temp_x10, &mut buf);
                render_tile(lcd, x, y, "TEMP", s, is_stale);
            } else {
                render_tile(lcd, x, y, "TEMP", "---C", false);
            }
        }
        17 => {
            let (has_data, is_stale) = check_sensor_status(ct.last_gps_ms, now_ms, ct.connected);
            if is_crsf && has_data {
                let s = format_u32_append(ct.gps_groundspeed_kmh_x10 as u32 / 10, "kmh", &mut buf);
                render_tile(lcd, x, y, "GSPD", s, is_stale);
            } else {
                render_tile(lcd, x, y, "GSPD", "---km", false);
            }
        }
        18 => {
            let (has_data, is_stale) = check_sensor_status(ct.last_gps_ms, now_ms, ct.connected);
            if is_crsf && has_data {
                let s = format_u32_append(ct.gps_satellites as u32, "", &mut buf);
                render_tile(lcd, x, y, "SATS", s, is_stale);
            } else {
                render_tile(lcd, x, y, "SATS", "--", false);
            }
        }
        19 => {
            let last_hdg_ms = if ct.last_gps_ms > 0 {
                ct.last_gps_ms
            } else {
                ct.last_att_ms
            };
            let (has_data, is_stale) = check_sensor_status(last_hdg_ms, now_ms, ct.connected);
            if is_crsf && has_data {
                let deg = if ct.last_gps_ms > 0 {
                    ct.gps_heading_cdeg / 100
                } else {
                    (ct.yaw_cdeg / 10).rem_euclid(360) as u16
                };
                let s = format_u32_append(deg as u32, "D", &mut buf);
                render_tile(lcd, x, y, "HDG", s, is_stale);
            } else {
                render_tile(lcd, x, y, "HDG", "---", false);
            }
        }
        20 => {
            let (has_data, is_stale) = check_sensor_status(ct.last_att_ms, now_ms, ct.connected);
            if is_crsf && has_data {
                let s = format_angle(ct.pitch_cdeg / 10, &mut buf);
                render_tile(lcd, x, y, "PITCH", s, is_stale);
            } else {
                render_tile(lcd, x, y, "PITCH", "---", false);
            }
        }
        21 => {
            let (has_data, is_stale) = check_sensor_status(ct.last_att_ms, now_ms, ct.connected);
            if is_crsf && has_data {
                let s = format_angle(ct.roll_cdeg / 10, &mut buf);
                render_tile(lcd, x, y, "ROLL", s, is_stale);
            } else {
                render_tile(lcd, x, y, "ROLL", "---", false);
            }
        }
        _ => {
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
    now_ms: u32,
) {
    let slots = storage.active_model.telem_slots;
    let is_custom = slots != [0, 1, 2, 3];

    if is_custom {
        // Render 4 custom configurable telemetry tiles (2x2 grid)
        render_sensor_tile(
            lcd, slots[0], 4, 14, is_crsf, telem, battery_mv, timer_str, now_ms,
        );
        render_sensor_tile(
            lcd, slots[1], 68, 14, is_crsf, telem, battery_mv, timer_str, now_ms,
        );
        render_sensor_tile(
            lcd, slots[2], 4, 33, is_crsf, telem, battery_mv, timer_str, now_ms,
        );
        render_sensor_tile(
            lcd, slots[3], 68, 33, is_crsf, telem, battery_mv, timer_str, now_ms,
        );

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
        let (link_has, link_stale) = check_sensor_status(ct.last_link_ms, now_ms, ct.connected);
        let (vbat_has, vbat_stale) = check_sensor_status(ct.last_vbat_ms, now_ms, ct.connected);

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
        if link_has {
            let mut s_buf = [0u8; 12];
            let s = append_stale(ascii_as_str(&lq_buf), link_stale, &mut s_buf);
            lcd.draw_str_6x10(2, 14, s, false);
        } else {
            lcd.draw_str_6x10(2, 14, "LQ: ---%", false);
        }

        // Row 2 (y = 24): Uplink RSSI 1 (dBm)
        let mut rssi_buf = *b"RS: -000dB";
        let rssi_val = ct.uplink_rssi_1.unsigned_abs();
        rssi_buf[5] = b'0' + (rssi_val / 100);
        rssi_buf[6] = b'0' + ((rssi_val / 10) % 10);
        rssi_buf[7] = b'0' + (rssi_val % 10);
        if link_has {
            let mut s_buf = [0u8; 12];
            let s = append_stale(ascii_as_str(&rssi_buf), link_stale, &mut s_buf);
            lcd.draw_str_6x10(2, 24, s, false);
        } else {
            lcd.draw_str_6x10(2, 24, "RS: ---dB", false);
        }

        // Row 3 (y = 34): Uplink SNR (dB)
        let mut snr_buf = *b"SNR:+00dB";
        let snr_sign = if ct.uplink_snr >= 0 { b'+' } else { b'-' };
        let snr_mag = ct.uplink_snr.unsigned_abs();
        snr_buf[4] = snr_sign;
        snr_buf[5] = b'0' + ((snr_mag / 10) % 10);
        snr_buf[6] = b'0' + (snr_mag % 10);
        if link_has {
            let mut s_buf = [0u8; 12];
            let s = append_stale(ascii_as_str(&snr_buf), link_stale, &mut s_buf);
            lcd.draw_str_6x10(2, 34, s, false);
        } else {
            lcd.draw_str_6x10(2, 34, "SNR: --dB", false);
        }

        // Row 4 (y = 44): Active Antenna
        let ant_str = if ct.connected {
            if ct.active_antenna == 0 {
                "ANT: 1"
            } else {
                "ANT: 2"
            }
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
        if link_has && ct.tx_power_mw > 0 {
            let mut s_buf = [0u8; 12];
            let s = append_stale(ascii_as_str(&pwr_buf), link_stale, &mut s_buf);
            lcd.draw_str_6x10(66, 14, s, false);
        } else {
            lcd.draw_str_6x10(66, 14, "PWR: ---", false);
        }

        // Row 2 (y = 24): RF Mode / Packet Rate
        let rate_name = if link_has {
            crsf::protocol::rf_mode_to_str(ct.rf_mode)
        } else {
            "---"
        };
        lcd.draw_str_6x10(66, 24, "RATE:", false);
        lcd.draw_str_6x10(98, 24, rate_name, false);

        // Row 3 (y = 34): Flight Battery Voltage
        if vbat_has && ct.cell_count > 1 && ct.cell_count < 10 {
            let mut label_buf = *b"0S: ";
            label_buf[0] = b'0' + ct.cell_count;
            lcd.draw_str_6x10(66, 34, ascii_as_str(&label_buf), false);
        } else {
            lcd.draw_str_6x10(66, 34, "BAT:", false);
        }
        if vbat_has && ct.rx_battery_mv > 0 {
            let mut rxv_buf = [0u8; 6];
            let rxv_str = format_vbat(ct.rx_battery_mv, &mut rxv_buf);
            let mut s_buf = [0u8; 12];
            let s = append_stale(rxv_str, vbat_stale, &mut s_buf);
            lcd.draw_str_6x10(92, 34, s, false);
        } else {
            lcd.draw_str_6x10(92, 34, "----", false);
        }

        // Row 4 (y = 44): Consumed Capacity (mAh)
        lcd.draw_str_6x10(66, 44, "CAP:", false);
        if vbat_has && ct.rx_capacity_mah > 0 {
            let mut cap_buf = [b' '; 5];
            u32_to_dec_5(ct.rx_capacity_mah.min(99999), &mut cap_buf);
            let mut s_buf = [0u8; 12];
            let cap_str = append_stale(ascii_as_str(&cap_buf), vbat_stale, &mut s_buf);
            lcd.draw_str_6x10(92, 44, cap_str, false);
        } else {
            lcd.draw_str_6x10(92, 44, "----", false);
        }

        // Standardized Footer
        widgets::draw_footer_split(lcd, "P5/5", "CRSF LINK DIAG");
    } else {
        let (rxv_has, rxv_stale) =
            check_sensor_status(telem.last_rx_voltage_ms, now_ms, telem.connected);

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
        let link_str = if telem.connected {
            "LINK: OK"
        } else {
            "LINK: DISC"
        };
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
        if rxv_has && telem.rx_voltage_mv > 0 {
            let mut rxv_buf = [0u8; 6];
            let rxv_str = format_vbat(telem.rx_voltage_mv, &mut rxv_buf);
            let mut s_buf = [0u8; 12];
            let s = append_stale(rxv_str, rxv_stale, &mut s_buf);
            lcd.draw_str_6x10(88, 14, s, false);
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
            let mut s_buf = [0u8; 12];
            let s = append_stale(mrxv_str, rxv_stale, &mut s_buf);
            lcd.draw_str_6x10(92, 44, s, false);
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
