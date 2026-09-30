//! CRSF / ExpressLRS protocol framing, channel encoding, and telemetry parser.
//!
//! Conforms to TBS Crossfire Protocol Specification Rev 08 & ExpressLRS implementation.

#![allow(dead_code)]

use crate::mixer::{CHANNEL_MAX_US, CHANNEL_MIN_US, CHANNEL_SPAN_US};

pub const CRSF_CHANNEL_MIN: u16 = 172;
pub const CRSF_CHANNEL_CENTER: u16 = 992;
pub const CRSF_CHANNEL_MAX: u16 = 1811;
pub const CRSF_CHANNEL_SPAN: u32 = (CRSF_CHANNEL_MAX - CRSF_CHANNEL_MIN) as u32; // 1639
pub const CRSF_CHANNEL_VALUE_MAX: u16 = 2047; // 11-bit maximum (0x07FF)

pub const CRSF_SYNC_BYTE: u8 = 0xC8;
pub const CRSF_ADDRESS_BROADCAST: u8 = 0x00;
pub const CRSF_ADDRESS_CLOUD: u8 = 0x0E;
pub const CRSF_ADDRESS_USB: u8 = 0x10;
pub const CRSF_ADDRESS_BLUETOOTH_WIFI: u8 = 0x12;
pub const CRSF_ADDRESS_WIFI_RECEIVER: u8 = 0x13;
pub const CRSF_ADDRESS_VIDEO_RECEIVER: u8 = 0x14;
pub const CRSF_ADDRESS_OSD: u8 = 0x80;
pub const CRSF_ADDRESS_ESC1: u8 = 0x90;
pub const CRSF_ADDRESS_ESC2: u8 = 0x91;
pub const CRSF_ADDRESS_ESC3: u8 = 0x92;
pub const CRSF_ADDRESS_ESC4: u8 = 0x93;
pub const CRSF_ADDRESS_ESC5: u8 = 0x94;
pub const CRSF_ADDRESS_ESC6: u8 = 0x95;
pub const CRSF_ADDRESS_ESC7: u8 = 0x96;
pub const CRSF_ADDRESS_ESC8: u8 = 0x97;
pub const CRSF_ADDRESS_CURRENT_SENSOR: u8 = 0xC0;
pub const CRSF_ADDRESS_GPS: u8 = 0xC2;
pub const CRSF_ADDRESS_BLACKBOX: u8 = 0xC4;
pub const CRSF_ADDRESS_FLIGHT_CONTROLLER: u8 = 0xC8;
pub const CRSF_ADDRESS_RACE_TAG: u8 = 0xCC;
pub const CRSF_ADDRESS_VTX: u8 = 0xCE;
pub const CRSF_ADDRESS_RADIO_TRANSMITTER: u8 = 0xEA;
pub const CRSF_ADDRESS_REPEATER_RECEIVER: u8 = 0xEB;
pub const CRSF_ADDRESS_CRSF_RECEIVER: u8 = 0xEC;
pub const CRSF_ADDRESS_REPEATER_TRANSMITTER: u8 = 0xED;
pub const CRSF_ADDRESS_CRSF_TRANSMITTER: u8 = 0xEE;

// Frame types
pub const CRSF_FRAMETYPE_GPS: u8 = 0x02;
pub const CRSF_FRAMETYPE_BATTERY_SENSOR: u8 = 0x08;
pub const CRSF_FRAMETYPE_HEARTBEAT: u8 = 0x0B;
pub const CRSF_FRAMETYPE_LINK_STATISTICS: u8 = 0x14;
pub const CRSF_FRAMETYPE_RC_CHANNELS_PACKED: u8 = 0x16;
pub const CRSF_FRAMETYPE_DEVICE_PING: u8 = 0x28;
pub const CRSF_FRAMETYPE_DEVICE_INFO: u8 = 0x29;
pub const CRSF_FRAMETYPE_PARAMETER_SETTINGS_ENTRY: u8 = 0x2B;
pub const CRSF_FRAMETYPE_PARAMETER_READ: u8 = 0x2C;
pub const CRSF_FRAMETYPE_PARAMETER_WRITE: u8 = 0x2D;
pub const CRSF_FRAMETYPE_ELRS_STATUS: u8 = 0x2E;

// Parameter data types
pub const CRSF_TYPE_UINT8: u8 = 0;
pub const CRSF_TYPE_INT8: u8 = 1;
pub const CRSF_TYPE_UINT16: u8 = 2;
pub const CRSF_TYPE_INT16: u8 = 3;
pub const CRSF_TYPE_FLOAT: u8 = 8;
pub const CRSF_TYPE_SELECT: u8 = 9;
pub const CRSF_TYPE_STRING: u8 = 10;
pub const CRSF_TYPE_FOLDER: u8 = 11;
pub const CRSF_TYPE_INFO: u8 = 12;
pub const CRSF_TYPE_COMMAND: u8 = 13;
pub const CRSF_TYPE_BACK: u8 = 14;

// Command statuses
pub const STATUS_READY: u8 = 0;
pub const STATUS_START: u8 = 1;
pub const STATUS_PROGRESS: u8 = 2;
pub const STATUS_CONFIRMATION_NEEDED: u8 = 3;
pub const STATUS_CONFIRM: u8 = 4;
pub const STATUS_CANCEL: u8 = 5;
pub const STATUS_POLL: u8 = 6;

pub const CRSF_FRAME_MAX_SIZE: usize = 64;
pub const CRSF_RC_FRAME_SIZE: usize = 26; // 1 (addr) + 1 (len) + 1 (type) + 22 (payload) + 1 (crc)

/// Incoming telemetry data decoded from CRSF link
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct CrsfTelemetry {
    pub connected: bool,
    pub uplink_rssi_1: i8,       // dBm (-130..0)
    pub uplink_rssi_2: i8,       // dBm (-130..0)
    pub uplink_link_quality: u8, // 0..100%
    pub uplink_snr: i8,          // dB (-30..30)
    pub active_antenna: u8,      // 0 or 1
    pub rf_mode: u8,             // ExpressLRS / CRSF rate mode index
    pub tx_power_mw: u16,        // Transmit power in milliwatts
    pub rx_battery_mv: u16,      // Flight pack voltage in millivolts
    pub rx_current_ma: u16,      // Current in 100mA units
    pub rx_capacity_mah: u32,    // Capacity consumed in mAh
    pub rx_battery_pct: u8,      // Battery remaining percentage (0..100)
    pub last_telemetry_ms: u32,  // System tick when last valid frame was received
}

impl CrsfTelemetry {
    pub const fn new() -> Self {
        Self {
            connected: false,
            uplink_rssi_1: 0,
            uplink_rssi_2: 0,
            uplink_link_quality: 0,
            uplink_snr: 0,
            active_antenna: 0,
            rf_mode: 0,
            tx_power_mw: 0,
            rx_battery_mv: 0,
            rx_current_ma: 0,
            rx_capacity_mah: 0,
            rx_battery_pct: 0,
            last_telemetry_ms: 0,
        }
    }
}

/// CRC8 lookup table using polynomial 0xD5 (DVB-S2).
static CRC8_TABLE: [u8; 256] = [
    0x00, 0xD5, 0x7F, 0xAA, 0xFE, 0x2B, 0x81, 0x54, 0x29, 0xFC, 0x56, 0x83, 0xD7, 0x02, 0xA8, 0x7D,
    0x52, 0x87, 0x2D, 0xF8, 0xAC, 0x79, 0xD3, 0x06, 0x7B, 0xAE, 0x04, 0xD1, 0x85, 0x50, 0xFA, 0x2F,
    0xA4, 0x71, 0xDB, 0x0E, 0x5A, 0x8F, 0x25, 0xF0, 0x8D, 0x58, 0xF2, 0x27, 0x73, 0xA6, 0x0C, 0xD9,
    0xF6, 0x23, 0x89, 0x5C, 0x08, 0xDD, 0x77, 0xA2, 0xDF, 0x0A, 0xA0, 0x75, 0x21, 0xF4, 0x5E, 0x8B,
    0x9D, 0x48, 0xE2, 0x37, 0x63, 0xB6, 0x1C, 0xC9, 0xB4, 0x61, 0xCB, 0x1E, 0x4A, 0x9F, 0x35, 0xE0,
    0xCF, 0x1A, 0xB0, 0x65, 0x31, 0xE4, 0x4E, 0x9B, 0xE6, 0x33, 0x99, 0x4C, 0x18, 0xCD, 0x67, 0xB2,
    0x39, 0xEC, 0x46, 0x93, 0xC7, 0x12, 0xB8, 0x6D, 0x10, 0xC5, 0x6F, 0xBA, 0xEE, 0x3B, 0x91, 0x44,
    0x6B, 0xBE, 0x14, 0xC1, 0x95, 0x40, 0xEA, 0x3F, 0x42, 0x97, 0x3D, 0xE8, 0xBC, 0x69, 0xC3, 0x16,
    0xEF, 0x3A, 0x90, 0x45, 0x11, 0xC4, 0x6E, 0xBB, 0xC6, 0x13, 0xB9, 0x6C, 0x38, 0xED, 0x47, 0x92,
    0xBD, 0x68, 0xC2, 0x17, 0x43, 0x96, 0x3C, 0xE9, 0x94, 0x41, 0xEB, 0x3E, 0x6A, 0xBF, 0x15, 0xC0,
    0x4B, 0x9E, 0x34, 0xE1, 0xB5, 0x60, 0xCA, 0x1F, 0x62, 0xB7, 0x1D, 0xC8, 0x9C, 0x49, 0xE3, 0x36,
    0x19, 0xCC, 0x66, 0xB3, 0xE7, 0x32, 0x98, 0x4D, 0x30, 0xE5, 0x4F, 0x9A, 0xCE, 0x1B, 0xB1, 0x64,
    0x72, 0xA7, 0x0D, 0xD8, 0x8C, 0x59, 0xF3, 0x26, 0x5B, 0x8E, 0x24, 0xF1, 0xA5, 0x70, 0xDA, 0x0F,
    0x20, 0xF5, 0x5F, 0x8A, 0xDE, 0x0B, 0xA1, 0x74, 0x09, 0xDC, 0x76, 0xA3, 0xF7, 0x22, 0x88, 0x5D,
    0xD6, 0x03, 0xA9, 0x7C, 0x28, 0xFD, 0x57, 0x82, 0xFF, 0x2A, 0x80, 0x55, 0x01, 0xD4, 0x7E, 0xAB,
    0x84, 0x51, 0xFB, 0x2E, 0x7A, 0xAF, 0x05, 0xD0, 0xAD, 0x78, 0xD2, 0x07, 0x53, 0x86, 0x2C, 0xF9,
];

/// Calculate CRC8-DVB over a slice of bytes.
pub fn crc8(data: &[u8]) -> u8 {
    let mut crc = 0u8;
    for &b in data {
        crc = CRC8_TABLE[(crc ^ b) as usize];
    }
    crc
}

/// Convert a microsecond pulse width (CHANNEL_MIN_US..CHANNEL_MAX_US, center CHANNEL_CENTER_US)
/// into standard 11-bit CRSF channel counts (CRSF_CHANNEL_MIN..CRSF_CHANNEL_MAX).
#[inline(always)]
pub fn us_to_crsf(us: u16) -> u16 {
    let clamped = us.clamp(CHANNEL_MIN_US, CHANNEL_MAX_US) as i32;
    // Standard CRSF 11-bit channel scaling:
    // Scale = (CRSF_CHANNEL_MAX - CRSF_CHANNEL_MIN) / (CHANNEL_MAX_US - CHANNEL_MIN_US) = 1639 / 1024
    let val = (((clamped - CHANNEL_MIN_US as i32) * CRSF_CHANNEL_SPAN as i32
        + (CHANNEL_SPAN_US as i32 / 2))
        / CHANNEL_SPAN_US as i32)
        + CRSF_CHANNEL_MIN as i32;
    val.clamp(0, CRSF_CHANNEL_VALUE_MAX as i32) as u16
}

/// Build a packed 16-channel CRSF RC packet (Type 0x16) into `out_frame`.
/// Returns the number of bytes written (always 26 bytes).
pub fn build_channels_frame(
    channels: &[u16; crate::mixer::NUM_CHANNELS],
    out_frame: &mut [u8; CRSF_RC_FRAME_SIZE],
) -> usize {
    // Header
    out_frame[0] = CRSF_SYNC_BYTE; // 0xC8 per TBS CRSF spec
    out_frame[1] = 24; // Length: Type (1) + Payload (22) + CRC (1) = 24
    out_frame[2] = CRSF_FRAMETYPE_RC_CHANNELS_PACKED; // 0x16

    // Convert first 16 radio channels to 16 CRSF 11-bit values (channels 17 & 18 ignored by CRSF 16-ch packet)
    let mut ch11 = [CRSF_CHANNEL_CENTER; 16];
    for i in 0..16 {
        ch11[i] = us_to_crsf(channels[i]);
    }

    // 16 channels * 11 bits = 176 bits = 22 bytes
    let p = &mut out_frame[3..25];
    p[0] = (ch11[0] & 0x07FF) as u8;
    p[1] = ((ch11[0] >> 8) | (ch11[1] << 3)) as u8;
    p[2] = ((ch11[1] >> 5) | (ch11[2] << 6)) as u8;
    p[3] = (ch11[2] >> 2) as u8;
    p[4] = ((ch11[2] >> 10) | (ch11[3] << 1)) as u8;
    p[5] = ((ch11[3] >> 7) | (ch11[4] << 4)) as u8;
    p[6] = ((ch11[4] >> 4) | (ch11[5] << 7)) as u8;
    p[7] = (ch11[5] >> 1) as u8;
    p[8] = ((ch11[5] >> 9) | (ch11[6] << 2)) as u8;
    p[9] = ((ch11[6] >> 6) | (ch11[7] << 5)) as u8;
    p[10] = (ch11[7] >> 3) as u8;
    p[11] = (ch11[8] & 0x07FF) as u8;
    p[12] = ((ch11[8] >> 8) | (ch11[9] << 3)) as u8;
    p[13] = ((ch11[9] >> 5) | (ch11[10] << 6)) as u8;
    p[14] = (ch11[10] >> 2) as u8;
    p[15] = ((ch11[10] >> 10) | (ch11[11] << 1)) as u8;
    p[16] = ((ch11[11] >> 7) | (ch11[12] << 4)) as u8;
    p[17] = ((ch11[12] >> 4) | (ch11[13] << 7)) as u8;
    p[18] = (ch11[13] >> 1) as u8;
    p[19] = ((ch11[13] >> 9) | (ch11[14] << 2)) as u8;
    p[20] = ((ch11[14] >> 6) | (ch11[15] << 5)) as u8;
    p[21] = (ch11[15] >> 3) as u8;

    // CRC8 is calculated over Type (byte 2) and Payload (bytes 3..25)
    out_frame[25] = crc8(&out_frame[2..25]);

    CRSF_RC_FRAME_SIZE
}

/// Parse incoming telemetry frame and update telemetry state.
pub fn parse_telemetry_frame(frame: &[u8], telem: &mut CrsfTelemetry, now_ms: u32) -> bool {
    if frame.len() < 4 {
        return false;
    }
    let frame_len = frame[1] as usize;
    if frame_len + 2 > frame.len() || frame_len < 2 {
        return false;
    }

    // Check CRC
    let expected_crc = frame[1 + frame_len];
    let calc = crc8(&frame[2..1 + frame_len]);
    if expected_crc != calc {
        return false;
    }

    let frame_type = frame[2];
    let payload = &frame[3..1 + frame_len];

    match frame_type {
        CRSF_FRAMETYPE_LINK_STATISTICS => {
            if payload.len() >= 10 {
                telem.uplink_rssi_1 = -(payload[0] as i8);
                telem.uplink_rssi_2 = -(payload[1] as i8);
                telem.uplink_link_quality = payload[2];
                telem.uplink_snr = payload[3] as i8;
                telem.active_antenna = payload[4];
                telem.rf_mode = payload[5];
                let pwr_code = payload[6];
                telem.tx_power_mw = match pwr_code {
                    0 => 0,
                    1 => 10,
                    2 => 25,
                    3 => 100,
                    4 => 500,
                    5 => 1000,
                    6 => 2000,
                    7 => 250,
                    8 => 50,
                    _ => 0,
                };
                telem.connected = true;
                telem.last_telemetry_ms = now_ms;
                return true;
            }
        }
        CRSF_FRAMETYPE_BATTERY_SENSOR if payload.len() >= 8 => {
            // Voltage in 0.1V units (big endian)
            let v_deci = u16::from_be_bytes([payload[0], payload[1]]);
            telem.rx_battery_mv = v_deci * 100;
            let c_deci = u16::from_be_bytes([payload[2], payload[3]]);
            telem.rx_current_ma = c_deci * 100;
            let cap =
                ((payload[4] as u32) << 16) | ((payload[5] as u32) << 8) | (payload[6] as u32);
            telem.rx_capacity_mah = cap;
            telem.rx_battery_pct = payload[7];
            telem.connected = true;
            telem.last_telemetry_ms = now_ms;
            return true;
        }
        _ => {}
    }

    false
}

/// Convert ExpressLRS RF mode index to standard readable packet rate string.
pub fn rf_mode_to_str(rf_mode: u8) -> &'static str {
    match rf_mode {
        0 => "4Hz",
        1 => "25Hz",
        2 => "50Hz",
        3 => "100Hz",
        4 => "100F",
        5 => "150Hz",
        6 => "200Hz",
        7 => "250Hz",
        8 => "333Hz",
        9 => "500Hz",
        10 => "D250",
        11 => "D500",
        12 => "F500",
        13 => "F1000",
        _ => "---",
    }
}

/// Convert CRSF device physical address to short role tag (e.g. TX, RX, FC, VTX, VRX, OSD, GPS, ESC, etc.).
/// Returns `None` if the address is not a standard known item.
pub fn device_role_str(addr: u8) -> Option<&'static str> {
    match addr {
        CRSF_ADDRESS_CRSF_TRANSMITTER => Some("TX"),
        CRSF_ADDRESS_CRSF_RECEIVER => Some("RX"),
        CRSF_ADDRESS_FLIGHT_CONTROLLER => Some("FC"),
        CRSF_ADDRESS_VTX => Some("VTX"),
        CRSF_ADDRESS_VIDEO_RECEIVER => Some("VRX"),
        CRSF_ADDRESS_OSD => Some("OSD"),
        CRSF_ADDRESS_GPS => Some("GPS"),
        CRSF_ADDRESS_CURRENT_SENSOR => Some("PWR"),
        CRSF_ADDRESS_BLACKBOX => Some("BOX"),
        CRSF_ADDRESS_BLUETOOTH_WIFI => Some("WIFI"),
        CRSF_ADDRESS_WIFI_RECEIVER => Some("WIRX"),
        CRSF_ADDRESS_USB => Some("USB"),
        CRSF_ADDRESS_CLOUD => Some("NET"),
        CRSF_ADDRESS_RACE_TAG => Some("TAG"),
        CRSF_ADDRESS_RADIO_TRANSMITTER => Some("RC"),
        CRSF_ADDRESS_REPEATER_RECEIVER => Some("RPRX"),
        CRSF_ADDRESS_REPEATER_TRANSMITTER => Some("RPTX"),
        CRSF_ADDRESS_ESC1 => Some("ESC1"),
        CRSF_ADDRESS_ESC2 => Some("ESC2"),
        CRSF_ADDRESS_ESC3 => Some("ESC3"),
        CRSF_ADDRESS_ESC4 => Some("ESC4"),
        CRSF_ADDRESS_ESC5 => Some("ESC5"),
        CRSF_ADDRESS_ESC6 => Some("ESC6"),
        CRSF_ADDRESS_ESC7 => Some("ESC7"),
        CRSF_ADDRESS_ESC8 => Some("ESC8"),
        _ => None,
    }
}

/// Build a Device Ping frame (0x28) to discover connected CRSF/ELRS modules.
/// Wire frame format per TBS spec: [Sync (0xC8)] [Len (4)] [Type (0x28)] [Dest (0x00)] [Orig (0xEA)] [CRC]
pub fn build_ping_frame(out_frame: &mut [u8]) -> usize {
    out_frame[0] = CRSF_SYNC_BYTE;
    out_frame[1] = 4; // Type (1) + Payload (2) + CRC (1)
    out_frame[2] = CRSF_FRAMETYPE_DEVICE_PING;
    out_frame[3] = CRSF_ADDRESS_BROADCAST;
    out_frame[4] = CRSF_ADDRESS_RADIO_TRANSMITTER;
    out_frame[5] = crc8(&out_frame[2..5]);
    6
}

/// Build an Extended Parameter frame (Read 0x2C or Write 0x2D).
/// Wire frame format per TBS spec: [Sync (0xC8)] [Len (6)] [Type] [Dest (Target)] [Orig (0xEA)] [Param] [Payload] [CRC]
pub fn build_param_ext_frame(
    target: u8,
    frame_type: u8,
    param_id: u8,
    val_or_chunk: u8,
    out_frame: &mut [u8],
) -> usize {
    out_frame[0] = CRSF_SYNC_BYTE;
    out_frame[1] = 6; // Type (1) + Dest (1) + Orig (1) + Param (1) + Value/Chunk (1) + CRC (1) = 6
    out_frame[2] = frame_type;
    out_frame[3] = target;
    out_frame[4] = CRSF_ADDRESS_RADIO_TRANSMITTER;
    out_frame[5] = param_id;
    out_frame[6] = val_or_chunk;
    out_frame[7] = crc8(&out_frame[2..7]);
    8
}

/// Build a Parameter Read frame (0x2C) requesting metadata/options for `param_id`.
#[inline]
pub fn build_param_read_frame(target: u8, param_id: u8, chunk: u8, out_frame: &mut [u8]) -> usize {
    build_param_ext_frame(
        target,
        CRSF_FRAMETYPE_PARAMETER_READ,
        param_id,
        chunk,
        out_frame,
    )
}

/// Build a Parameter Write frame (0x2D) updating `param_id` value or command status (single byte).
#[inline]
pub fn build_param_write_frame(target: u8, param_id: u8, value: u8, out_frame: &mut [u8]) -> usize {
    build_param_ext_frame(
        target,
        CRSF_FRAMETYPE_PARAMETER_WRITE,
        param_id,
        value,
        out_frame,
    )
}

/// Build a Parameter Write frame (0x2D) with arbitrary payload (e.g. multi-byte integers).
pub fn build_param_write_frame_multi(
    target: u8,
    param_id: u8,
    payload: &[u8],
    out_frame: &mut [u8],
) -> usize {
    out_frame[0] = CRSF_SYNC_BYTE;
    out_frame[1] = (5 + payload.len()) as u8; // Type (1) + Dest (1) + Orig (1) + Param (1) + Payload (len) + CRC (1)
    out_frame[2] = CRSF_FRAMETYPE_PARAMETER_WRITE;
    out_frame[3] = target;
    out_frame[4] = CRSF_ADDRESS_RADIO_TRANSMITTER;
    out_frame[5] = param_id;
    let end = 6 + payload.len();
    out_frame[6..end].copy_from_slice(payload);
    out_frame[end] = crc8(&out_frame[2..end]);
    end + 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mixer::CHANNEL_CENTER_US;

    #[test]
    fn test_crc8_dvb() {
        assert_eq!(crc8(&[0x00]), 0x00);
        // Test CRC repeatability
        let sample = [0x16, 0x01, 0x02, 0x03, 0x04];
        let c1 = crc8(&sample);
        let c2 = crc8(&sample);
        assert_eq!(c1, c2);
        assert_ne!(c1, 0);
    }

    #[test]
    fn test_us_to_crsf_scaling() {
        assert_eq!(us_to_crsf(CHANNEL_MIN_US), CRSF_CHANNEL_MIN); // 988 -> 172
        assert_eq!(us_to_crsf(CHANNEL_CENTER_US), CRSF_CHANNEL_CENTER); // 1500 -> 992
        assert_eq!(us_to_crsf(CHANNEL_MAX_US), CRSF_CHANNEL_MAX); // 2012 -> 1811

        // Clamping bounds
        assert_eq!(us_to_crsf(800), CRSF_CHANNEL_MIN);
        assert_eq!(us_to_crsf(2200), CRSF_CHANNEL_MAX);
    }

    #[test]
    fn test_build_channels_frame_and_unpack() {
        let channels: [u16; crate::mixer::NUM_CHANNELS] = [
            CHANNEL_CENTER_US, // CH1 1500 -> 992
            CHANNEL_MIN_US,    // CH2 988  -> 172
            CHANNEL_MAX_US,    // CH3 2012 -> 1811
            1200,              // CH4
            1800,              // CH5
            1400,              // CH6
            1600,              // CH7
            1500,              // CH8
            988,               // CH9
            2012,              // CH10
            1500,              // CH11
            1500,              // CH12
            1500,              // CH13
            1500,              // CH14
            1750,              // CH15
            1250,              // CH16
            1900,              // CH17 (ignored by CRSF 16ch)
            1100,              // CH18 (ignored by CRSF 16ch)
        ];

        let mut frame = [0u8; CRSF_RC_FRAME_SIZE];
        let len = build_channels_frame(&channels, &mut frame);
        assert_eq!(len, CRSF_RC_FRAME_SIZE);

        // Header check
        assert_eq!(frame[0], CRSF_SYNC_BYTE);
        assert_eq!(frame[1], 24);
        assert_eq!(frame[2], CRSF_FRAMETYPE_RC_CHANNELS_PACKED);

        // CRC check
        assert_eq!(frame[25], crc8(&frame[2..25]));

        // Unpack channels from 22 bytes (bits 0..176)
        let p = &frame[3..25];
        let mut unpacked = [0u16; 16];
        unpacked[0] = ((p[0] as u16) | ((p[1] as u16) << 8)) & 0x07FF;
        unpacked[1] = (((p[1] as u16) >> 3) | ((p[2] as u16) << 5)) & 0x07FF;
        unpacked[2] =
            (((p[2] as u16) >> 6) | ((p[3] as u16) << 2) | ((p[4] as u16) << 10)) & 0x07FF;
        unpacked[3] = (((p[4] as u16) >> 1) | ((p[5] as u16) << 7)) & 0x07FF;
        unpacked[4] = (((p[5] as u16) >> 4) | ((p[6] as u16) << 4)) & 0x07FF;
        unpacked[5] = (((p[6] as u16) >> 7) | ((p[7] as u16) << 1) | ((p[8] as u16) << 9)) & 0x07FF;
        unpacked[6] = (((p[8] as u16) >> 2) | ((p[9] as u16) << 6)) & 0x07FF;
        unpacked[7] = ((p[9] as u16) >> 5) | ((p[10] as u16) << 3) & 0x07FF;
        unpacked[14] = (((p[19] as u16) >> 2) | ((p[20] as u16) << 6)) & 0x07FF;
        unpacked[15] = (((p[20] as u16) >> 5) | ((p[21] as u16) << 3)) & 0x07FF;

        // Verify unpacked values match expected CRSF counts
        assert_eq!(unpacked[0], us_to_crsf(channels[0]));
        assert_eq!(unpacked[1], us_to_crsf(channels[1]));
        assert_eq!(unpacked[2], us_to_crsf(channels[2]));
        assert_eq!(unpacked[3], us_to_crsf(channels[3]));
        assert_eq!(unpacked[14], us_to_crsf(channels[14]));
        assert_eq!(unpacked[15], us_to_crsf(channels[15]));
    }

    #[test]
    fn test_parse_telemetry_link_statistics() {
        let mut telem = CrsfTelemetry::new();
        // Construct Link Statistics frame:
        // [addr=0xEA, len=12, type=0x14, rssi1=80 (-80dBm), rssi2=85 (-85dBm), lq=99, snr=10, ant=0, rf_mode=2, pwr=3 (100mW), ...]
        let mut frame = [0u8; 14];
        frame[0] = CRSF_ADDRESS_RADIO_TRANSMITTER;
        frame[1] = 12; // type + 10 bytes payload + crc
        frame[2] = CRSF_FRAMETYPE_LINK_STATISTICS;
        frame[3] = 80; // RSSI1: 80 -> -80 dBm
        frame[4] = 85; // RSSI2: 85 -> -85 dBm
        frame[5] = 99; // LQ: 99%
        frame[6] = 10; // SNR: +10 dB
        frame[7] = 0; // Antenna 0
        frame[8] = 2; // RF Mode 2
        frame[9] = 3; // Power code 3 -> 100 mW
        frame[10] = 0;
        frame[11] = 0;
        frame[12] = 0;
        frame[13] = crc8(&frame[2..13]);

        let success = parse_telemetry_frame(&frame, &mut telem, 1000);
        assert!(success);
        assert_eq!(telem.uplink_rssi_1, -80);
        assert_eq!(telem.uplink_rssi_2, -85);
        assert_eq!(telem.uplink_link_quality, 99);
        assert_eq!(telem.uplink_snr, 10);
        assert_eq!(telem.tx_power_mw, 100);

        // Corrupt CRC and verify rejection
        frame[13] ^= 0xFF;
        let fail = parse_telemetry_frame(&frame, &mut telem, 2000);
        assert!(!fail);
    }

    #[test]
    fn test_parse_telemetry_battery_sensor() {
        let mut telem = CrsfTelemetry::new();
        // Battery Sensor frame: [addr=0xEA, len=10, type=0x08, v=126 (12.6V), c=35 (3.5A), cap=1500 (0x0005DC), pct=85, crc]
        let mut frame = [0u8; 12];
        frame[0] = CRSF_ADDRESS_RADIO_TRANSMITTER;
        frame[1] = 10; // type(1) + 8 payload + crc(1)
        frame[2] = CRSF_FRAMETYPE_BATTERY_SENSOR;
        frame[3] = 0; // Voltage high byte (126 = 0x007E)
        frame[4] = 126; // Voltage low byte -> 126 * 100 = 12,600 mV
        frame[5] = 0; // Current high byte (35 = 0x0023)
        frame[6] = 35; // Current low byte -> 35 * 100 = 3,500 mA
        frame[7] = 0x00; // Capacity byte 2 (1500 = 0x0005DC)
        frame[8] = 0x05; // Capacity byte 1
        frame[9] = 0xDC; // Capacity byte 0 -> 1500 mAh
        frame[10] = 85; // 85% remaining
        frame[11] = crc8(&frame[2..11]);

        let success = parse_telemetry_frame(&frame, &mut telem, 1500);
        assert!(success);
        assert_eq!(telem.rx_battery_mv, 12600);
        assert_eq!(telem.rx_current_ma, 3500);
        assert_eq!(telem.rx_capacity_mah, 1500);
        assert_eq!(telem.rx_battery_pct, 85);
        assert!(telem.connected);
        assert_eq!(telem.last_telemetry_ms, 1500);

        // Truncated payload (< 8 bytes)
        let mut short_frame = [0u8; 8];
        short_frame[0] = CRSF_ADDRESS_RADIO_TRANSMITTER;
        short_frame[1] = 6;
        short_frame[2] = CRSF_FRAMETYPE_BATTERY_SENSOR;
        short_frame[7] = crc8(&short_frame[2..7]);
        assert!(!parse_telemetry_frame(&short_frame, &mut telem, 1600));
    }

    #[test]
    fn test_build_ping_frame_wire_spec() {
        let mut buf = [0u8; 8];
        let len = build_ping_frame(&mut buf);
        assert_eq!(len, 6);
        assert_eq!(buf[0], CRSF_SYNC_BYTE); // 0xC8 per TBS CRSF spec
        assert_eq!(buf[1], 4);
        assert_eq!(buf[2], CRSF_FRAMETYPE_DEVICE_PING); // 0x28
        assert_eq!(buf[3], CRSF_ADDRESS_BROADCAST); // 0x00
        assert_eq!(buf[4], CRSF_ADDRESS_RADIO_TRANSMITTER); // 0xEA
        assert_eq!(buf[5], 0x54, "Ping CRC8 over [0x28, 0x00, 0xEA] is 0x54");
    }

    #[test]
    fn test_build_param_read_and_write_frames() {
        let mut read_buf = [0u8; 8];
        let read_len = build_param_read_frame(CRSF_ADDRESS_CRSF_TRANSMITTER, 3, 1, &mut read_buf);
        assert_eq!(read_len, 8);
        assert_eq!(read_buf[0], CRSF_SYNC_BYTE);
        assert_eq!(read_buf[1], 6);
        assert_eq!(read_buf[2], CRSF_FRAMETYPE_PARAMETER_READ);
        assert_eq!(read_buf[3], CRSF_ADDRESS_CRSF_TRANSMITTER);
        assert_eq!(read_buf[4], CRSF_ADDRESS_RADIO_TRANSMITTER);
        assert_eq!(read_buf[5], 3); // param_id
        assert_eq!(read_buf[6], 1); // chunk
        assert_eq!(read_buf[7], crc8(&read_buf[2..7]));

        let mut write_buf = [0u8; 8];
        let write_len =
            build_param_write_frame(CRSF_ADDRESS_CRSF_TRANSMITTER, 3, 42, &mut write_buf);
        assert_eq!(write_len, 8);
        assert_eq!(write_buf[0], CRSF_SYNC_BYTE);
        assert_eq!(write_buf[2], CRSF_FRAMETYPE_PARAMETER_WRITE);
        assert_eq!(write_buf[5], 3);
        assert_eq!(write_buf[6], 42); // value
        assert_eq!(write_buf[7], crc8(&write_buf[2..7]));
    }

    #[test]
    fn test_rf_mode_strings() {
        assert_eq!(rf_mode_to_str(0), "4Hz");
        assert_eq!(rf_mode_to_str(2), "50Hz");
        assert_eq!(rf_mode_to_str(3), "100Hz");
        assert_eq!(rf_mode_to_str(7), "250Hz");
        assert_eq!(rf_mode_to_str(9), "500Hz");
        assert_eq!(rf_mode_to_str(13), "F1000");
        assert_eq!(rf_mode_to_str(99), "---");
    }

    #[test]
    fn test_dynamic_target_addressing() {
        let mut buf = [0u8; 8];
        // Target 1: ELRS Receiver (0xEC)
        build_param_read_frame(CRSF_ADDRESS_CRSF_RECEIVER, 1, 0, &mut buf);
        assert_eq!(
            buf[0], CRSF_SYNC_BYTE,
            "Wire sync byte must always be 0xC8 per TBS CRSF spec"
        );
        assert_eq!(
            buf[3], CRSF_ADDRESS_CRSF_RECEIVER,
            "Payload destination must match target (0xEC)"
        );

        // Target 2: Flight Controller (0xC8)
        build_param_write_frame(CRSF_ADDRESS_FLIGHT_CONTROLLER, 2, 99, &mut buf);
        assert_eq!(
            buf[0], CRSF_SYNC_BYTE,
            "Wire sync byte must always be 0xC8 per TBS CRSF spec"
        );
        assert_eq!(
            buf[3], CRSF_ADDRESS_FLIGHT_CONTROLLER,
            "Payload destination must match target (0xC8)"
        );
    }
}
