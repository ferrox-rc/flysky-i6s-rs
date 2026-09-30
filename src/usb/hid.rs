//! USB HID Gamepad / Joystick driver for FlySky FS-i6X.
//!
//! Exposes an 8-axis, 16-button standard USB Gamepad compatible with all FPV
//! and RC flight simulators (Liftoff, Velocidrone, RealFlight, AccuRC, DCL, etc.).

use crate::input::{SwitchPos, Switches};
use crate::mixer::{CHANNEL_MAX_US, CHANNEL_MIN_US, CHANNEL_SPAN_US, NUM_CHANNELS};

pub const USB_AXIS_MIN: u16 = 0;
pub const USB_AXIS_MAX: u16 = 2047; // 11-bit endpoint
pub const AUX_BUTTON_THRESHOLD_US: u16 = 1750; // Threshold for high-channel logical button triggers

/// USB HID Report Descriptor for an 8-Axis, 16-Button Joystick (18 bytes input report).
/// Matches OpenI6X and EdgeTX 11-bit simulator conventions.
pub const GAMEPAD_REPORT_DESC: &[u8] = &[
    0x05, 0x01,        // Usage Page (Generic Desktop Ctrls)
    0x09, 0x04,        // Usage (Joystick)
    0xA1, 0x01,        // Collection (Application)
    0xA1, 0x00,        //   Collection (Physical)
    0x05, 0x09,        //     Usage Page (Button)
    0x19, 0x01,        //     Usage Minimum (Button 1)
    0x29, 0x10,        //     Usage Maximum (Button 16)
    0x15, 0x00,        //     Logical Minimum (0)
    0x25, 0x01,        //     Logical Maximum (1)
    0x95, 0x10,        //     Report Count (16)
    0x75, 0x01,        //     Report Size (1)
    0x81, 0x02,        //     Input (Data,Var,Abs) - 2 bytes buttons
    0x05, 0x01,        //     Usage Page (Generic Desktop Ctrls)
    // EdgeTX Classic 8-Axis Standard:
    0x09, 0x30,        //     Usage (X)      - CH1 Roll / Aileron
    0x09, 0x31,        //     Usage (Y)      - CH2 Pitch / Elevator
    0x09, 0x32,        //     Usage (Z)      - CH3 Throttle
    0x09, 0x33,        //     Usage (Rx)     - CH4 Yaw / Rudder
    0x09, 0x34,        //     Usage (Ry)     - CH5 Aux 1 (SA)
    0x09, 0x35,        //     Usage (Rz)     - CH6 Aux 2 (SB)
    0x09, 0x37,        //     Usage (Dial)   - CH7 Pot VRA
    0x09, 0x36,        //     Usage (Slider) - CH8 Pot VRB
    0x16, 0x00, 0x00,  //     Logical Minimum (0)
    0x26, 0xFF, 0x07,  //     Logical Maximum (2047) - 11-bit resolution
    0x75, 0x10,        //     Report Size (16)
    0x95, 0x08,        //     Report Count (8)
    0x81, 0x02,        //     Input (Data,Var,Abs) - 8 x 16-bit axes = 16 bytes
    0xC0,              //   End Collection
    0xC0,              // End Collection
];

pub const REPORT_SIZE: usize = 18;

/// Map a microsecond channel pulse width (CHANNEL_MIN_US..CHANNEL_MAX_US) to 11-bit USB axis value (USB_AXIS_MIN..USB_AXIS_MAX).
#[inline(always)]
fn us_to_axis(us: u16) -> u16 {
    let clamped = us.clamp(CHANNEL_MIN_US, CHANNEL_MAX_US) as u32;
    let span = CHANNEL_SPAN_US;
    let axis = (((clamped - CHANNEL_MIN_US as u32) * (USB_AXIS_MAX - USB_AXIS_MIN) as u32 + (span / 2)) / span) as u16 + USB_AXIS_MIN;
    axis.clamp(USB_AXIS_MIN, USB_AXIS_MAX)
}

/// Build standard 18-byte HID Joystick report matching EdgeTX/OpenTX conventions:
/// - 8 axes directly map output channels CH1..CH8 (11-bit 0..2047)
/// - 16 buttons map physical switch states and high channels without overlapping/double-triggering:
///   - Button 1: Switch SA (Down)
///   - Button 2: Switch SB (Mid)
///   - Button 3: Switch SB (Down)
///   - Button 4: Switch SC (Mid)
///   - Button 5: Switch SC (Down)
///   - Button 6: Switch SD (Down)
///   - Buttons 7..10: Channels 11..14 (> AUX_BUTTON_THRESHOLD_US for auxiliary functions)
pub fn build_gamepad_report(
    rf_chs: &[u16; NUM_CHANNELS],
    switches: &Switches,
    out: &mut [u8; REPORT_SIZE],
) {
    // 1. Digital Buttons (16 bits)
    let mut btns = 0u16;

    // Physical switches mapped directly to dedicated discrete buttons:
    // Neutral state (all switches UP) = all buttons released (0)
    if switches.sa == SwitchPos::Down {
        btns |= 1 << 0; // Button 1
    }
    if switches.sb == SwitchPos::Mid {
        btns |= 1 << 1; // Button 2
    }
    if switches.sb == SwitchPos::Down {
        btns |= 1 << 2; // Button 3
    }
    if switches.sc == SwitchPos::Mid {
        btns |= 1 << 3; // Button 4
    }
    if switches.sc == SwitchPos::Down {
        btns |= 1 << 4; // Button 5
    }
    if switches.sd == SwitchPos::Down {
        btns |= 1 << 5; // Button 6
    }

    // Buttons 7..10: Channels 11..14 (for any extra logical mixer triggers, > AUX_BUTTON_THRESHOLD_US)
    // Only Channels 11..14 to prevent double-reporting SC (CH9) or SD (CH10)
    for (i, &pulse) in rf_chs[10..14].iter().enumerate() {
        if pulse > AUX_BUTTON_THRESHOLD_US {
            btns |= 1 << (6 + i);
        }
    }

    out[0] = (btns & 0xFF) as u8;
    out[1] = ((btns >> 8) & 0xFF) as u8;

    // 2. Analog Axes (8 x 16-bit, little endian) - 1:1 with CH1..CH8
    // Axis 1 (X):      CH1 (Roll / Aileron)
    let a1 = us_to_axis(rf_chs[0]);
    out[2] = (a1 & 0xFF) as u8;
    out[3] = ((a1 >> 8) & 0xFF) as u8;

    // Axis 2 (Y):      CH2 (Pitch / Elevator)
    let a2 = us_to_axis(rf_chs[1]);
    out[4] = (a2 & 0xFF) as u8;
    out[5] = ((a2 >> 8) & 0xFF) as u8;

    // Axis 3 (Z):      CH3 (Throttle)
    let a3 = us_to_axis(rf_chs[2]);
    out[6] = (a3 & 0xFF) as u8;
    out[7] = ((a3 >> 8) & 0xFF) as u8;

    // Axis 4 (Rx):     CH4 (Yaw / Rudder)
    let a4 = us_to_axis(rf_chs[3]);
    out[8] = (a4 & 0xFF) as u8;
    out[9] = ((a4 >> 8) & 0xFF) as u8;

    // Axis 5 (Ry):     CH5 (Aux 1 / Switch SA)
    let a5 = us_to_axis(rf_chs[4]);
    out[10] = (a5 & 0xFF) as u8;
    out[11] = ((a5 >> 8) & 0xFF) as u8;

    // Axis 6 (Rz):     CH6 (Aux 2 / Switch SB)
    let a6 = us_to_axis(rf_chs[5]);
    out[12] = (a6 & 0xFF) as u8;
    out[13] = ((a6 >> 8) & 0xFF) as u8;

    // Axis 7 (Dial):   CH7 (Pot VRA)
    let a7 = us_to_axis(rf_chs[6]);
    out[14] = (a7 & 0xFF) as u8;
    out[15] = ((a7 >> 8) & 0xFF) as u8;

    // Axis 8 (Slider): CH8 (Pot VRB)
    let a8 = us_to_axis(rf_chs[7]);
    out[16] = (a8 & 0xFF) as u8;
    out[17] = ((a8 >> 8) & 0xFF) as u8;
}
