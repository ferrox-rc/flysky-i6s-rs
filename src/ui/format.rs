//! Numeric and string formatters for zero-allocation menu display rendering.

/// Convert a byte buffer known to contain only valid ASCII bytes to a &str without runtime UTF-8 validation overhead.
///
/// # Safety
/// The caller must ensure that `buf` contains only valid UTF-8 sequences (ASCII is a subset of UTF-8).
#[inline(always)]
pub fn ascii_as_str(buf: &[u8]) -> &str {
    unsafe { core::str::from_utf8_unchecked(buf) }
}

/// Format an integer in 0..99 as two ASCII decimal digits into buf[0..2].
#[inline(always)]
pub fn write_dec2(val: u8, buf: &mut [u8]) {
    buf[0] = b'0' + (val / 10);
    buf[1] = b'0' + (val % 10);
}

const HEX_CHARS: &[u8; 16] = b"0123456789ABCDEF";

pub fn u32_to_hex(val: u32, buf: &mut [u8; 8]) {
    for i in 0..8 {
        buf[7 - i] = HEX_CHARS[((val >> (i * 4)) & 0x0F) as usize];
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SignMode {
    None,
    Force,
    Pad,
    LeadingSpace,
}

/// Unified fixed-point decimal and integer formatter for zero-allocation display rendering.
#[inline(never)]
pub fn format_fixed<'a>(
    val: i32,
    decimals: u8,
    pad_digits: u8,
    pad_zero: bool,
    sign_mode: SignMode,
    suffix: &str,
    buf: &'a mut [u8],
) -> &'a str {
    let mut pos = 0;
    let (mut n, is_neg) = if val < 0 {
        (val.unsigned_abs(), true)
    } else {
        (val as u32, false)
    };

    match sign_mode {
        SignMode::None => {
            if is_neg && pos < buf.len() {
                buf[pos] = b'-';
                pos += 1;
            }
        }
        SignMode::Force => {
            if pos < buf.len() {
                buf[pos] = if is_neg { b'-' } else { b'+' };
                pos += 1;
            }
        }
        SignMode::Pad => {
            if pos < buf.len() {
                buf[pos] = if is_neg {
                    b'-'
                } else if val > 0 {
                    b'+'
                } else {
                    b' '
                };
                pos += 1;
            }
        }
        SignMode::LeadingSpace => {
            if pos < buf.len() {
                buf[pos] = b' ';
                pos += 1;
            }
        }
    }

    let mut digits = [0u8; 10];
    let mut num_digits = 0;
    if n == 0 {
        digits[0] = 0;
        num_digits = 1;
    } else {
        while n > 0 && num_digits < digits.len() {
            digits[num_digits] = (n % 10) as u8;
            n /= 10;
            num_digits += 1;
        }
    }

    let min_digits = (decimals as usize) + 1;
    while num_digits < min_digits && num_digits < digits.len() {
        digits[num_digits] = 0;
        num_digits += 1;
    }

    let int_digits = num_digits - (decimals as usize);

    if (pad_digits as usize) > int_digits {
        let pad_char = if pad_zero { b'0' } else { b' ' };
        let pad_count = (pad_digits as usize) - int_digits;
        for _ in 0..pad_count {
            if pos < buf.len() {
                buf[pos] = pad_char;
                pos += 1;
            }
        }
    }

    for i in (decimals as usize..num_digits).rev() {
        if pos < buf.len() {
            buf[pos] = b'0' + digits[i];
            pos += 1;
        }
    }

    if decimals > 0 {
        if pos < buf.len() {
            buf[pos] = b'.';
            pos += 1;
        }
        for i in (0..decimals as usize).rev() {
            if pos < buf.len() {
                buf[pos] = b'0' + digits[i];
                pos += 1;
            }
        }
    }

    let s_bytes = suffix.as_bytes();
    let copy_len = s_bytes.len().min(buf.len() - pos);
    buf[pos..pos + copy_len].copy_from_slice(&s_bytes[..copy_len]);
    pos += copy_len;

    ascii_as_str(&buf[..pos])
}

pub fn u16_to_dec_4(val: u16, buf: &mut [u8; 4]) {
    format_fixed(val as i32, 0, 4, true, SignMode::None, "", buf);
}

/// Format 0..100 as percentage string "100%", " 50%", "  0%" into buf[0..4]
pub fn format_pct_3(val: u8, buf: &mut [u8; 4]) -> &str {
    format_fixed(val.min(100) as i32, 0, 3, false, SignMode::None, "%", buf)
}

/// Format 0..99 as two decimal digits "00".."99" into buf[0..2]
pub fn format_u8_2(val: u8, buf: &mut [u8; 2]) -> &str {
    format_fixed(val.min(99) as i32, 0, 2, true, SignMode::None, "", buf)
}

/// Format decivolts (e.g. 44 -> "4.4V") into buf[0..4]
pub fn format_deci_volt(deci: u8, buf: &mut [u8; 4]) -> &str {
    format_fixed(deci as i32, 1, 0, false, SignMode::None, "V", buf)
}

/// Format servo frequency (50..400 Hz) into buf[0..8] as "50 Hz" .. "400 Hz"
pub fn format_servo_hz(hz: u16, buf: &mut [u8; 8]) -> &str {
    format_fixed(hz.clamp(50, 400) as i32, 0, 0, false, SignMode::None, " Hz", buf)
}

pub fn next_ascii(c: u8) -> u8 {
    match c {
        b' ' => b'A',
        b'A'..=b'Y' => c + 1,
        b'Z' => b'0',
        b'0'..=b'8' => c + 1,
        b'9' => b'-',
        b'-' => b'_',
        _ => b' ',
    }
}

pub fn prev_ascii(c: u8) -> u8 {
    match c {
        b' ' => b'_',
        b'_' => b'-',
        b'-' => b'9',
        b'1'..=b'9' => c - 1,
        b'0' => b'Z',
        b'B'..=b'Z' => c - 1,
        b'A' => b' ',
        _ => b' ',
    }
}

pub const SOURCE_NAMES: [&str; 50] = [
    "None", "Roll", "Pitch", "Thr", "Yaw", "VRA", "VRB", "SA", "SB", "SC", "SD", "MAX", "CH1",
    "CH2", "CH3", "CH4", "CH5", "CH6", "CH7", "CH8", "CH9", "CH10", "CH11", "CH12", "CH13", "CH14",
    "CH15", "CH16", "CH17", "CH18", "Thr+", "SE", "SF", "SG", "SH", "SWI", "SWJ", "VRC", "VRD",
    "VRE", "VRF", "VRG", "SWK", "SWL", "SWM", "GV1", "GV2", "GV3", "GV4", "GV5",
];

pub const SWITCH_COND_NAMES: [&str; 29] = [
    "ON", "SA^", "SAv", "SB^", "SB-", "SBv", "SC^", "SC-", "SCv", "SD^", "SDv", "SE^", "SEv",
    "SF^", "SFv", "SG^", "SGv", "SH^", "SHv", "SWI^", "SWIv", "SWJ^", "SWJv", "SWK^", "SWKv",
    "SWL^", "SWLv", "SWM^", "SWMv",
];

pub const MODE_NAMES: [&str; 3] = ["ADD (+)", "MULT (*)", "REPL (:=)"];
pub const TEMPLATE_NAMES: [&str; 4] = ["NORMAL", "ELEVON/DELTA", "V-TAIL", "FLAPERON"];
pub const DR_SWITCH_NAMES: [&str; 7] = ["None", "SA", "SB", "SC", "SD", "SE", "SF"];
pub const AXIS_NAMES: [&str; 3] = ["Roll", "Pitch", "Yaw"];

pub const WEIGHT_SRC_NAMES: [&str; 42] = [
    "Fixed", "CH1", "CH2", "CH3", "CH4", "CH5", "CH6", "CH7", "CH8", "CH9", "CH10", "CH11", "CH12",
    "CH13", "CH14", "CH15", "CH16", "CH17", "CH18", "CH1+", "CH2+", "CH3+", "CH4+", "CH5+", "CH6+",
    "CH7+", "CH8+", "CH9+", "CH10+", "CH11+", "CH12+", "CH13+", "CH14+", "CH15+", "CH16+", "CH17+",
    "CH18+", "GV1", "GV2", "GV3", "GV4", "GV5",
];

pub fn i8_to_dec(val: i8, buf: &mut [u8; 6]) -> &str {
    format_fixed(val as i32, 0, 0, false, SignMode::Force, "%", buf)
}

pub fn u8_to_dec(val: u8, buf: &mut [u8; 5]) -> &str {
    format_fixed(val as i32, 0, 0, false, SignMode::None, "%", buf)
}

pub fn u32_to_dec_5(val: u32, buf: &mut [u8; 5]) {
    format_fixed(val.min(99999) as i32, 0, 5, false, SignMode::None, "", buf);
}

/// Format voltage in mV to "X.YYV" or "XX.YYV"
pub fn format_vbat(mv: u16, buf: &mut [u8; 6]) -> &str {
    format_fixed((mv / 10) as i32, 2, 0, false, SignMode::None, "V", buf)
}

/// Format timer in seconds to "MM:SS" or "+MM:SS" if expired.
pub fn format_timer(secs: u16, expired: bool, buf: &mut [u8; 8]) -> &str {
    let min = (secs / 60).min(99) as u8;
    let sec = (secs % 60) as u8;
    let mut i = 0;
    if expired {
        buf[i] = b'+';
        i += 1;
    }
    write_dec2(min, &mut buf[i..i + 2]);
    buf[i + 2] = b':';
    write_dec2(sec, &mut buf[i + 3..i + 5]);
    ascii_as_str(&buf[..i + 5])
}

/// Format stick value (-1000..+1000) to percentage string "-100%" .. "+100%"
pub fn format_percent(val: i16, buf: &mut [u8; 5]) -> &str {
    let pct = val as i32 / 10;
    format_fixed(pct, 0, 3, false, SignMode::Pad, "%", buf)
}

/// Format throttle value (-1000..+1000) to unipolar percentage string " 100%" .. "   0%"
pub fn format_throttle_percent(val: i16, buf: &mut [u8; 5]) -> &str {
    let pct = (((val as i32 + 1000) * 100) / 2000).clamp(0, 100);
    format_fixed(pct, 0, 3, false, SignMode::LeadingSpace, "%", buf)
}

/// Format active trim status to "TRM X:+00"
pub fn format_trim(axis: crate::trim::ActiveTrim, val: i8, buf: &mut [u8; 9]) -> &str {
    let name = match axis {
        crate::trim::ActiveTrim::Roll => b'A',
        crate::trim::ActiveTrim::Pitch => b'E',
        crate::trim::ActiveTrim::Throttle => b'T',
        crate::trim::ActiveTrim::Yaw => b'R',
        crate::trim::ActiveTrim::None => b' ',
    };
    buf[0..6].copy_from_slice(b"TRM  :");
    buf[4] = name;
    buf[6] = if val < 0 {
        b'-'
    } else if val > 0 {
        b'+'
    } else {
        b' '
    };
    write_dec2(val.unsigned_abs(), &mut buf[7..9]);
    ascii_as_str(buf)
}

/// Format an i32 to decimal ASCII representation in buf. Returns bytes written.
pub fn i32_to_dec(val: i32, buf: &mut [u8]) -> usize {
    format_fixed(val, 0, 0, false, SignMode::None, "", buf).len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_helpers() {
        let mut b4 = [0u8; 4];
        assert_eq!(format_pct_3(100, &mut b4), "100%");
        assert_eq!(format_pct_3(50, &mut b4), " 50%");
        assert_eq!(format_pct_3(5, &mut b4), "  5%");
        assert_eq!(format_pct_3(0, &mut b4), "  0%");

        let mut b2 = [0u8; 2];
        assert_eq!(format_u8_2(37, &mut b2), "37");
        assert_eq!(format_u8_2(0, &mut b2), "00");
        assert_eq!(format_u8_2(9, &mut b2), "09");

        assert_eq!(format_deci_volt(44, &mut b4), "4.4V");
        assert_eq!(format_deci_volt(50, &mut b4), "5.0V");
        assert_eq!(format_deci_volt(38, &mut b4), "3.8V");

        let mut b8 = [0u8; 8];
        assert_eq!(format_servo_hz(50, &mut b8), "50 Hz");
        assert_eq!(format_servo_hz(60, &mut b8), "60 Hz");
        assert_eq!(format_servo_hz(100, &mut b8), "100 Hz");
        assert_eq!(format_servo_hz(400, &mut b8), "400 Hz");
        assert_eq!(format_servo_hz(30, &mut b8), "50 Hz"); // clamped
        assert_eq!(format_servo_hz(500, &mut b8), "400 Hz"); // clamped

        let mut b16 = [0u8; 16];
        let len0 = i32_to_dec(0, &mut b16);
        assert_eq!(ascii_as_str(&b16[..len0]), "0");

        let len_pos = i32_to_dec(1234, &mut b16);
        assert_eq!(ascii_as_str(&b16[..len_pos]), "1234");

        let len_neg = i32_to_dec(-987, &mut b16);
        assert_eq!(ascii_as_str(&b16[..len_neg]), "-987");

        let mut b5 = [0u8; 5];
        assert_eq!(format_throttle_percent(1000, &mut b5), " 100%");
        assert_eq!(format_throttle_percent(0, &mut b5), "  50%");
        assert_eq!(format_throttle_percent(-1000, &mut b5), "   0%");
        assert_eq!(format_percent(1000, &mut b5), "+100%");
        assert_eq!(format_percent(0, &mut b5), "   0%");
        assert_eq!(format_percent(-1000, &mut b5), "-100%");
    }
}
