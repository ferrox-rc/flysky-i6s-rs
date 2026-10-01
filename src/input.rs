//! Input normalization, stick axis processing, switch decoding, and battery calculation.

use crate::adc;

/// 3-position switch states.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum SwitchPos {
    #[default]
    Up,
    Mid,
    Down,
}

impl SwitchPos {
    pub fn as_char(&self) -> char {
        match self {
            SwitchPos::Up => 'U',
            SwitchPos::Mid => 'M',
            SwitchPos::Down => 'D',
        }
    }
}

/// Normalized stick axes (-1000 .. +1000).
#[derive(Copy, Clone, Debug)]
pub struct Sticks {
    pub roll: i16,     // CH1 (AIL): -1000 (left) .. +1000 (right)
    pub pitch: i16,    // CH2 (ELE): -1000 (down) .. +1000 (up)
    pub throttle: i16, // CH3 (THR): -1000 (bottom/0%) .. +1000 (top/100%)
    pub yaw: i16,      // CH4 (RUD): -1000 (left) .. +1000 (right)
}

/// Normalized potentiometer rotary dials (-1000 .. +1000).
#[derive(Copy, Clone, Debug)]
pub struct Pots {
    pub vr1: i16, // VRA
    pub vr2: i16, // VRB
}

/// Physical switch states on the radio.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub struct Switches {
    pub sa: SwitchPos, // 2-pos
    pub sb: SwitchPos, // 3-pos
    pub sc: SwitchPos, // 3-pos
    pub sd: SwitchPos, // 2-pos
    pub se: SwitchPos, // 2-pos (Ext switch PC12)
    pub sf: SwitchPos, // 2-pos (Ext switch PC15)
}

impl Switches {
    pub const DEFAULT: Self = Self {
        sa: SwitchPos::Up,
        sb: SwitchPos::Up,
        sc: SwitchPos::Up,
        sd: SwitchPos::Up,
        se: SwitchPos::Up,
        sf: SwitchPos::Up,
    };

    /// Detect if a switch moved between prev and self, returning the matching condition index 1..14.
    pub fn detect_condition_change(&self, prev: &Switches) -> Option<u8> {
        if self.sa != prev.sa {
            return Some(if self.sa == SwitchPos::Up { 1 } else { 2 });
        }
        if self.sb != prev.sb {
            return Some(match self.sb {
                SwitchPos::Up => 3,
                SwitchPos::Mid => 4,
                SwitchPos::Down => 5,
            });
        }
        if self.sc != prev.sc {
            return Some(match self.sc {
                SwitchPos::Up => 6,
                SwitchPos::Mid => 7,
                SwitchPos::Down => 8,
            });
        }
        if self.sd != prev.sd {
            return Some(if self.sd == SwitchPos::Up { 9 } else { 10 });
        }
        if self.se != prev.se {
            return Some(if self.se == SwitchPos::Up { 11 } else { 12 });
        }
        if self.sf != prev.sf {
            return Some(if self.sf == SwitchPos::Up { 13 } else { 14 });
        }
        None
    }

    /// Detect if any switch toggled, returning the switch index 1..6 (1:SA, 2:SB, 3:SC, 4:SD, 5:SE, 6:SF).
    pub fn detect_dr_switch_change(&self, prev: &Switches) -> Option<u8> {
        if self.sa != prev.sa {
            Some(1)
        } else if self.sb != prev.sb {
            Some(2)
        } else if self.sc != prev.sc {
            Some(3)
        } else if self.sd != prev.sd {
            Some(4)
        } else if self.se != prev.se {
            Some(5)
        } else if self.sf != prev.sf {
            Some(6)
        } else {
            None
        }
    }
}

/// Full processed input snapshot.
pub struct InputState {
    pub sticks: Sticks,
    pub pots: Pots,
    pub switches: Switches,
    pub battery_mv: u16,
    #[allow(dead_code)]
    pub raw: [u16; adc::NUM_CHANNELS],
}

/// Calibration data for a single analog axis.
#[derive(Copy, Clone, Debug)]
pub struct AxisCalib {
    pub min: u16,
    pub center: u16,
    pub max: u16,
    pub filtered_raw: u32,
    pub invert: bool,
}

impl AxisCalib {
    pub const fn new(min: u16, center: u16, max: u16, invert: bool) -> Self {
        Self {
            min,
            center,
            max,
            filtered_raw: 0,
            invert,
        }
    }

    /// Normalize raw ADC count (0..4095) around center point to -1000..+1000.
    /// Fast responsive jitter filter: suppresses resting potentiometer noise
    /// while passing all intentional stick movements (> 6 counts) with 0 latency.
    pub fn normalize(&mut self, raw: u16) -> i16 {
        if self.filtered_raw == 0 {
            self.filtered_raw = raw as u32 * 4;
        }

        let previous = (self.filtered_raw / 4) as u16;
        let diff = (raw as i32 - previous as i32).abs();

        // Responsive low-latency jitter filter:
        // Pass through any change >= 6 counts directly (0 latency)
        // For micro-noise (< 6 counts), use 4-sample fast MMA filter
        if diff < 6 {
            self.filtered_raw = (self.filtered_raw - previous as u32) + raw as u32;
        } else {
            self.filtered_raw = raw as u32 * 4;
        }
        let smoothed_raw = (self.filtered_raw / 4) as u16;

        let val = if smoothed_raw <= self.center {
            let span = (self.center - self.min).max(100) as i32;
            let delta = smoothed_raw as i32 - self.center as i32;
            ((delta * 1000) / span).clamp(-1000, 0)
        } else {
            let span = (self.max - self.center).max(100) as i32;
            let delta = smoothed_raw as i32 - self.center as i32;
            ((delta * 1000) / span).clamp(0, 1000)
        };

        if self.invert {
            -val as i16
        } else {
            val as i16
        }
    }
}

use crate::adc::{ADC_CENTER, ADC_MAX, ADC_MIN};

#[derive(Copy, Clone, Debug)]
pub struct InputCalibration {
    pub roll: AxisCalib,
    pub pitch: AxisCalib,
    pub throttle: AxisCalib,
    pub yaw: AxisCalib,
    pub vra: AxisCalib,
    pub vrb: AxisCalib,
    pub filtered_battery_mv: u32,
    pub ext_switches: bool,
}

impl InputCalibration {
    pub const fn default_factory() -> Self {
        Self {
            roll: AxisCalib::new(ADC_MIN, ADC_CENTER, ADC_MAX, true),
            pitch: AxisCalib::new(ADC_MIN, ADC_CENTER, ADC_MAX, true),
            throttle: AxisCalib::new(ADC_MIN, ADC_CENTER, ADC_MAX, false),
            yaw: AxisCalib::new(ADC_MIN, ADC_CENTER, ADC_MAX, false),
            vra: AxisCalib::new(ADC_MIN, ADC_CENTER, ADC_MAX, false),
            vrb: AxisCalib::new(ADC_MIN, ADC_CENTER, ADC_MAX, false),
            filtered_battery_mv: 0,
            ext_switches: false,
        }
    }
}

use core::cell::UnsafeCell;

struct InputManagerCell(UnsafeCell<InputCalibration>);
unsafe impl Sync for InputManagerCell {}

static INPUT_MANAGER: InputManagerCell = InputManagerCell(UnsafeCell::new(InputCalibration::default_factory()));

/// Apply a full set of stick and pot calibration endpoints.
pub fn apply_calibration(config: &crate::storage::RadioConfig) {
    let calib = unsafe { &mut *INPUT_MANAGER.0.get() };

    calib.ext_switches = config.ext_switches != 0;

    // Roll: PA0 (RH) - inverted on FlySky mechanical gimbal
    calib.roll.invert = true;
    calib.roll.min = config.sticks[0].min;
    calib.roll.center = config.sticks[0].center;
    calib.roll.max = config.sticks[0].max;

    // Pitch: PA1 (RV) - inverted on FlySky mechanical gimbal
    calib.pitch.invert = true;
    calib.pitch.min = config.sticks[1].min;
    calib.pitch.center = config.sticks[1].center;
    calib.pitch.max = config.sticks[1].max;

    // Throttle: PA2 (LV) - strictly normal/uninverted on Mode 2 hardware
    calib.throttle.invert = false;
    calib.throttle.min = config.sticks[2].min;
    calib.throttle.center = config.sticks[2].center;
    calib.throttle.max = config.sticks[2].max;

    // Yaw: PA3 (LH) - normal/uninverted
    calib.yaw.invert = false;
    calib.yaw.min = config.sticks[3].min;
    calib.yaw.center = config.sticks[3].center;
    calib.yaw.max = config.sticks[3].max;

    // Pots: VRA (PA6), VRB (PA7)
    calib.vra.invert = false;
    calib.vra.min = config.pots[0].min;
    calib.vra.center = config.pots[0].center;
    calib.vra.max = config.pots[0].max;

    calib.vrb.invert = false;
    calib.vrb.min = config.pots[1].min;
    calib.vrb.center = config.pots[1].center;
    calib.vrb.max = config.pots[1].max;
}

/// Enable or disable external switches SE & SF reading at runtime.
pub fn set_ext_switches_enabled(enabled: bool) {
    let calib = unsafe { &mut *INPUT_MANAGER.0.get() };
    calib.ext_switches = enabled;
}

/// Check if external switches SE & SF are currently enabled.
pub fn is_ext_switches_enabled() -> bool {
    let calib = unsafe { &*INPUT_MANAGER.0.get() };
    calib.ext_switches
}

/// Initialize input subsystem, load Flash calibration, and measure resting center for spring-loaded gimbals.
pub fn init() {
    // Wait for continuous DMA scanner to complete its initial cycle
    adc::wait_first_conversion();

    // 1. Load persisted calibration from Flash
    let cfg = crate::storage::load_config();
    apply_calibration(&cfg);

    // 2. Average 16 scans over ~4ms for rock-solid zero reference
    let mut sum_pitch = 0u32;
    let mut sum_roll = 0u32;
    let mut sum_yaw = 0u32;
    const SAMPLES: u32 = 16;

    for _ in 0..SAMPLES {
        let raw = adc::read_raw();
        sum_roll += raw[0] as u32;  // PA0 = Roll (Aileron)
        sum_pitch += raw[1] as u32; // PA1 = Pitch (Elevator)
        sum_yaw += raw[3] as u32;   // PA3 = LH (Yaw)
        for _ in 0..3_000 {
            cortex_m::asm::nop();
        }
    }

    let avg_roll = (sum_roll / SAMPLES) as u16;
    let avg_pitch = (sum_pitch / SAMPLES) as u16;
    let avg_yaw = (sum_yaw / SAMPLES) as u16;

    let calib = unsafe { &mut *INPUT_MANAGER.0.get() };
    // Slightly refine spring-loaded resting center if within reasonable range (1500..2500)
    if (1500..=2500).contains(&avg_roll) {
        calib.roll.center = avg_roll;
    }
    if (1500..=2500).contains(&avg_pitch) {
        calib.pitch.center = avg_pitch;
    }
    if (1500..=2500).contains(&avg_yaw) {
        calib.yaw.center = avg_yaw;
    }
}

/// Decode resistor ladder analog switch voltage:
/// - UP:   0 .. 1365 (0 .. 1/3 Vcc)
/// - MID:  1366 .. 2730 (1/3 .. 2/3 Vcc)
/// - DOWN: 2731 .. 4095 (2/3 .. 1 Vcc)
fn decode_switch(raw: u16) -> SwitchPos {
    if raw < 1365 {
        SwitchPos::Up
    } else if raw < 2730 {
        SwitchPos::Mid
    } else {
        SwitchPos::Down
    }
}

/// Calculate battery voltage in millivolts using the OpenI6X calibrated formula.
/// Accounts for the 1/2 resistor divider and 0.20V series protection diode.
fn calculate_battery_mv(raw: u16) -> u16 {
    // OpenTX calibrated formula uses 11-bit ADC (raw / 2):
    // instant_vbat = ((raw/2) * 200) / 421 + 20 (in 10mV steps)
    // Directly from 12-bit raw: (raw * 100) / 421 + 20
    let vbat_10mv = ((raw as u32 * 100) / 421) + 20;
    (vbat_10mv * 10) as u16
}

/// Poll the ADC and return complete, processed flight controls.
pub fn poll() -> InputState {
    let raw = adc::read_raw();
    let calib = unsafe { &mut *INPUT_MANAGER.0.get() };

    // Mode 2 Pinout matching FlySky FS-i6X hardware:
    // raw[0] = PA0: RH (Right Horizontal - Roll / Aileron)
    // raw[1] = PA1: RV (Right Vertical - Pitch / Elevator)
    // raw[2] = PA2: LV (Left Vertical - Throttle, friction ratchet / no spring return)
    // raw[3] = PA3: LH (Left Horizontal - Yaw / Rudder)
    let sticks = Sticks {
        roll: calib.roll.normalize(raw[0]),
        pitch: calib.pitch.normalize(raw[1]),
        throttle: calib.throttle.normalize(raw[2]),
        yaw: calib.yaw.normalize(raw[3]),
    };

    // Pots: VRA on PA6, VRB on PA7
    let pots = Pots {
        vr1: calib.vra.normalize(raw[6]), // PA6 (VRA)
        vr2: calib.vrb.normalize(raw[7]), // PA7 (VRB)
    };

    // Switches:
    // SA on PA4 (2-pos)
    // SB on PA5 (3-pos)
    // SC on PB0 (3-pos) - Swapped with VRB!
    // SD on PB1 (2-pos)
    // SE on PC12 (2-pos, active-low mod)
    // SF on PC15 (2-pos, active-low mod)
    let (se, sf) = if calib.ext_switches {
        #[cfg(all(feature = "stm32", not(test)))]
        {
            let gpioc = unsafe { &*stm32f0xx_hal::pac::GPIOC::ptr() };
            let idr = gpioc.idr.read().bits();
            let se = if (idr & (1 << 12)) == 0 {
                SwitchPos::Down
            } else {
                SwitchPos::Up
            };
            let sf = if (idr & (1 << 15)) == 0 {
                SwitchPos::Down
            } else {
                SwitchPos::Up
            };
            (se, sf)
        }
        #[cfg(any(not(feature = "stm32"), test))]
        {
            (SwitchPos::Up, SwitchPos::Up)
        }
    } else {
        (SwitchPos::Up, SwitchPos::Up)
    };

    let switches = Switches {
        sa: decode_switch(raw[4]), // PA4
        sb: decode_switch(raw[5]), // PA5
        sc: decode_switch(raw[8]), // PB0
        sd: decode_switch(raw[9]), // PB1
        se,
        sf,
    };

    let instant_mv = calculate_battery_mv(raw[10]); // PC0
    let battery_mv = if calib.filtered_battery_mv == 0 {
        calib.filtered_battery_mv = (instant_mv as u32) << 8;
        instant_mv
    } else {
        // Exponential moving average filter (alpha = 1/32) to stabilize hundredths digit
        calib.filtered_battery_mv = calib.filtered_battery_mv - (calib.filtered_battery_mv >> 5) + ((instant_mv as u32) << 3);
        (calib.filtered_battery_mv >> 8) as u16
    };

    InputState {
        sticks,
        pots,
        switches,
        battery_mv,
        raw,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_switches_condition_detection() {
        let base = Switches::DEFAULT;

        // SA: Up=1, Down=2
        let mut sw = base;
        sw.sa = SwitchPos::Down;
        assert_eq!(sw.detect_condition_change(&base), Some(2));
        assert_eq!(base.detect_condition_change(&sw), Some(1));

        // SB: Up=3, Mid=4, Down=5
        let mut sw = base;
        sw.sb = SwitchPos::Mid;
        assert_eq!(sw.detect_condition_change(&base), Some(4));
        sw.sb = SwitchPos::Down;
        assert_eq!(sw.detect_condition_change(&base), Some(5));

        // SC: Up=6, Mid=7, Down=8
        let mut sw = base;
        sw.sc = SwitchPos::Mid;
        assert_eq!(sw.detect_condition_change(&base), Some(7));
        sw.sc = SwitchPos::Down;
        assert_eq!(sw.detect_condition_change(&base), Some(8));

        // SD: Up=9, Down=10
        let mut sw = base;
        sw.sd = SwitchPos::Down;
        assert_eq!(sw.detect_condition_change(&base), Some(10));
        assert_eq!(base.detect_condition_change(&sw), Some(9));

        // SE: Up=11, Down=12
        let mut sw = base;
        sw.se = SwitchPos::Down;
        assert_eq!(sw.detect_condition_change(&base), Some(12));
        assert_eq!(base.detect_condition_change(&sw), Some(11));

        // SF: Up=13, Down=14
        let mut sw = base;
        sw.sf = SwitchPos::Down;
        assert_eq!(sw.detect_condition_change(&base), Some(14));
        assert_eq!(base.detect_condition_change(&sw), Some(13));

        // No change
        assert_eq!(base.detect_condition_change(&base), None);
    }

    #[test]
    fn test_dr_switch_detection_with_se_sf() {
        let base = Switches::DEFAULT;

        let mut sw = base;
        sw.sa = SwitchPos::Down;
        assert_eq!(sw.detect_dr_switch_change(&base), Some(1));

        let mut sw = base;
        sw.sb = SwitchPos::Mid;
        assert_eq!(sw.detect_dr_switch_change(&base), Some(2));

        let mut sw = base;
        sw.sc = SwitchPos::Down;
        assert_eq!(sw.detect_dr_switch_change(&base), Some(3));

        let mut sw = base;
        sw.sd = SwitchPos::Down;
        assert_eq!(sw.detect_dr_switch_change(&base), Some(4));

        let mut sw = base;
        sw.se = SwitchPos::Down;
        assert_eq!(sw.detect_dr_switch_change(&base), Some(5));

        let mut sw = base;
        sw.sf = SwitchPos::Down;
        assert_eq!(sw.detect_dr_switch_change(&base), Some(6));

        assert_eq!(base.detect_dr_switch_change(&base), None);
    }

    #[test]
    fn test_ext_switches_toggle() {
        set_ext_switches_enabled(true);
        assert!(is_ext_switches_enabled());
        set_ext_switches_enabled(false);
        assert!(!is_ext_switches_enabled());
    }
}
