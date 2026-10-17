//! Input normalization, stick axis processing, switch decoding, and battery calculation.

use crate::adc;
use crate::storage::{AdcInputMode, RadioConfig};

/// Default ADC deadband window around mechanical center detent (±16 counts ≈ ±0.4% travel).
pub const DETENT_DEADBAND: u16 = 16;

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

/// Normalized potentiometer rotary dials and auxiliary analog inputs (-1000 .. +1000).
#[derive(Copy, Clone, Debug, Default)]
pub struct Pots {
    pub vr1: i16, // VRA (PA6)
    pub vr2: i16, // VRB (PA7)
    pub vr3: i16, // VRC (Ext ADC 1)
    pub vr4: i16, // VRD (Ext ADC 2)
    pub vr5: i16, // VRE (Ext ADC 3)
    pub vr6: i16, // VRF (Ext ADC 4)
}

/// Physical switch states on the radio.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub struct Switches {
    pub sa: SwitchPos,  // 2-pos
    pub sb: SwitchPos,  // 3-pos
    pub sc: SwitchPos,  // 3-pos
    pub sd: SwitchPos,  // 2-pos
    pub se: SwitchPos,  // 2-pos (Ext switch PC12 on i6X / Rear Left PA9 on i6S)
    pub sf: SwitchPos,  // 2-pos (Ext switch PC15 on i6X / Rear Right PA10 on i6S)
    pub sg: SwitchPos,  // 2-pos (J4 PA13 SWDIO when enabled on i6S)
    pub sh: SwitchPos,  // 2-pos (J4 PA14 SWCLK when enabled on i6S)
    pub swi: SwitchPos, // 2-pos or 3-pos (Ext ADC 1 when configured as switch)
    pub swj: SwitchPos, // 2-pos or 3-pos (Ext ADC 2 when configured as switch)
    pub swk: SwitchPos, // 2-pos or 3-pos (Ext ADC 3 when configured as switch)
    pub swl: SwitchPos, // 2-pos or 3-pos (Ext ADC 4 when configured as switch)
}

impl Switches {
    pub const DEFAULT: Self = Self::new();

    pub const fn new() -> Self {
        Self {
            sa: SwitchPos::Up,
            sb: SwitchPos::Up,
            sc: SwitchPos::Up,
            sd: SwitchPos::Up,
            se: SwitchPos::Up,
            sf: SwitchPos::Up,
            sg: SwitchPos::Up,
            sh: SwitchPos::Up,
            swi: SwitchPos::Up,
            swj: SwitchPos::Up,
            swk: SwitchPos::Up,
            swl: SwitchPos::Up,
        }
    }

    /// Detect if a switch moved between prev and self, returning the matching condition index 1..26.
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
        if self.sg != prev.sg {
            return Some(if self.sg == SwitchPos::Up { 15 } else { 16 });
        }
        if self.sh != prev.sh {
            return Some(if self.sh == SwitchPos::Up { 17 } else { 18 });
        }
        if self.swi != prev.swi {
            return Some(if self.swi == SwitchPos::Up { 19 } else { 20 });
        }
        if self.swj != prev.swj {
            return Some(if self.swj == SwitchPos::Up { 21 } else { 22 });
        }
        if self.swk != prev.swk {
            return Some(if self.swk == SwitchPos::Up { 23 } else { 24 });
        }
        if self.swl != prev.swl {
            return Some(if self.swl == SwitchPos::Up { 25 } else { 26 });
        }
        None
    }

    /// Detect if any switch toggled, returning the switch index 1..6.
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
    pub gimbals: Gimbals,
    pub pots: Pots,
    pub switches: Switches,
    pub battery_mv: u16,
    pub aux_pots: [i16; 10],
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

    /// Responsive low-latency jitter filter:
    /// Passes changes >= 6 counts with 0 latency; applies 4-sample MMA filter for resting micro-noise.
    pub fn filter_raw(&mut self, raw: u16) -> u16 {
        if self.filtered_raw == 0 {
            self.filtered_raw = raw as u32 * 4;
        }

        let previous = (self.filtered_raw / 4) as u16;
        let diff = (raw as i32 - previous as i32).abs();

        if diff < 6 {
            self.filtered_raw = (self.filtered_raw - previous as u32) + raw as u32;
        } else {
            self.filtered_raw = raw as u32 * 4;
        }
        (self.filtered_raw / 4) as u16
    }

    /// Normalize raw ADC count (0..4095) around center point to -1000..+1000.
    pub fn normalize(&mut self, raw: u16) -> i16 {
        let smoothed_raw = self.filter_raw(raw);

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

    /// Normalize a center-detented potentiometer with a deadband around the mechanical detent.
    /// Guarantees a solid 0 output when resting in the physical notch, while maintaining full
    /// independent linear travel (-1000 to +1000) even when negative and positive spans are asymmetric.
    pub fn normalize_detent(&mut self, raw: u16, deadband: u16) -> i16 {
        let smoothed_raw = self.filter_raw(raw);

        let val = if smoothed_raw.abs_diff(self.center) <= deadband {
            0
        } else if smoothed_raw < self.center {
            let active_center = self.center.saturating_sub(deadband);
            let span = active_center.saturating_sub(self.min).max(100) as i32;
            let delta = smoothed_raw as i32 - active_center as i32;
            ((delta * 1000) / span).clamp(-1000, 0)
        } else {
            let active_center = self.center.saturating_add(deadband);
            let span = self.max.saturating_sub(active_center).max(100) as i32;
            let delta = smoothed_raw as i32 - active_center as i32;
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
    pub aux: [AxisCalib; 10], // 0..3: SA..SD, 4..5: VRA..VRB, 6..9: VRC..VRF
    pub adc_modes: [u8; 10],
    pub filtered_battery_mv: u32,
    pub ext_switches: bool,
    pub ext_adc: bool,
}

impl InputCalibration {
    pub const fn default_factory() -> Self {
        Self {
            rh: AxisCalib::new(ADC_MIN, ADC_CENTER, ADC_MAX, true),
            rv: AxisCalib::new(ADC_MIN, ADC_CENTER, ADC_MAX, true),
            lv: AxisCalib::new(ADC_MIN, ADC_CENTER, ADC_MAX, false),
            lh: AxisCalib::new(ADC_MIN, ADC_CENTER, ADC_MAX, false),
            aux: [
                AxisCalib::new(ADC_MIN, ADC_CENTER, ADC_MAX, false), // SA
                AxisCalib::new(ADC_MIN, ADC_CENTER, ADC_MAX, false), // SB
                AxisCalib::new(ADC_MIN, ADC_CENTER, ADC_MAX, false), // SC
                AxisCalib::new(ADC_MIN, ADC_CENTER, ADC_MAX, false), // SD
                AxisCalib::new(ADC_MIN, ADC_CENTER, ADC_MAX, false), // VRA
                AxisCalib::new(ADC_MIN, ADC_CENTER, ADC_MAX, false), // VRB
                AxisCalib::new(ADC_MIN, ADC_CENTER, ADC_MAX, false), // VRC
                AxisCalib::new(ADC_MIN, ADC_CENTER, ADC_MAX, false), // VRD
                AxisCalib::new(ADC_MIN, ADC_CENTER, ADC_MAX, false), // VRE
                AxisCalib::new(ADC_MIN, ADC_CENTER, ADC_MAX, false), // VRF
            ],
            adc_modes: [0; 10],
            filtered_battery_mv: 0,
            ext_switches: false,
            ext_adc: false,
        }
    }
}

use core::cell::UnsafeCell;

struct InputManagerCell(UnsafeCell<InputCalibration>);
unsafe impl Sync for InputManagerCell {}

static INPUT_MANAGER: InputManagerCell =
    InputManagerCell(UnsafeCell::new(InputCalibration::default_factory()));

/// Apply a full set of stick and pot calibration endpoints.
pub fn apply_calibration(config: &RadioConfig) {
    let calib = unsafe { &mut *INPUT_MANAGER.0.get() };

    calib.ext_switches = config.ext_switches != 0;
    calib.ext_adc = config.ext_adc != 0;
    for i in 0..10 {
        calib.adc_modes[i] = config.get_adc_mode_val(i);
    }

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

    // 10 Auxiliary Analog Channels
    for i in 0..10 {
        let mode = AdcInputMode::resolve(i, calib.adc_modes[i]);
        calib.aux[i].invert = false;
        calib.aux[i].min = config.aux_pots[i].min;
        calib.aux[i].max = config.aux_pots[i].max;

        if mode == AdcInputMode::PotDetent {
            // Fixed at the captured physical detent center
            calib.aux[i].center = config.aux_pots[i].center;
        } else {
            // Standard continuous pot (or switch): clean calculated arithmetic midpoint
            calib.aux[i].center =
                ((config.aux_pots[i].min as u32 + config.aux_pots[i].max as u32) / 2) as u16;
        }
    }
}

pub fn set_ext_switches_enabled(enabled: bool) {
    let calib = unsafe { &mut *INPUT_MANAGER.0.get() };
    calib.ext_switches = enabled;
}

pub fn is_ext_switches_enabled() -> bool {
    let calib = unsafe { &*INPUT_MANAGER.0.get() };
    calib.ext_switches
}

pub fn set_ext_adc_enabled(enabled: bool) {
    let calib = unsafe { &mut *INPUT_MANAGER.0.get() };
    calib.ext_adc = enabled;
}

pub fn is_ext_adc_enabled() -> bool {
    let calib = unsafe { &*INPUT_MANAGER.0.get() };
    calib.ext_adc
}

/// Map raw analog reading to a 6-position flight mode index (1..6).
pub fn decode_switch_6pos_num(raw: u16) -> u8 {
    if raw < 682 {
        1
    } else if raw < 1365 {
        2
    } else if raw < 2048 {
        3
    } else if raw < 2730 {
        4
    } else if raw < 3413 {
        5
    } else {
        6
    }
}

/// Decode a 3-position switch from raw ADC counts.
pub fn decode_switch_3pos(raw: u16) -> SwitchPos {
    if raw < 1365 {
        SwitchPos::Up
    } else if raw < 2730 {
        SwitchPos::Mid
    } else {
        SwitchPos::Down
    }
}

/// Decode a 2-position switch from raw ADC counts.
pub fn decode_switch_2pos(raw: u16) -> SwitchPos {
    if raw < 2048 {
        SwitchPos::Up
    } else {
        SwitchPos::Down
    }
}

/// Map an aux channel index (0..9) to raw ADC channel index.
#[inline(always)]
pub fn aux_index_to_raw_adc(ch: usize) -> usize {
    match ch {
        0 => 4,  // SA: PA4
        1 => 5,  // SB: PA5
        2 => 8,  // SC: PB0
        3 => 9,  // SD: PB1
        4 => 6,  // VRA: PA6
        5 => 7,  // VRB: PA7
        6 => 11, // VRC: Ext ADC 1
        7 => 12, // VRD: Ext ADC 2
        8 => 13, // VRE: Ext ADC 3
        _ => 14, // VRF: Ext ADC 4
    }
}

/// Process a single complete sample frame of raw ADC channels into an `InputState`.
pub fn process(raw: [u16; adc::NUM_CHANNELS]) -> InputState {
    let calib = unsafe { &mut *INPUT_MANAGER.0.get() };

    // Process physical gimbal axes: PA0 (RH), PA1 (RV), PA2 (LV), PA3 (LH)
    let gimbals = Gimbals {
        rh: calib.rh.normalize(raw[0]),
        rv: calib.rv.normalize(raw[1]),
        lv: calib.lv.normalize(raw[2]),
        lh: calib.lh.normalize(raw[3]),
    };

    // Process auxiliary analog channels (0..9)
    let mut aux_pots = [0i16; 10];
    let max_channels = if calib.ext_adc { 10 } else { 6 };

    for i in 0..max_channels {
        let raw_val = raw[aux_index_to_raw_adc(i)];
        let mode = AdcInputMode::resolve(i, calib.adc_modes[i]);

        aux_pots[i] = match mode {
            AdcInputMode::PotDetent => calib.aux[i].normalize_detent(raw_val, DETENT_DEADBAND),
            AdcInputMode::Pot => calib.aux[i].normalize(raw_val),
            AdcInputMode::TwoPos | AdcInputMode::InstantTrim => {
                if raw_val < 2048 {
                    -1000
                } else {
                    1000
                }
            }
            AdcInputMode::ThreePos => {
                if raw_val < 1365 {
                    -1000
                } else if raw_val < 2730 {
                    0
                } else {
                    1000
                }
            }
            AdcInputMode::SixPos => {
                let step = decode_switch_6pos_num(raw_val);
                -1000 + ((step as i16 - 1) * 400)
            }
            AdcInputMode::Default => calib.aux[i].normalize(raw_val),
        };
    }

    let pots = Pots {
        vr1: aux_pots[4],
        vr2: aux_pots[5],
        vr3: aux_pots[6],
        vr4: aux_pots[7],
        vr5: aux_pots[8],
        vr6: aux_pots[9],
    };

    // Switches decoding
    let sa = match AdcInputMode::resolve(0, calib.adc_modes[0]) {
        AdcInputMode::ThreePos => decode_switch_3pos(raw[4]),
        _ => decode_switch_2pos(raw[4]),
    };
    let sb = match AdcInputMode::resolve(1, calib.adc_modes[1]) {
        AdcInputMode::TwoPos => decode_switch_2pos(raw[5]),
        _ => decode_switch_3pos(raw[5]),
    };
    let sc = match AdcInputMode::resolve(2, calib.adc_modes[2]) {
        AdcInputMode::TwoPos => decode_switch_2pos(raw[8]),
        _ => decode_switch_3pos(raw[8]),
    };
    let sd = match AdcInputMode::resolve(3, calib.adc_modes[3]) {
        AdcInputMode::ThreePos => decode_switch_3pos(raw[9]),
        _ => decode_switch_2pos(raw[9]),
    };

    let (se, sf, sg, sh, swi, swj, swk, swl) =
        crate::hw::decode_board_switches(&raw, calib.ext_switches, calib.ext_adc, &calib.adc_modes);

    let switches = Switches {
        sa,
        sb,
        sc,
        sd,
        se,
        sf,
        sg,
        sh,
        swi,
        swj,
        swk,
        swl,
    };

    let instant_mv = calculate_battery_mv(raw[10]); // PC0
    let battery_mv = if calib.filtered_battery_mv == 0 {
        calib.filtered_battery_mv = (instant_mv as u32) << 8;
        instant_mv
    } else {
        // Exponential moving average filter (alpha = 1/32) to stabilize hundredths digit
        calib.filtered_battery_mv = calib.filtered_battery_mv - (calib.filtered_battery_mv >> 5)
            + ((instant_mv as u32) << 3);
        (calib.filtered_battery_mv >> 8) as u16
    };

    InputState {
        gimbals,
        pots,
        switches,
        battery_mv,
        aux_pots,
        raw,
    }
}

/// Calculate battery voltage in millivolts using the board-specific resistor divider.
pub fn calculate_battery_mv(raw: u16) -> u16 {
    crate::hw::calculate_battery_mv(raw)
}

/// Read the latest frame from the ADC and return the processed input state.
pub fn read() -> InputState {
    let raw = adc::read_raw();
    process(raw)
}

/// Poll the ADC and return complete, processed flight controls.
pub fn poll() -> InputState {
    read()
}

/// Initialize input subsystem, load saved calibration from flash, and prime filters.
pub fn init() {
    adc::wait_first_conversion();
    let cfg = crate::storage::load_config();
    apply_calibration(&cfg);

    // Prime the jitter and noise filters across 16 sample cycles
    const SAMPLES: u32 = 16;
    for _ in 0..SAMPLES {
        let raw = adc::read_raw();
        let _ = process(raw);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_battery_scaling() {
        let low = calculate_battery_mv(0);
        let mid = calculate_battery_mv(2048);
        let high = calculate_battery_mv(4095);

        assert!(low <= 200);
        assert!(mid > 4500 && mid < 5500, "Actual half-scale: {}", mid);
        assert!(high > 9500 && high < 10500, "Actual full-scale: {}", high);
    }

    #[test]
    fn test_detent_normalization_and_deadband() {
        let mut axis = AxisCalib::new(1000, 2048, 3000, false);

        // Within deadband around center (2048 +/- 16) must be exactly 0
        assert_eq!(axis.normalize_detent(2048, DETENT_DEADBAND), 0);
        assert_eq!(axis.normalize_detent(2048 - 15, DETENT_DEADBAND), 0);
        assert_eq!(axis.normalize_detent(2048 + 15, DETENT_DEADBAND), 0);

        // Outside deadband moves smoothly toward -1000 and +1000
        let neg = axis.normalize_detent(1000, DETENT_DEADBAND);
        assert_eq!(neg, -1000);

        let pos = axis.normalize_detent(3000, DETENT_DEADBAND);
        assert_eq!(pos, 1000);
    }

    #[test]
    fn test_decode_switch_6pos_steps() {
        assert_eq!(decode_switch_6pos_num(100), 1);
        assert_eq!(decode_switch_6pos_num(1000), 2);
        assert_eq!(decode_switch_6pos_num(1700), 3);
        assert_eq!(decode_switch_6pos_num(2400), 4);
        assert_eq!(decode_switch_6pos_num(3100), 5);
        assert_eq!(decode_switch_6pos_num(3800), 6);
    }

    #[test]
    fn test_aux_index_to_raw_adc_mapping() {
        assert_eq!(aux_index_to_raw_adc(0), 4);  // SA: PA4
        assert_eq!(aux_index_to_raw_adc(1), 5);  // SB: PA5
        assert_eq!(aux_index_to_raw_adc(2), 8);  // SC: PB0
        assert_eq!(aux_index_to_raw_adc(3), 9);  // SD: PB1
        assert_eq!(aux_index_to_raw_adc(4), 6);  // VRA: PA6
        assert_eq!(aux_index_to_raw_adc(5), 7);  // VRB: PA7
        assert_eq!(aux_index_to_raw_adc(6), 11); // VRC
        assert_eq!(aux_index_to_raw_adc(7), 12); // VRD
        assert_eq!(aux_index_to_raw_adc(8), 13); // VRE
        assert_eq!(aux_index_to_raw_adc(9), 14); // VRF
    }

    #[test]
    fn test_swl_switch_condition_change() {
        let mut prev = Switches::new();
        let mut curr = Switches::new();

        // Initially both Up
        assert_eq!(curr.detect_condition_change(&prev), None);

        // SWL toggled to Down -> condition 26
        curr.swl = SwitchPos::Down;
        assert_eq!(curr.detect_condition_change(&prev), Some(26));

        // SWL toggled back to Up -> condition 25
        prev.swl = SwitchPos::Down;
        curr.swl = SwitchPos::Up;
        assert_eq!(curr.detect_condition_change(&prev), Some(25));
    }

    #[test]
    fn test_extended_adc_poll() {
        let mut cfg = RadioConfig::default_factory();
        cfg.ext_adc = 1;
        cfg.set_adc_mode_val(6, 1);
        cfg.set_adc_mode_val(7, 2);
        cfg.set_adc_mode_val(8, 3);
        cfg.set_adc_mode_val(9, 4);

        apply_calibration(&cfg);

        // Mock ADC readings
        let mut mock_adc = [2048u16; crate::adc::NUM_CHANNELS];
        mock_adc[11] = 3500; // ch6: high count -> 2-pos switch Down
        mock_adc[12] = 2048; // ch7: center count -> 3-pos switch Mid
        mock_adc[13] = 2500; // ch8: count 2500 -> 6-pos switch step 4
        mock_adc[14] = 4095; // ch9: max count -> Pot +1000

        crate::adc::set_test_adc_snapshot(mock_adc);

        let state = poll();
        assert_eq!(state.switches.swi, SwitchPos::Down);
        assert_eq!(state.switches.swj, SwitchPos::Mid);
        // aux_pots[8] for 6-pos step 4 = -1000 + (3 * 400) = 200
        assert_eq!(state.aux_pots[8], 200);
        // aux_pots[9] for pot at 4095 = 1000
        assert_eq!(state.aux_pots[9], 1000);
    }
}
