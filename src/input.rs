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

/// Normalized physical gimbal axes (-1000 .. +1000).
/// Model-type and stick-mode agnostic hardware representation:
/// - `rh`: Right Horizontal (PA0, index 0)
/// - `rv`: Right Vertical   (PA1, index 1)
/// - `lv`: Left Vertical    (PA2, index 2)
/// - `lh`: Left Horizontal  (PA3, index 3)
#[derive(Copy, Clone, Debug, Default)]
pub struct Gimbals {
    pub rh: i16,
    pub rv: i16,
    pub lv: i16,
    pub lh: i16,
}

impl Gimbals {
    /// Map physical gimbals to logical flight controls according to radio stick mode.
    #[inline(always)]
    pub fn to_flight_controls(&self, mode: crate::safety::StickMode) -> FlightControls {
        use crate::safety::StickMode;
        match mode {
            StickMode::Mode1 => FlightControls {
                aileron: self.rh,
                elevator: self.lv,
                throttle: self.rv,
                rudder: self.lh,
            },
            StickMode::Mode2 => FlightControls {
                aileron: self.rh,
                elevator: self.rv,
                throttle: self.lv,
                rudder: self.lh,
            },
            StickMode::Mode3 => FlightControls {
                aileron: self.lh,
                elevator: self.lv,
                throttle: self.rv,
                rudder: self.rh,
            },
            StickMode::Mode4 => FlightControls {
                aileron: self.lh,
                elevator: self.rv,
                throttle: self.lv,
                rudder: self.rh,
            },
        }
    }
}

/// Logical primary flight control axes (-1000 .. +1000).
#[derive(Copy, Clone, Debug, Default)]
pub struct FlightControls {
    pub aileron: i16,  // Roll (AIL)
    pub elevator: i16, // Pitch (ELE)
    pub throttle: i16, // Throttle (THR)
    pub rudder: i16,   // Yaw (RUD)
}

/// Backwards compatibility alias while migrating.
pub type Sticks = Gimbals;

/// Normalized potentiometer rotary dials (-1000 .. +1000).
#[derive(Copy, Clone, Debug)]
pub struct Pots {
    pub vr1: i16, // VRA
    pub vr2: i16, // VRB
}

/// Physical switch states on the radio.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub struct Switches {
    pub sa: SwitchPos,  // 2-pos
    pub sb: SwitchPos,  // 3-pos
    pub sc: SwitchPos,  // 3-pos
    pub sd: SwitchPos,  // 2-pos
    pub swe: SwitchPos, // 2-pos (Rear Left PA9)
    pub swf: SwitchPos, // 2-pos (Rear Right PA10)
    pub sg: SwitchPos,  // 2-pos (J4 PA13 SWDIO when enabled)
    pub sh: SwitchPos,  // 2-pos (J4 PA14 SWCLK when enabled)
}

impl Switches {
    pub const fn new() -> Self {
        Self {
            sa: SwitchPos::Up,
            sb: SwitchPos::Up,
            sc: SwitchPos::Up,
            sd: SwitchPos::Up,
            swe: SwitchPos::Up,
            swf: SwitchPos::Up,
            sg: SwitchPos::Up,
            sh: SwitchPos::Up,
        }
    }

    /// Detect if a switch moved between prev and self, returning the matching condition index 1..18.
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
        if self.swe != prev.swe {
            return Some(if self.swe == SwitchPos::Up { 11 } else { 12 });
        }
        if self.swf != prev.swf {
            return Some(if self.swf == SwitchPos::Up { 13 } else { 14 });
        }
        if self.sg != prev.sg {
            return Some(if self.sg == SwitchPos::Up { 15 } else { 16 });
        }
        if self.sh != prev.sh {
            return Some(if self.sh == SwitchPos::Up { 17 } else { 18 });
        }
        None
    }

    /// Detect if any switch toggled, returning the switch index 1..4 (1:SA, 2:SB, 3:SC, 4:SD).
    pub fn detect_dr_switch_change(&self, prev: &Switches) -> Option<u8> {
        if self.sa != prev.sa {
            Some(1)
        } else if self.sb != prev.sb {
            Some(2)
        } else if self.sc != prev.sc {
            Some(3)
        } else if self.sd != prev.sd {
            Some(4)
        } else {
            None
        }
    }
}

/// Full processed input snapshot.
pub struct InputState {
    pub gimbals: Gimbals,
    pub pots: Pots,
    pub switches: Switches,
    pub battery_mv: u16,
    #[allow(dead_code)]
    pub raw: [u16; adc::NUM_CHANNELS],
}

impl InputState {
    /// Convenience accessor for flight controls given a stick mode.
    #[inline(always)]
    pub fn flight_controls(&self, mode: crate::safety::StickMode) -> FlightControls {
        self.gimbals.to_flight_controls(mode)
    }

    /// Backwards compatibility accessor for physical gimbals.
    #[inline(always)]
    pub fn sticks(&self) -> &Gimbals {
        &self.gimbals
    }
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
    pub rh: AxisCalib, // PA0: Right Horizontal
    pub rv: AxisCalib, // PA1: Right Vertical
    pub lv: AxisCalib, // PA2: Left Vertical
    pub lh: AxisCalib, // PA3: Left Horizontal
    pub vra: AxisCalib,
    pub vrb: AxisCalib,
    pub filtered_battery_mv: u32,
}

impl InputCalibration {
    pub const fn default_factory() -> Self {
        Self {
            rh: AxisCalib::new(ADC_MIN, ADC_CENTER, ADC_MAX, true),
            rv: AxisCalib::new(ADC_MIN, ADC_CENTER, ADC_MAX, true),
            lv: AxisCalib::new(ADC_MIN, ADC_CENTER, ADC_MAX, false),
            lh: AxisCalib::new(ADC_MIN, ADC_CENTER, ADC_MAX, false),
            vra: AxisCalib::new(ADC_MIN, ADC_CENTER, ADC_MAX, false),
            vrb: AxisCalib::new(ADC_MIN, ADC_CENTER, ADC_MAX, false),
            filtered_battery_mv: 0,
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

    // Physical Gimbals: PA0 (RH), PA1 (RV), PA2 (LV), PA3 (LH)
    // RH: PA0
    calib.rh.invert = true;
    calib.rh.min = config.sticks[0].min;
    calib.rh.center = config.sticks[0].center;
    calib.rh.max = config.sticks[0].max;

    // RV: PA1
    calib.rv.invert = true;
    calib.rv.min = config.sticks[1].min;
    calib.rv.center = config.sticks[1].center;
    calib.rv.max = config.sticks[1].max;

    // LV: PA2
    calib.lv.invert = false;
    calib.lv.min = config.sticks[2].min;
    calib.lv.center = config.sticks[2].center;
    calib.lv.max = config.sticks[2].max;

    // LH: PA3
    calib.lh.invert = false;
    calib.lh.min = config.sticks[3].min;
    calib.lh.center = config.sticks[3].center;
    calib.lh.max = config.sticks[3].max;

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
        calib.rh.center = avg_roll;
    }
    if (1500..=2500).contains(&avg_pitch) {
        calib.rv.center = avg_pitch;
    }
    if (1500..=2500).contains(&avg_yaw) {
        calib.lh.center = avg_yaw;
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

/// Calculate battery voltage in millivolts using the FlySky FS-i6S 10k/5.1k resistor divider:
/// Vbat = Vadc * ((10000 + 5100) / 5100) = Vadc * 2.9608
/// Vadc = (raw * 3300) / 4095
pub fn calculate_battery_mv(raw: u16) -> u16 {
    // (raw * 3300 * 29608) / (4095 * 10000) = (raw * 977064) / 409500
    let vbat_mv = (raw as u32 * 977064) / 409500;
    vbat_mv as u16
}

/// Poll the ADC and return complete, processed flight controls.
pub fn poll() -> InputState {
    let raw = adc::read_raw();
    let calib = unsafe { &mut *INPUT_MANAGER.0.get() };

    // Process physical gimbal axes: PA0 (RH), PA1 (RV), PA2 (LV), PA3 (LH)
    let gimbals = Gimbals {
        rh: calib.rh.normalize(raw[0]),
        rv: calib.rv.normalize(raw[1]),
        lv: calib.lv.normalize(raw[2]),
        lh: calib.lh.normalize(raw[3]),
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
    let switches = Switches {
        sa: decode_switch(raw[4]), // PA4
        sb: decode_switch(raw[5]), // PA5
        sc: decode_switch(raw[8]), // PB0
        sd: decode_switch(raw[9]), // PB1
        swe: SwitchPos::Up,
        swf: SwitchPos::Up,
        sg: SwitchPos::Up,
        sh: SwitchPos::Up,
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
        gimbals,
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
    fn test_battery_scaling_fs_i6s() {
        // 0 ADC -> 0 mV
        assert_eq!(calculate_battery_mv(0), 0);

        // Max 12-bit ADC (4095) -> 3.3V * 2.9608 = 9770 mV (9.77V)
        assert_eq!(calculate_battery_mv(4095), 9770);

        // Half scale (2048) -> ~4.88V
        let half = calculate_battery_mv(2048);
        assert!(half >= 4880 && half <= 4895, "Actual half-scale: {}", half);

        // Typical 4x AA nominal (5.0V): Vadc = 5.0 / 2.9608 = 1.6888V -> ADC = 1.6888 / 3.3 * 4095 = 2096
        let v5 = calculate_battery_mv(2096);
        assert!(v5 >= 4990 && v5 <= 5010, "Actual 5V scale: {}", v5);
    }
}
