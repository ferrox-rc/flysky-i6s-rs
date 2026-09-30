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

pub fn u16_to_dec_4(val: u16, buf: &mut [u8; 4]) {
    buf[0] = b'0' + ((val / 1000) % 10) as u8;
    buf[1] = b'0' + ((val / 100) % 10) as u8;
    buf[2] = b'0' + ((val / 10) % 10) as u8;
    buf[3] = b'0' + (val % 10) as u8;
}

/// Format 0..100 as percentage string "100%", " 50%", "  0%" into buf[0..4]
pub fn format_pct_3(val: u8, buf: &mut [u8; 4]) -> &str {
    let v = val.min(100);
    if v == 100 {
        buf[0] = b'1';
        buf[1] = b'0';
        buf[2] = b'0';
    } else if v >= 10 {
        buf[0] = b' ';
        buf[1] = b'0' + (v / 10);
        buf[2] = b'0' + (v % 10);
    } else {
        buf[0] = b' ';
        buf[1] = b' ';
        buf[2] = b'0' + v;
    }
    buf[3] = b'%';
    ascii_as_str(buf)
}

/// Format 0..99 as two decimal digits "00".."99" into buf[0..2]
pub fn format_u8_2(val: u8, buf: &mut [u8; 2]) -> &str {
    let v = val.min(99);
    buf[0] = b'0' + (v / 10);
    buf[1] = b'0' + (v % 10);
    ascii_as_str(buf)
}

/// Format decivolts (e.g. 44 -> "4.4V") into buf[0..4]
pub fn format_deci_volt(deci: u8, buf: &mut [u8; 4]) -> &str {
    buf[0] = b'0' + (deci / 10);
    buf[1] = b'.';
    buf[2] = b'0' + (deci % 10);
    buf[3] = b'V';
    ascii_as_str(buf)
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

pub const SOURCE_NAMES: [&str; 31] = [
    "None", "Roll", "Pitch", "Thr", "Yaw", "VRA", "VRB", "SA", "SB", "SC", "SD", "MAX", "CH1",
    "CH2", "CH3", "CH4", "CH5", "CH6", "CH7", "CH8", "CH9", "CH10", "CH11", "CH12", "CH13", "CH14",
    "CH15", "CH16", "CH17", "CH18", "Thr+",
];

pub const SWITCH_COND_NAMES: [&str; 11] = [
    "ON", "SA^", "SAv", "SB^", "SB-", "SBv", "SC^", "SC-", "SCv", "SD^", "SDv",
];

pub const MODE_NAMES: [&str; 3] = ["ADD (+)", "MULT (*)", "REPL (:=)"];
pub const TEMPLATE_NAMES: [&str; 4] = ["NORMAL", "ELEVON/DELTA", "V-TAIL", "FLAPERON"];
pub const DR_SWITCH_NAMES: [&str; 5] = ["None", "SA", "SB", "SC", "SD"];
pub const AXIS_NAMES: [&str; 3] = ["Roll", "Pitch", "Yaw"];

pub fn i8_to_dec(val: i8, buf: &mut [u8; 6]) -> &str {
    let mut i = 0;
    let abs_val = if val < 0 {
        buf[i] = b'-';
        i += 1;
        (-val) as u8
    } else {
        buf[i] = b'+';
        i += 1;
        val as u8
    };
    if abs_val >= 100 {
        buf[i] = b'0' + (abs_val / 100);
        i += 1;
    }
    if abs_val >= 10 {
        buf[i] = b'0' + ((abs_val / 10) % 10);
        i += 1;
    }
    buf[i] = b'0' + (abs_val % 10);
    i += 1;
    buf[i] = b'%';
    i += 1;
    ascii_as_str(&buf[..i])
}

pub fn u8_to_dec(val: u8, buf: &mut [u8; 5]) -> &str {
    let mut i = 0;
    if val >= 100 {
        buf[i] = b'0' + (val / 100);
        i += 1;
    }
    if val >= 10 {
        buf[i] = b'0' + ((val / 10) % 10);
        i += 1;
    }
    buf[i] = b'0' + (val % 10);
    i += 1;
    buf[i] = b'%';
    i += 1;
    ascii_as_str(&buf[..i])
}

pub fn u32_to_dec_5(val: u32, buf: &mut [u8; 5]) {
    let v = val.min(99999);
    buf[0] = if v >= 10000 {
        b'0' + ((v / 10000) % 10) as u8
    } else {
        b' '
    };
    buf[1] = if v >= 1000 {
        b'0' + ((v / 1000) % 10) as u8
    } else {
        b' '
    };
    buf[2] = if v >= 100 {
        b'0' + ((v / 100) % 10) as u8
    } else {
        b' '
    };
    buf[3] = if v >= 10 {
        b'0' + ((v / 10) % 10) as u8
    } else {
        b' '
    };
    buf[4] = b'0' + (v % 10) as u8;
}

/// Format voltage in mV to "X.YYV" or "XX.YYV"
pub fn format_vbat(mv: u16, buf: &mut [u8; 6]) -> &str {
    let v = mv / 1000;
    let rem = mv % 1000;
    let d1 = rem / 100;
    let d2 = (rem % 100) / 10;

    if v >= 10 {
        buf[0] = b'0' + ((v / 10) as u8);
        buf[1] = b'0' + ((v % 10) as u8);
        buf[2] = b'.';
        buf[3] = b'0' + (d1 as u8);
        buf[4] = b'0' + (d2 as u8);
        buf[5] = b'V';
        ascii_as_str(buf)
    } else {
        buf[0] = b'0' + (v as u8);
        buf[1] = b'.';
        buf[2] = b'0' + (d1 as u8);
        buf[3] = b'0' + (d2 as u8);
        buf[4] = b'V';
        ascii_as_str(&buf[0..5])
    }
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
    buf[i] = b'0' + (min / 10);
    i += 1;
    buf[i] = b'0' + (min % 10);
    i += 1;
    buf[i] = b':';
    i += 1;
    buf[i] = b'0' + (sec / 10);
    i += 1;
    buf[i] = b'0' + (sec % 10);
    i += 1;
    ascii_as_str(&buf[..i])
}

/// Format stick value (-1000..+1000) to percentage string "-100%" .. "+100%"
pub fn format_percent(val: i16, buf: &mut [u8; 5]) -> &str {
    let pct = val / 10; // -100 .. +100
    let abs_pct = pct.unsigned_abs();

    if pct < 0 {
        buf[0] = b'-';
    } else if pct > 0 {
        buf[0] = b'+';
    } else {
        buf[0] = b' ';
    }

    if abs_pct >= 100 {
        buf[1] = b'1';
        buf[2] = b'0';
        buf[3] = b'0';
        buf[4] = b'%';
        ascii_as_str(buf)
    } else if abs_pct >= 10 {
        buf[1] = b' ';
        buf[2] = b'0' + ((abs_pct / 10) as u8);
        buf[3] = b'0' + ((abs_pct % 10) as u8);
        buf[4] = b'%';
        ascii_as_str(&buf[0..5])
    } else {
        buf[1] = b' ';
        buf[2] = b' ';
        buf[3] = b'0' + (abs_pct as u8);
        buf[4] = b'%';
        ascii_as_str(&buf[0..5])
    }
}

/// Format throttle value (-1000..+1000) to unipolar percentage string "  0%" .. "100%"
pub fn format_throttle_percent(val: i16, buf: &mut [u8; 5]) -> &str {
    let pct = (((val as i32 + 1000) * 100) / 2000).clamp(0, 100) as u8;

    if pct >= 100 {
        buf[0] = b'1';
        buf[1] = b'0';
        buf[2] = b'0';
        buf[3] = b'%';
        ascii_as_str(&buf[0..4])
    } else if pct >= 10 {
        buf[0] = b' ';
        buf[1] = b'0' + (pct / 10);
        buf[2] = b'0' + (pct % 10);
        buf[3] = b'%';
        ascii_as_str(&buf[0..4])
    } else {
        buf[0] = b' ';
        buf[1] = b' ';
        buf[2] = b'0' + pct;
        buf[3] = b'%';
        ascii_as_str(&buf[0..4])
    }
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
    buf[0] = b'T';
    buf[1] = b'R';
    buf[2] = b'M';
    buf[3] = b' ';
    buf[4] = name;
    buf[5] = b':';
    buf[6] = if val < 0 {
        b'-'
    } else if val > 0 {
        b'+'
    } else {
        b' '
    };
    let abs = val.unsigned_abs();
    buf[7] = b'0' + (abs / 10);
    buf[8] = b'0' + (abs % 10);
    ascii_as_str(buf)
}

/// Format an i32 to decimal ASCII representation in buf. Returns bytes written.
pub fn i32_to_dec(val: i32, buf: &mut [u8]) -> usize {
    if buf.is_empty() {
        return 0;
    }
    if val == 0 {
        buf[0] = b'0';
        return 1;
    }
    let mut i = 0;
    let (mut n, is_neg) = if val < 0 {
        (val.unsigned_abs(), true)
    } else {
        (val as u32, false)
    };
    if is_neg && i < buf.len() {
        buf[i] = b'-';
        i += 1;
    }
    let start_digit = i;
    while n > 0 && i < buf.len() {
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }
    // Reverse digits
    buf[start_digit..i].reverse();
    i
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

        let mut b16 = [0u8; 16];
        let len0 = i32_to_dec(0, &mut b16);
        assert_eq!(ascii_as_str(&b16[..len0]), "0");

        let len_pos = i32_to_dec(1234, &mut b16);
        assert_eq!(ascii_as_str(&b16[..len_pos]), "1234");

        let len_neg = i32_to_dec(-987, &mut b16);
        assert_eq!(ascii_as_str(&b16[..len_neg]), "-987");
    }
}
