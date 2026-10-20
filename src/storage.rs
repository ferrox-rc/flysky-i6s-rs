//! I2C EEPROM storage (FT24C128A 16 KB) for non-volatile configuration and 20-model memory system.
//!
//! Supports adaptive bus & address auto-detection:
//! - Primary: I2C2 on PB10 (SCL) and PB11 (SDA) across addresses 0x50..0x57
//! - Fallback: I2C1 on PB8 (SCL) and PB9 (SDA) across addresses 0x50..0x57
//!
//! Physical capacity: 16,384 bytes (256 pages x 64 bytes).
//! Slot layout:
//! - 0x0000..0x0100 (256 B / 4 pages): Primary Radio Config (Slot A)
//! - 0x0100..0x0200 (256 B / 4 pages): Backup Radio Config (Slot B - A/B safe save)
//! - 0x0200..0x3E00 (15,360 B / 240 pages): 20 Models x 768 B (12 pages each)
//! - 0x3E00..0x4000 (512 B / 8 pages): Reserved / scratchpad

#[cfg(not(test))]
use stm32f0xx_hal::pac;

pub const FLASH_MAGIC: u32 = 0x4653_4B59; // "FSKY"
pub const CONFIG_VERSION: u32 = 5;
pub const NUM_MODELS: usize = 20;

/// 8-byte framing envelope magic ("FX" in little-endian = 0x58, 0x46 -> 0x4658)
pub const SLOT_MAGIC: u16 = 0x4658;
pub const SCHEMA_VERSION: u8 = 1;
pub const HEADER_SIZE: usize = 8;

pub const PAGE_SIZE: usize = 64;
pub const EEPROM_TOTAL_SIZE: usize = 16_384;

pub const RADIO_SLOT_A_ADDR: u16 = 0x0000;
pub const RADIO_SLOT_B_ADDR: u16 = 0x0100;
pub const RADIO_SLOT_SIZE: usize = 256;

pub const MODEL_BASE_ADDR: u16 = 0x0200;
pub const MODEL_SLOT_SIZE: usize = 768;

#[inline(always)]
pub const fn model_slot_addr(index: usize) -> Option<u16> {
    if index >= NUM_MODELS {
        None
    } else {
        Some(MODEL_BASE_ADDR + (index as u16) * (MODEL_SLOT_SIZE as u16))
    }
}

/// Calibration parameters for a single analog channel (8 bytes).
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ChannelCalib {
    pub min: u16,
    pub center: u16,
    pub max: u16,
    pub _pad: u16,
}

impl ChannelCalib {
    pub const fn new(min: u16, center: u16, max: u16) -> Self {
        Self {
            min,
            center,
            max,
            _pad: 0,
        }
    }
}

/// Compact calibration parameters for an auxiliary potentiometer/slider (6 bytes).
/// Supports both standard pots (with calculated midpoint) and detented pots (with fixed detent center).
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct PotCalib {
    pub min: u16,
    pub center: u16,
    pub max: u16,
}

impl PotCalib {
    pub const fn new(min: u16, center: u16, max: u16) -> Self {
        Self { min, center, max }
    }
}

/// Universal input mode for auxiliary analog channels (SA..SD, VRA..VRB, VRC..VRF).
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum AdcInputMode {
    #[default]
    Default = 0,
    Off = 1,
    TwoPos = 2,
    ThreePos = 3,
    SixPos = 4,
    Pot = 5,
    PotDetent = 6,
    InstantTrim = 7,
}

impl AdcInputMode {
    pub fn from_u8(val: u8) -> Self {
        match val {
            1 => Self::Off,
            2 => Self::TwoPos,
            3 => Self::ThreePos,
            4 => Self::SixPos,
            5 => Self::Pot,
            6 => Self::PotDetent,
            7 => Self::InstantTrim,
            _ => Self::Default,
        }
    }

    /// Check if this mode represents any analog potentiometer (continuous or center-detent).
    pub fn is_pot(&self) -> bool {
        matches!(self, Self::Pot | Self::PotDetent)
    }

    /// Check if this mode utilizes a physical center detent.
    pub fn has_detent(&self) -> bool {
        matches!(self, Self::PotDetent)
    }

    /// Check if this input is disabled / turned off.
    pub fn is_off(&self) -> bool {
        matches!(self, Self::Off)
    }

    /// Resolve effective mode for a given channel index (0..3: SA..SD, 4..5: VRA..VRB, 6..10: VRC..VRG)
    pub fn resolve(channel_idx: usize, mode_val: u8) -> Self {
        let mode = Self::from_u8(mode_val);
        if mode != Self::Default {
            return mode;
        }
        // Default hardware mapping:
        match channel_idx {
            0 => Self::TwoPos,               // SA (2-pos stock)
            1 => crate::hw::DEFAULT_SB_MODE, // SB (2-pos stock on i6X, 3-pos stock on i6S)
            2 => Self::ThreePos,             // SC (3-pos stock)
            3 => Self::TwoPos,               // SD (2-pos stock)
            _ => Self::Pot,                  // VRA..VRG (Pots stock)
        }
    }
}

/// Persistent system/radio-level configuration (140 bytes, 4-byte aligned for STM32F0 Flash programming).
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct RadioConfig {
    pub magic: u32,                // 0..4 ("FSKY")
    pub version: u32,              // 4..8 (5)
    pub active_model: u8,          // 8 (0..19, active model index)
    pub throttle_trim: u8,         // 9 (0: Off/Lock, 1: Idle T-Trim, 2: Linear)
    pub audio_enabled: u8,         // 10 (0: Muted, 1: Enabled)
    pub backlight_timeout: u8,     // 11 (0: Always On, 1: 15s, 2: 30s, 3: 60s)
    pub backlight_brightness: u8,  // 12 (1..10)
    pub vbat_warn_deci: u8,        // 13 (40..50 = 4.0V..5.0V, default 44 = 4.4V)
    pub lcd_contrast: u8,          // 14 (15..55, default 37 / 0x25)
    pub usb_mode: u8,              // 15 (0: Off, 1: Joystick, 2: Serial, 3: Composite)
    pub sticks: [ChannelCalib; 4], // 16..48 (32 bytes: Roll, Pitch, Throttle, Yaw)
    pub aux_pots: [PotCalib; 11],  // 48..114 (66 bytes: SA..SD, VRA..VRB, VRC..VRG)
    pub ext_module_pwr: u8,        // 114 (0: Active HIGH / N-type, 1: Active LOW / P-type)
    pub tone_style: u8,            // 115 (0: Simple / Standard, 1: Rich / Melodic)
    pub servo_rate_hz: u16,        // 116..118 (50..400 Hz, default 50 Hz for analog servo safety)
    pub rx_out_mode: u8,           // 118 (0: PWM, 1: PPM, default 0)
    pub rx_serial_proto: u8,       // 119 (0: i-BUS, 1: S.BUS, default 0)
    pub ext_switches: u8,          // 120 (0: Disabled, 1: Enabled / PC12+PC15)
    pub ext_adc: u8,               // 121 (0: Disabled, 1: Enabled / Header P7 AD11-AD15 or PC1-PC5)
    pub adc_modes: [u8; 11],       // 122..133: Input modes for the 11 auxiliary analog channels
    pub stick_mode: u8,            // 133 (0: Mode 1, 1: Mode 2, 2: Mode 3, 3: Mode 4)
    pub telem_slots: [u8; 4],      // 134..138: Configurable telemetry dashboard sensor slots (1..4)
    pub trainer_config: u8,        // 138: Bit 0: PPM Out on PF10, Bit 1: PPM In on PF9
    pub rear_left_func: u8,        // 139: 0: RC Switch (SWE), 1: Timer Reset, 2: Trims Modifier
    pub rear_right_func: u8,       // 140: 0: RC Switch (SWF), 1: Instant Trim, 2: Trims Modifier
    pub touch_disabled: u8,        // 141: 0: Touch Enabled, 1: Touch Disabled
    pub j4_sg_sh_en: u8,           // 142: 0: SWD Debug, 1: SG/SH Buttons on PA13/PA14
    pub switch_beep: u8,           // 143: 0: Muted, 1: Beep on switch toggle
    pub gimbal_type: u8,           // 144: 0: Standard Potentiometer, 1: Hall Effect
    pub stick_invert_mask: u8,     // 145: 4 bits (bit 0: RH, bit 1: RV, bit 2: LV, bit 3: LH), default 0x03 (RH+RV inverted)
    pub _pad_align: [u8; 2],       // 146..148: Align to 148 bytes (4-byte word boundary)
}

impl RadioConfig {
    pub const fn default_factory() -> Self {
        Self {
            magic: FLASH_MAGIC,
            version: CONFIG_VERSION,
            active_model: 0,
            throttle_trim: 0,
            audio_enabled: 1,
            backlight_timeout: 0,
            backlight_brightness: 10,
            vbat_warn_deci: 44,
            lcd_contrast: 37,
            usb_mode: 0,
            sticks: [
                ChannelCalib::new(
                    crate::adc::ADC_MIN,
                    crate::adc::ADC_CENTER,
                    crate::adc::ADC_MAX,
                ), // Roll (Horizontal)
                ChannelCalib::new(
                    crate::adc::ADC_MIN,
                    crate::adc::ADC_CENTER,
                    crate::adc::ADC_MAX,
                ), // Pitch (Vertical)
                ChannelCalib::new(
                    crate::adc::ADC_MIN,
                    crate::adc::ADC_CENTER,
                    crate::adc::ADC_MAX,
                ), // Throttle (Vertical)
                ChannelCalib::new(
                    crate::adc::ADC_MIN,
                    crate::adc::ADC_CENTER,
                    crate::adc::ADC_MAX,
                ), // Yaw (Horizontal)
            ],
            aux_pots: [
                PotCalib::new(
                    crate::adc::ADC_MIN,
                    crate::adc::ADC_CENTER,
                    crate::adc::ADC_MAX,
                ), // SA
                PotCalib::new(
                    crate::adc::ADC_MIN,
                    crate::adc::ADC_CENTER,
                    crate::adc::ADC_MAX,
                ), // SB
                PotCalib::new(
                    crate::adc::ADC_MIN,
                    crate::adc::ADC_CENTER,
                    crate::adc::ADC_MAX,
                ), // SC
                PotCalib::new(
                    crate::adc::ADC_MIN,
                    crate::adc::ADC_CENTER,
                    crate::adc::ADC_MAX,
                ), // SD
                PotCalib::new(
                    crate::adc::ADC_MIN,
                    crate::adc::ADC_CENTER,
                    crate::adc::ADC_MAX,
                ), // VRA
                PotCalib::new(
                    crate::adc::ADC_MIN,
                    crate::adc::ADC_CENTER,
                    crate::adc::ADC_MAX,
                ), // VRB
                PotCalib::new(
                    crate::adc::ADC_MIN,
                    crate::adc::ADC_CENTER,
                    crate::adc::ADC_MAX,
                ), // VRC
                PotCalib::new(
                    crate::adc::ADC_MIN,
                    crate::adc::ADC_CENTER,
                    crate::adc::ADC_MAX,
                ), // VRD
                PotCalib::new(
                    crate::adc::ADC_MIN,
                    crate::adc::ADC_CENTER,
                    crate::adc::ADC_MAX,
                ), // VRE
                PotCalib::new(
                    crate::adc::ADC_MIN,
                    crate::adc::ADC_CENTER,
                    crate::adc::ADC_MAX,
                ), // VRF
                PotCalib::new(
                    crate::adc::ADC_MIN,
                    crate::adc::ADC_CENTER,
                    crate::adc::ADC_MAX,
                ), // VRG
            ],
            ext_module_pwr: 0,
            tone_style: 1,
            servo_rate_hz: 50,
            rx_out_mode: 0,
            rx_serial_proto: 0,
            ext_switches: 0,
            ext_adc: 0,
            adc_modes: [0; 11], // Default hardware modes
            stick_mode: 1,      // Mode 2 default
            telem_slots: [0, 1, 2, 3], // Default: RxBat, RSSI/LQ, TxBat, Current/Link
            trainer_config: 0,
            rear_left_func: 0,
            rear_right_func: 0,
            touch_disabled: 0,
            j4_sg_sh_en: 0,
            gimbal_type: 0,
            stick_invert_mask: 0x03, // RH (bit 0) and RV (bit 1) inverted by default for factory pot gimbals
            _pad_align: [0; 2],
        }
    }

    /// Check if aftermarket Hall effect gimbals mode is enabled.
    #[inline(always)]
    pub fn is_hall_gimbals(&self) -> bool {
        self.gimbal_type == 1
    }

    /// Check if a specific stick axis is inverted (0: RH, 1: RV, 2: LV, 3: LH).
    #[inline(always)]
    pub fn is_stick_inverted(&self, stick_idx: usize) -> bool {
        (self.stick_invert_mask & (1 << (stick_idx & 3))) != 0
    }

    /// Set stick axis inversion bit (0: RH, 1: RV, 2: LV, 3: LH).
    #[inline(always)]
    pub fn set_stick_inverted(&mut self, stick_idx: usize, inverted: bool) {
        let bit = 1 << (stick_idx & 3);
        if inverted {
            self.stick_invert_mask |= bit;
        } else {
            self.stick_invert_mask &= !bit;
        }
    }

    /// Check if PPM Output on PF10 is enabled.
    #[inline(always)]
    pub fn is_ppm_out_enabled(&self) -> bool {
        (self.trainer_config & 0x01) != 0
    }

    /// Check if PPM Input on PF9 is enabled.
    #[inline(always)]
    pub fn is_ppm_in_enabled(&self) -> bool {
        (self.trainer_config & 0x02) != 0
    }

    /// Set PPM Output on PF10 enabled state.
    #[inline(always)]
    pub fn set_ppm_out_enabled(&mut self, enabled: bool) {
        if enabled {
            self.trainer_config |= 0x01;
        } else {
            self.trainer_config &= !0x01;
        }
    }

    /// Set PPM Input on PF9 enabled state.
    #[inline(always)]
    pub fn set_ppm_in_enabled(&mut self, enabled: bool) {
        if enabled {
            self.trainer_config |= 0x02;
        } else {
            self.trainer_config &= !0x02;
        }
    }

    /// Get raw configured ADC mode index (0..=6) for auxiliary input `ch` (`0..10`).
    #[inline(always)]
    pub const fn get_adc_mode_val(&self, ch: usize) -> u8 {
        if ch < 11 {
            self.adc_modes[ch]
        } else {
            0
        }
    }

    /// Set raw configured ADC mode index (0..=7) for auxiliary input `ch` (`0..10`).
    #[inline(always)]
    pub fn set_adc_mode_val(&mut self, ch: usize, val: u8) {
        if ch < 11 {
            self.adc_modes[ch] = val.min(7);
        }
    }

    /// Get the effective AdcInputMode for an auxiliary channel (0..10: SA..SD, VRA..VRB, VRC..VRG).
    pub fn get_adc_mode(&self, ch: usize) -> AdcInputMode {
        if ch < 11 {
            AdcInputMode::resolve(ch, self.adc_modes[ch])
        } else {
            AdcInputMode::Default
        }
    }

    /// Get the configured raw AdcInputMode (including Default).
    pub fn get_raw_adc_mode(&self, ch: usize) -> AdcInputMode {
        if ch < 11 {
            AdcInputMode::from_u8(self.adc_modes[ch])
        } else {
            AdcInputMode::Default
        }
    }
}

pub const NUM_MIX_LINES: usize = 32;

/// A single freeform mix rule in the matrix mixer (8 bytes).
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct MixLine {
    pub target_ch: u8, // 0: Disabled, 1..18: Target Channel CH1..CH18
    pub source: u8, // 0: None, 1: Roll, 2: Pitch, 3: Thr, 4: Yaw, 5: VRA, 6: VRB, 7: SA, 8: SB, 9: SC, 10: SD, 11: MAX, 12..29: CH1..CH18, 30: ThrUnipolar, 31..40: SE..VRF
    pub weight_src: u8, // 0: Fixed Weight, 1..18: Modulate with CH1..CH18 (Bipolar), 19..36: Modulate (Unipolar 0..100%)
    pub weight: i8, // -100% .. +100%
    pub offset: i8, // -100% .. +100%
    pub switch: u8, // 0: Always On, 1..22: SA_UP..SWJ_DN
    pub mode: u8,   // 0: Add (+), 1: Multiply (*), 2: Replace (:=)
    pub _reserved: u8, // 1-byte padding for 8-byte alignment
}

impl MixLine {
    pub const fn disabled() -> Self {
        Self {
            target_ch: 0,
            source: 0,
            weight_src: 0,
            weight: 100,
            offset: 0,
            switch: 0,
            mode: 0,
            _reserved: 0,
        }
    }
}

/// Complete profile for an individual aircraft/model (exactly 348 bytes).
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ModelConfig {
    pub name: [u8; 10],         // 0..10: 10-char ASCII name
    pub model_type: u8,         // 10: 0: Airplane, 1: Glider, 2: Heli, 3: Quad, 4: General
    pub _pad0: u8,              // 11: align rx_id to 4 bytes
    pub rx_id: u32,             // 12..16: Model Match bound receiver ID
    pub trims: [i8; 4],         // 16..20: Roll, Pitch, Throttle, Yaw (-25..+25)
    pub channel_reverse: u32,   // 20..24: Bitmask for CH1..CH18 inversion
    pub dr_switch: u8,          // 24: 0: None, 1: SA, 2: SB, 3: SC, 4: SD, 5: SE, 6: SF
    pub thr_curve_pts: u8,      // 25: 5, 7, or 9 points mode
    pub thr_curve_smooth: u8,   // 26: 0: Linear, 1: Spline / Smooth
    pub thr_curve: [u8; 17],    // 27..44: Points buffer (0..100%, reserved up to 17)
    pub _pad_crv: u8,           // 44..45
    pub dr_high: [u8; 3],       // 45..48: High rate (AIL, ELE, RUD: 50..100%)
    pub dr_low: [u8; 3],        // 48..51: Low rate (AIL, ELE, RUD: 30..100%)
    pub expo_high: [i8; 3],     // 51..54: High expo (-100..+100%)
    pub expo_low: [i8; 3],      // 54..57: Low expo (-100..+100%)
    pub _pad1: u8,              // 57..58: align timer_secs to 2-byte boundary
    pub timer_secs: u16,        // 58..60: Countdown timer in seconds (e.g. 300 = 5 min)
    pub timer_source: u8, // 60: 0: Off, 1: THs (Thr>5%), 2: THt (Thr Latched), 3: Always On, 4..17: SA^..SFv
    pub rf_power: u8, // 61: AFHDS 2A RF power (0: 100mW, 1: 10mW, 2: 1mW, 3: 100uW)
    pub failsafe_thr: u16, // 62..64: Failsafe throttle pulse in µs (e.g. 1000)
    pub aux_channels: [u8; 14], // 64..78: Source for CH5..CH18
    pub wing_tail_mix: u8, // 78: 0: Normal, 1: Elevon/Delta, 2: V-Tail, 3: Flaperon
    pub template_diff: i8, // 79: Differential / mix ratio (-100..+100)
    pub mixes: [MixLine; NUM_MIX_LINES], // 80..336: 32 freeform mix rules (32 * 8 = 256 bytes)
    pub failsafe_mode: u8, // 336: 0: Hold last, 1: Custom pulses
    pub failsafe_timeout: u8, // 337: 10..50 (1.0s..5.0s)
    pub rf_protocol: u8,  // 338: 0: AFHDS 2A, 1: CRSF / ELRS
    pub crsf_baud: u8,    // 339: 0: 115.2k, 1: 416.6k, 2: 420k, 3: 921.6k, 4: 1.875M
    pub arm_switch: u8, // 340: 0: None, 1..14: SA^..SFv
    pub rx_out_mode: u8, // 341: 0: PWM, 1: PPM (default 0)
    pub servo_rate_hz: u16, // 342..344: 50..400 Hz (default 50 Hz)
    pub rx_serial_proto: u8, // 344: 0: i-BUS, 1: S.BUS (default 0)
    pub crsf_half_duplex: u8, // 345: 0: Full-Duplex (2-Wire), 1: Half-Duplex (1-Wire)
    pub _pad_end: [u8; 2], // 346..348: Align struct to 4-byte word boundary
    pub gvars: [i16; 5],   // 348..358: Global Variables GV1..GV5 (-1000..+1000)
    pub _pad_gvar: [u8; 2], // 358..360: Align to 4-byte word boundary
}

impl ModelConfig {
    pub const fn default_for_index(idx: usize) -> Self {
        let num = (idx + 1) as u8;
        let digit1 = b'0' + (num / 10);
        let digit2 = b'0' + (num % 10);
        Self {
            name: [
                b'M', b'O', b'D', b'E', b'L', b' ', digit1, digit2, b' ', b' ',
            ],
            model_type: 0,
            _pad0: 0,
            rx_id: 0,
            trims: [0, 0, 0, 0],
            channel_reverse: 0,
            dr_switch: 0,
            thr_curve_pts: 5,
            thr_curve_smooth: 0,
            thr_curve: [0, 25, 50, 75, 100, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            _pad_crv: 0,
            dr_high: [100, 100, 100],
            dr_low: [70, 70, 70],
            expo_high: [0, 0, 0],
            expo_low: [0, 0, 0],
            _pad1: 0,
            timer_secs: 300,
            timer_source: 1,
            rf_power: 0,
            failsafe_thr: 1000,
            aux_channels: [7, 8, 5, 6, 9, 10, 0, 0, 0, 0, 0, 0, 0, 0],
            wing_tail_mix: 0,
            template_diff: 0,
            mixes: [MixLine::disabled(); NUM_MIX_LINES],
            failsafe_mode: 0,
            failsafe_timeout: 20,
            rf_protocol: 0,
            crsf_baud: 2,
            arm_switch: 0,
            servo_rate_hz: 50,
            rx_out_mode: 0,
            rx_serial_proto: 0,
            crsf_half_duplex: 0,
            _pad_end: [0; 2],
            gvars: [0; 5],
            _pad_gvar: [0; 2],
        }
    }

    #[inline(always)]
    pub fn model_type(&self) -> crate::safety::ModelType {
        crate::safety::ModelType::from_u8(self.model_type)
    }

    #[inline(always)]
    pub fn set_model_type(&mut self, mtype: crate::safety::ModelType) {
        self.model_type = mtype.to_u8();
    }
}

/// Compact storage layout containing radio settings and active model (488 bytes).
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct RadioStorage {
    pub radio: RadioConfig,
    pub active_model: ModelConfig,
}

impl RadioStorage {
    pub const fn empty() -> Self {
        Self {
            radio: RadioConfig::default_factory(),
            active_model: ModelConfig::default_for_index(0),
        }
    }

    #[allow(dead_code)]
    pub const fn default_factory() -> Self {
        Self::empty()
    }

    pub fn init_default(&mut self) {
        self.radio = RadioConfig::default_factory();
        self.active_model = ModelConfig::default_for_index(0);
    }

    #[inline(always)]
    pub fn active_model(&self) -> &ModelConfig {
        &self.active_model
    }

    #[inline(always)]
    pub fn active_model_mut(&mut self) -> &mut ModelConfig {
        &mut self.active_model
    }

    /// Check if a mix source ID (0..40, matching `SOURCE_NAMES`) is available on this radio.
    /// Filters out unconfigured switches, potentiometers, and unmapped channels.
    pub fn is_source_available(&self, src: u8) -> bool {
        match src {
            0..=4 => true, // None, Roll, Pitch, Thr, Yaw are always available
            5 => self.radio.get_adc_mode(4).is_pot(), // VRA (ADC ch 4)
            6 => self.radio.get_adc_mode(5).is_pot(), // VRB (ADC ch 5)
            7 => matches!(self.radio.get_adc_mode(0), AdcInputMode::TwoPos | AdcInputMode::ThreePos | AdcInputMode::InstantTrim), // SA
            8 => matches!(self.radio.get_adc_mode(1), AdcInputMode::TwoPos | AdcInputMode::ThreePos | AdcInputMode::InstantTrim), // SB
            9 => matches!(self.radio.get_adc_mode(2), AdcInputMode::TwoPos | AdcInputMode::ThreePos | AdcInputMode::InstantTrim), // SC
            10 => matches!(self.radio.get_adc_mode(3), AdcInputMode::TwoPos | AdcInputMode::ThreePos | AdcInputMode::InstantTrim), // SD
            11 => true, // MAX constant is always available
            12..=15 => true, // CH1..CH4 primary flight channels are always active
            16..=29 => {
                // CH5..CH18: available if assigned to an active source or targeted by a mix line
                let ch_idx = (src - 12) as usize; // 4..17
                let aux_idx = ch_idx - 4; // 0..13
                if self.active_model.wing_tail_mix == 3 && ch_idx == 5 {
                    return true; // Flaperon drives CH6
                }
                if self.active_model.aux_channels[aux_idx] != 0 {
                    return true;
                }
                self.active_model.mixes.iter().any(|m| m.target_ch == (ch_idx + 1) as u8)
            }
            30 => true, // Thr+ is always available
            31 => self.radio.ext_switches != 0, // SE
            32 => self.radio.ext_switches != 0, // SF
            33 => self.radio.j4_sg_sh_en != 0,   // SG
            34 => self.radio.j4_sg_sh_en != 0,   // SH
            35 => self.radio.ext_adc != 0 && matches!(self.radio.get_adc_mode(6), AdcInputMode::TwoPos | AdcInputMode::ThreePos), // SWI (ADC ch 6 / PC1)
            36 => self.radio.ext_adc != 0 && matches!(self.radio.get_adc_mode(7), AdcInputMode::TwoPos | AdcInputMode::ThreePos), // SWJ (ADC ch 7 / PC2)
            37 => self.radio.ext_adc != 0 && self.radio.get_adc_mode(6).is_pot(), // VRC (ADC ch 6 / PC1)
            38 => self.radio.ext_adc != 0 && self.radio.get_adc_mode(7).is_pot(), // VRD (ADC ch 7 / PC2)
            39 => self.radio.ext_adc != 0 && self.radio.get_adc_mode(8).is_pot(), // VRE (ADC ch 8 / PC3)
            40 => self.radio.ext_adc != 0 && self.radio.get_adc_mode(9).is_pot(), // VRF (ADC ch 9 / PC4)
            41 => self.radio.ext_adc != 0 && self.radio.get_adc_mode(10).is_pot(), // VRG (ADC ch 10 / PC5)
            42 => self.radio.ext_adc != 0 && matches!(self.radio.get_adc_mode(8), AdcInputMode::TwoPos | AdcInputMode::ThreePos), // SWK (ADC ch 8 / PC3)
            43 => self.radio.ext_adc != 0 && matches!(self.radio.get_adc_mode(9), AdcInputMode::TwoPos | AdcInputMode::ThreePos), // SWL (ADC ch 9 / PC4)
            44 => self.radio.ext_adc != 0 && matches!(self.radio.get_adc_mode(10), AdcInputMode::TwoPos | AdcInputMode::ThreePos), // SWM (ADC ch 10 / PC5)
            45..=49 => true, // GV1..GV5 (Global Variables) are always available
            _ => false,
        }
    }

    /// Check if a switch condition index (0..28, matching `SWITCH_COND_NAMES`) is valid given hardware config.
    pub fn is_switch_cond_available(&self, cond: u8) -> bool {
        match cond {
            0 => true, // Always ON
            1 | 2 => matches!(self.radio.get_adc_mode(0), AdcInputMode::TwoPos | AdcInputMode::ThreePos | AdcInputMode::InstantTrim), // SA
            3 | 5 => matches!(self.radio.get_adc_mode(1), AdcInputMode::TwoPos | AdcInputMode::ThreePos | AdcInputMode::InstantTrim), // SB Up, Down
            4 => self.radio.get_adc_mode(1) == AdcInputMode::ThreePos, // SB Mid
            6 | 8 => matches!(self.radio.get_adc_mode(2), AdcInputMode::TwoPos | AdcInputMode::ThreePos | AdcInputMode::InstantTrim), // SC Up, Down
            7 => self.radio.get_adc_mode(2) == AdcInputMode::ThreePos, // SC Mid
            9 | 10 => matches!(self.radio.get_adc_mode(3), AdcInputMode::TwoPos | AdcInputMode::ThreePos | AdcInputMode::InstantTrim), // SD
            11 | 12 => self.radio.ext_switches != 0, // SE
            13 | 14 => self.radio.ext_switches != 0, // SF
            15 | 16 => self.radio.j4_sg_sh_en != 0, // SG
            17 | 18 => self.radio.j4_sg_sh_en != 0, // SH
            19 | 20 => self.radio.ext_adc != 0 && matches!(self.radio.get_adc_mode(6), AdcInputMode::TwoPos | AdcInputMode::ThreePos), // SWI
            21 | 22 => self.radio.ext_adc != 0 && matches!(self.radio.get_adc_mode(7), AdcInputMode::TwoPos | AdcInputMode::ThreePos), // SWJ
            23 | 24 => self.radio.ext_adc != 0 && matches!(self.radio.get_adc_mode(8), AdcInputMode::TwoPos | AdcInputMode::ThreePos), // SWK
            25 | 26 => self.radio.ext_adc != 0 && matches!(self.radio.get_adc_mode(9), AdcInputMode::TwoPos | AdcInputMode::ThreePos), // SWL
            27 | 28 => self.radio.ext_adc != 0 && matches!(self.radio.get_adc_mode(10), AdcInputMode::TwoPos | AdcInputMode::ThreePos), // SWM
            _ => false,
        }
    }

    /// Check if a channel index (1..=18) is configured / active in the active model.
    pub fn is_channel_configured(&self, ch: u8) -> bool {
        if ch == 0 || ch > 18 {
            return false;
        }
        if ch <= 4 {
            return true; // CH1..CH4 primary controls
        }
        let ch_idx = (ch - 1) as usize; // 4..17
        let aux_idx = ch_idx - 4; // 0..13
        if self.active_model.wing_tail_mix == 3 && ch == 6 {
            return true; // Flaperon right aileron
        }
        if self.active_model.aux_channels[aux_idx] != 0 {
            return true;
        }
        self.active_model.mixes.iter().any(|m| m.target_ch == ch)
    }

    /// Check if a weight modulation source index (0..=41, matching `WEIGHT_SRC_NAMES`) is available.
    pub fn is_weight_src_available(&self, ws: u8) -> bool {
        match ws {
            0 => true, // Fixed weight
            1..=18 => self.is_channel_configured(ws), // Bipolar CH1..CH18
            19..=36 => self.is_channel_configured(ws - 18), // Unipolar CH1+..CH18+
            37..=41 => true, // GV1..GV5 (Global Variables) are always available
            _ => false,
        }
    }

    /// Find the next available mix source index, wrapping around.
    pub fn next_available_source(&self, cur: u8, max_len: usize) -> u8 {
        let max = max_len as u8;
        let mut candidate = (cur + 1) % max;
        while candidate != cur {
            if self.is_source_available(candidate) {
                return candidate;
            }
            candidate = (candidate + 1) % max;
        }
        cur
    }

    /// Find the previous available mix source index, wrapping around.
    pub fn prev_available_source(&self, cur: u8, max_len: usize) -> u8 {
        let max = max_len as u8;
        let mut candidate = if cur == 0 { max - 1 } else { cur - 1 };
        while candidate != cur {
            if self.is_source_available(candidate) {
                return candidate;
            }
            candidate = if candidate == 0 { max - 1 } else { candidate - 1 };
        }
        cur
    }

    /// Find the next available weight modulation source index, wrapping around.
    pub fn next_available_weight_src(&self, cur: u8, max_len: usize) -> u8 {
        let max = max_len as u8;
        let mut candidate = (cur + 1) % max;
        while candidate != cur {
            if self.is_weight_src_available(candidate) {
                return candidate;
            }
            candidate = (candidate + 1) % max;
        }
        cur
    }

    /// Find the previous available weight modulation source index, wrapping around.
    pub fn prev_available_weight_src(&self, cur: u8, max_len: usize) -> u8 {
        let max = max_len as u8;
        let mut candidate = if cur == 0 { max - 1 } else { cur - 1 };
        while candidate != cur {
            if self.is_weight_src_available(candidate) {
                return candidate;
            }
            candidate = if candidate == 0 { max - 1 } else { candidate - 1 };
        }
        cur
    }

    /// Find the next available switch condition index, wrapping around.
    pub fn next_available_switch_cond(&self, cur: u8, max_len: usize) -> u8 {
        let max = max_len as u8;
        let mut candidate = (cur + 1) % max;
        while candidate != cur {
            if self.is_switch_cond_available(candidate) {
                return candidate;
            }
            candidate = (candidate + 1) % max;
        }
        cur
    }

    /// Find the previous available switch condition index, wrapping around.
    pub fn prev_available_switch_cond(&self, cur: u8, max_len: usize) -> u8 {
        let max = max_len as u8;
        let mut candidate = if cur == 0 { max - 1 } else { cur - 1 };
        while candidate != cur {
            if self.is_switch_cond_available(candidate) {
                return candidate;
            }
            candidate = if candidate == 0 { max - 1 } else { candidate - 1 };
        }
        cur
    }

    /// Check if a physical switch index (0..6: None, SA..SF) is available for Dual Rate selection.
    pub fn is_dr_switch_available(&self, sw: u8) -> bool {
        match sw {
            0 => true, // None
            1 => matches!(self.radio.get_adc_mode(0), AdcInputMode::TwoPos | AdcInputMode::ThreePos | AdcInputMode::InstantTrim), // SA
            2 => matches!(self.radio.get_adc_mode(1), AdcInputMode::TwoPos | AdcInputMode::ThreePos | AdcInputMode::InstantTrim), // SB
            3 => matches!(self.radio.get_adc_mode(2), AdcInputMode::TwoPos | AdcInputMode::ThreePos | AdcInputMode::InstantTrim), // SC
            4 => matches!(self.radio.get_adc_mode(3), AdcInputMode::TwoPos | AdcInputMode::ThreePos | AdcInputMode::InstantTrim), // SD
            5 => self.radio.ext_switches != 0, // SE
            6 => self.radio.ext_switches != 0, // SF
            _ => false,
        }
    }

    /// Find next available Dual Rate switch (0..6).
    pub fn next_available_dr_switch(&self, cur: u8) -> u8 {
        let mut candidate = (cur + 1) % 7;
        while candidate != cur {
            if self.is_dr_switch_available(candidate) {
                return candidate;
            }
            candidate = (candidate + 1) % 7;
        }
        cur
    }

    /// Find previous available Dual Rate switch (0..6).
    pub fn prev_available_dr_switch(&self, cur: u8) -> u8 {
        let mut candidate = if cur == 0 { 6 } else { cur - 1 };
        while candidate != cur {
            if self.is_dr_switch_available(candidate) {
                return candidate;
            }
            candidate = if candidate == 0 { 6 } else { candidate - 1 };
        }
        cur
    }

    /// Check if an AUX channel input source (from `crate::hw::AUX_SOURCES`) is hardware-available.
    pub fn is_aux_source_available(&self, src: u8) -> bool {
        match src {
            0..=4 => true, // None, Roll, Pitch, Thr, Yaw
            5 => self.radio.get_adc_mode(4).is_pot(), // VRA
            6 => self.radio.get_adc_mode(5).is_pot(), // VRB
            7 => matches!(self.radio.get_adc_mode(0), AdcInputMode::TwoPos | AdcInputMode::ThreePos | AdcInputMode::InstantTrim), // SA
            8 => matches!(self.radio.get_adc_mode(1), AdcInputMode::TwoPos | AdcInputMode::ThreePos | AdcInputMode::InstantTrim), // SB
            9 => matches!(self.radio.get_adc_mode(2), AdcInputMode::TwoPos | AdcInputMode::ThreePos | AdcInputMode::InstantTrim), // SC
            10 => matches!(self.radio.get_adc_mode(3), AdcInputMode::TwoPos | AdcInputMode::ThreePos | AdcInputMode::InstantTrim), // SD
            31 => self.radio.ext_switches != 0, // SE
            32 => self.radio.ext_switches != 0, // SF
            33 => self.radio.j4_sg_sh_en != 0,   // SG
            34 => self.radio.j4_sg_sh_en != 0,   // SH
            35 => self.radio.ext_adc != 0 && matches!(self.radio.get_adc_mode(6), AdcInputMode::TwoPos | AdcInputMode::ThreePos), // SWI
            36 => self.radio.ext_adc != 0 && matches!(self.radio.get_adc_mode(7), AdcInputMode::TwoPos | AdcInputMode::ThreePos), // SWJ
            37 => self.radio.ext_adc != 0 && self.radio.get_adc_mode(6).is_pot(), // VRC
            38 => self.radio.ext_adc != 0 && self.radio.get_adc_mode(7).is_pot(), // VRD
            39 => self.radio.ext_adc != 0 && self.radio.get_adc_mode(8).is_pot(), // VRE
            40 => self.radio.ext_adc != 0 && self.radio.get_adc_mode(9).is_pot(), // VRF
            41 => self.radio.ext_adc != 0 && self.radio.get_adc_mode(10).is_pot(), // VRG
            42 => self.radio.ext_adc != 0 && matches!(self.radio.get_adc_mode(8), AdcInputMode::TwoPos | AdcInputMode::ThreePos), // SWK
            43 => self.radio.ext_adc != 0 && matches!(self.radio.get_adc_mode(9), AdcInputMode::TwoPos | AdcInputMode::ThreePos), // SWL
            44 => self.radio.ext_adc != 0 && matches!(self.radio.get_adc_mode(10), AdcInputMode::TwoPos | AdcInputMode::ThreePos), // SWM
            _ => false,
        }
    }

    /// Cycle to next available AUX source in a given slice of source IDs.
    pub fn next_available_aux_source(&self, cur: u8, sources: &[u8]) -> u8 {
        let cur_pos = sources.iter().position(|&s| s == cur).unwrap_or(0);
        let len = sources.len();
        let mut idx = (cur_pos + 1) % len;
        while idx != cur_pos {
            let cand = sources[idx];
            if self.is_aux_source_available(cand) {
                return cand;
            }
            idx = (idx + 1) % len;
        }
        cur
    }

    /// Cycle to previous available AUX source in a given slice of source IDs.
    pub fn prev_available_aux_source(&self, cur: u8, sources: &[u8]) -> u8 {
        let cur_pos = sources.iter().position(|&s| s == cur).unwrap_or(0);
        let len = sources.len();
        let mut idx = if cur_pos == 0 { len - 1 } else { cur_pos - 1 };
        while idx != cur_pos {
            let cand = sources[idx];
            if self.is_aux_source_available(cand) {
                return cand;
            }
            idx = if idx == 0 { len - 1 } else { idx - 1 };
        }
        cur
    }

    /// Check if an Arm Switch condition (0..=14: 0: None, 1..=14: SA^..SFv) is available.
    pub fn is_arm_switch_available(&self, sw: u8) -> bool {
        match sw {
            0 => true, // None
            1..=14 => self.is_switch_cond_available(sw),
            _ => false,
        }
    }

    /// Cycle to next available Arm Switch (0..=14).
    pub fn next_available_arm_switch(&self, cur: u8) -> u8 {
        let mut candidate = (cur + 1) % 15;
        while candidate != cur {
            if self.is_arm_switch_available(candidate) {
                return candidate;
            }
            candidate = (candidate + 1) % 15;
        }
        cur
    }

    /// Cycle to previous available Arm Switch (0..=14).
    pub fn prev_available_arm_switch(&self, cur: u8) -> u8 {
        let mut candidate = if cur == 0 { 14 } else { cur - 1 };
        while candidate != cur {
            if self.is_arm_switch_available(candidate) {
                return candidate;
            }
            candidate = if candidate == 0 { 14 } else { candidate - 1 };
        }
        cur
    }

    /// Check if a Timer Source (0..=17: 0: Off, 1: THs, 2: THt, 3: Always On, 4..=17: SA^..SFv) is available.
    pub fn is_timer_source_available(&self, ts: u8) -> bool {
        match ts {
            0..=3 => true, // Off, THs, THt, Always On
            4..=17 => self.is_switch_cond_available(ts - 3),
            _ => false,
        }
    }

    /// Cycle to next available Timer Source (0..=17).
    pub fn next_available_timer_source(&self, cur: u8) -> u8 {
        let mut candidate = (cur + 1) % 18;
        while candidate != cur {
            if self.is_timer_source_available(candidate) {
                return candidate;
            }
            candidate = (candidate + 1) % 18;
        }
        cur
    }

    /// Cycle to previous available Timer Source (0..=17).
    pub fn prev_available_timer_source(&self, cur: u8) -> u8 {
        let mut candidate = if cur == 0 { 17 } else { cur - 1 };
        while candidate != cur {
            if self.is_timer_source_available(candidate) {
                return candidate;
            }
            candidate = if candidate == 0 { 17 } else { candidate - 1 };
        }
        cur
    }

    /// Sanitize all radio and model parameters to valid operating bounds.
    pub fn sanitize(&mut self) {
        self.radio.magic = FLASH_MAGIC;
        self.radio.version = CONFIG_VERSION;
        if self.radio.vbat_warn_deci < 35 || self.radio.vbat_warn_deci > 60 {
            self.radio.vbat_warn_deci = 44;
        }
        if self.radio.active_model >= NUM_MODELS as u8 {
            self.radio.active_model = 0;
        }
        if self.radio.backlight_brightness == 0 || self.radio.backlight_brightness > 10 {
            self.radio.backlight_brightness = 10;
        }
        if self.radio.lcd_contrast < 15 || self.radio.lcd_contrast > 55 {
            self.radio.lcd_contrast = 37;
        }
        if self.radio.usb_mode > 3 {
            self.radio.usb_mode = 0;
        }
        if self.radio.ext_module_pwr > 1 {
            self.radio.ext_module_pwr = 0;
        }
        if self.radio.throttle_trim > 2 {
            self.radio.throttle_trim = 0;
        }
        if self.radio.tone_style > 1 {
            self.radio.tone_style = 1;
        }
        if self.radio.servo_rate_hz < 50 || self.radio.servo_rate_hz > 400 {
            self.radio.servo_rate_hz = 50;
        }
        if self.radio.rx_out_mode > 1 {
            self.radio.rx_out_mode = 0;
        }
        if self.radio.rx_serial_proto > 1 {
            self.radio.rx_serial_proto = 0;
        }
        if self.radio.ext_switches > 1 {
            self.radio.ext_switches = 0;
        }
        if self.radio.rear_left_func > 2 {
            self.radio.rear_left_func = 0;
        }
        if self.radio.rear_right_func > 2 {
            self.radio.rear_right_func = 0;
        }
        if self.radio.touch_disabled > 1 {
            self.radio.touch_disabled = 0;
        }
        if self.radio.j4_sg_sh_en > 1 {
            self.radio.j4_sg_sh_en = 0;
        }
        if self.radio.switch_beep > 1 {
            self.radio.switch_beep = 1;
        }
        if self.radio.gimbal_type > 1 {
            self.radio.gimbal_type = 0;
        }
        if self.radio.stick_invert_mask > 0x0F {
            self.radio.stick_invert_mask = 0x03; // Default RH+RV inverted
        }

        for stick in self.radio.sticks.iter_mut() {
            if stick.min >= stick.center
                || stick.center >= stick.max
                || stick.max > crate::adc::ADC_MAX
            {
                stick.min = crate::adc::ADC_MIN;
                stick.center = crate::adc::ADC_CENTER;
                stick.max = crate::adc::ADC_MAX;
            }
        }
        if self.radio.ext_adc > 1 {
            self.radio.ext_adc = 0;
        }
        for pot in self.radio.aux_pots.iter_mut() {
            if pot.min >= pot.center || pot.center >= pot.max || pot.max > crate::adc::ADC_MAX {
                pot.min = crate::adc::ADC_MIN;
                pot.center = crate::adc::ADC_CENTER;
                pot.max = crate::adc::ADC_MAX;
            }
        }
        for mode in self.radio.adc_modes.iter_mut() {
            if *mode > 6 {
                *mode = 0; // Default
            }
        }
        for slot in self.radio.telem_slots.iter_mut() {
            if *slot > 12 {
                *slot = 12; // OFF
            }
        }
        if self.radio.trainer_config > 3 {
            self.radio.trainer_config = 0;
        }

        let m = &mut self.active_model;
        if m.model_type > 4 {
            m.model_type = 0;
        }
        if m.rf_protocol > 1 {
            m.rf_protocol = 0;
        }
        if m.crsf_baud > 4 {
            m.crsf_baud = 2;
        }
        if m.crsf_half_duplex > 1 {
            m.crsf_half_duplex = 0;
        }
        for axis in 0..3 {
            if m.dr_high[axis] < 30 || m.dr_high[axis] > 100 {
                m.dr_high[axis] = 100;
            }
            if m.dr_low[axis] < 30 || m.dr_low[axis] > 100 {
                m.dr_low[axis] = 70;
            }
            if m.expo_high[axis] < -100 || m.expo_high[axis] > 100 {
                m.expo_high[axis] = 0;
            }
            if m.expo_low[axis] < -100 || m.expo_low[axis] > 100 {
                m.expo_low[axis] = 0;
            }
        }
        if m.dr_switch > 6 {
            m.dr_switch = 0;
        }
        if m.wing_tail_mix > 3 {
            m.wing_tail_mix = 0;
        }
        if m.template_diff < -100 || m.template_diff > 100 {
            m.template_diff = 0;
        }
        if m.failsafe_thr < 900 || m.failsafe_thr > 2100 {
            m.failsafe_thr = 1000;
        }
        if m.aux_channels[0..6] == [0; 6] {
            m.aux_channels = [7, 8, 5, 6, 9, 10, 0, 0, 0, 0, 0, 0, 0, 0];
        }
        for src in m.aux_channels.iter_mut() {
            if *src > 40 {
                *src = 0;
            }
        }
        for mix in m.mixes.iter_mut() {
            if mix.target_ch > 18 || mix.source > 49 || mix.weight_src > 41 || mix.mode > 2 || mix.switch > 22 {
                *mix = MixLine::disabled();
            }
        }
        for gv in m.gvars.iter_mut() {
            *gv = (*gv).clamp(-1000, 1000);
        }
        if m.thr_curve_pts != 5 && m.thr_curve_pts != 7 && m.thr_curve_pts != 9 {
            m.thr_curve_pts = 5;
            m.thr_curve = [0, 25, 50, 75, 100, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        }
        if m.arm_switch > 14 {
            m.arm_switch = 0;
        }
        if m.timer_source > 17 {
            m.timer_source = 0;
        }
        if m.timer_secs > 3600 {
            m.timer_secs = 0;
        }
        if m.rf_power > 3 {
            m.rf_power = 0;
        }
        if m.servo_rate_hz < 50 || m.servo_rate_hz > 400 {
            m.servo_rate_hz = 50;
        }
        if m.rx_out_mode > 1 {
            m.rx_out_mode = 0;
        }
        if m.rx_serial_proto > 1 {
            m.rx_serial_proto = 0;
        }
        if m.name[0] == 0 || m.name[0] == 0xFF {
            let num = self.radio.active_model + 1;
            let digit1 = b'0' + (num / 10);
            let digit2 = b'0' + (num % 10);
            m.name = [
                b'M', b'O', b'D', b'E', b'L', b' ', digit1, digit2, b' ', b' ',
            ];
        }
    }
}

const _: () = assert!(core::mem::size_of::<RadioConfig>() == 148);
const _: () = assert!(core::mem::size_of::<MixLine>() == 8);
const _: () = assert!(core::mem::size_of::<ModelConfig>() == 360);
const _: () = assert!(core::mem::size_of::<RadioStorage>() == 508);

static mut GLOBAL_STORAGE: RadioStorage = RadioStorage::empty();

/// Get global mutable reference to persistent RadioStorage.
#[allow(static_mut_refs)]
pub fn get_storage() -> &'static mut RadioStorage {
    unsafe { &mut GLOBAL_STORAGE }
}

/// CCITT CRC-16 (polynomial 0x1021, init 0xFFFF).
pub fn crc16(data: &[u8]) -> u16 {
    let mut crc = 0xFFFFu16;
    for &byte in data {
        crc ^= (byte as u16) << 8;
        for _ in 0..8 {
            if (crc & 0x8000) != 0 {
                crc = (crc << 1) ^ 0x1021;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum StorageError {
    I2cError,
    CrcMismatch,
    VersionMismatch(u8),
    LengthMismatch(u16),
    EmptySlot,
    SlotCorrupt,
}

/// 8-byte framing envelope for EEPROM slots.
/// Provides integrity validation, versioning, and length check.
/// [0..2] magic: u16 LE (0x4658 = "FX")
/// [2]    version: u8 (1 = SCHEMA_VERSION)
/// [3]    flags: u8 (0 = Valid active slot, 0xFF = Deleted)
/// [4..6] len: u16 LE (exact payload byte length)
/// [6..8] crc16: u16 LE (CRC-16 over payload)
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct SlotHeader {
    pub magic: u16,
    pub version: u8,
    pub flags: u8,
    pub len: u16,
    pub crc16: u16,
}

impl SlotHeader {
    pub fn parse(buf: &[u8; HEADER_SIZE]) -> Self {
        Self {
            magic: u16::from_le_bytes([buf[0], buf[1]]),
            version: buf[2],
            flags: buf[3],
            len: u16::from_le_bytes([buf[4], buf[5]]),
            crc16: u16::from_le_bytes([buf[6], buf[7]]),
        }
    }

    pub fn to_bytes(&self) -> [u8; HEADER_SIZE] {
        let mut out = [0u8; HEADER_SIZE];
        out[0..2].copy_from_slice(&self.magic.to_le_bytes());
        out[2] = self.version;
        out[3] = self.flags;
        out[4..6].copy_from_slice(&self.len.to_le_bytes());
        out[6..8].copy_from_slice(&self.crc16.to_le_bytes());
        out
    }

    pub fn is_valid(&self, expected_len: usize) -> Result<(), StorageError> {
        if self.magic != SLOT_MAGIC {
            return Err(StorageError::EmptySlot);
        }
        if self.version != SCHEMA_VERSION {
            return Err(StorageError::VersionMismatch(self.version));
        }
        if self.flags != 0 {
            return Err(StorageError::EmptySlot);
        }
        if self.len as usize != expected_len {
            return Err(StorageError::LengthMismatch(self.len));
        }
        Ok(())
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct EepromStatus {
    pub bus: u8,       // 0: None, 1: I2C1 (PB8/PB9), 2: I2C2 (PB10/PB11)
    pub addr: u8,      // 7-bit slave address, e.g. 0x50
    pub detected: bool,
    pub read_ok: bool,
    pub write_ok: bool,
    pub write_err: bool,
}

pub static mut EEPROM_STATUS: EepromStatus = EepromStatus {
    bus: 0,
    addr: 0x50,
    detected: false,
    read_ok: false,
    write_ok: false,
    write_err: false,
};

pub fn get_eeprom_status() -> EepromStatus {
    unsafe { EEPROM_STATUS }
}

/// Direct hardware I2C EEPROM driver with adaptive bus & address auto-detection.
/// Primary: I2C2 on PB10 (SCL) and PB11 (SDA)
/// Fallback: I2C1 on PB8 (SCL) and PB9 (SDA)
#[cfg(not(test))]
pub struct EepromI2c;

#[cfg(not(test))]
#[inline(always)]
fn clear_i2c_flags(i2c: &pac::i2c1::RegisterBlock) {
    // Clear NACK, STOP, BERR, ARLO, OVR, PEC, TIMOUT, ALERT flags
    unsafe {
        i2c.icr.write(|w| w.bits(0x3F38));
    }
}

/// Reset and reinitialize an I2C peripheral (I2C1 or I2C2) following bus faults.
#[cfg(not(test))]
fn recover_i2c(bus_num: u8) {
    let rcc = unsafe { &*pac::RCC::ptr() };
    unsafe {
        if bus_num == 1 {
            let i2c1 = &*pac::I2C1::ptr();
            // 1. Send STOP request to release lines if busy
            i2c1.cr2.modify(|r, w| w.bits(r.bits() | (1 << 14)));
            let mut timeout = 2_000u32;
            while !i2c1.isr.read().stopf().bit() && timeout > 0 {
                timeout -= 1;
            }

            // 2. Disable peripheral and clear flags
            i2c1.cr1.modify(|_, w| w.pe().clear_bit());
            clear_i2c_flags(&*pac::I2C1::ptr());

            // 3. Reset peripheral via APB1 peripheral reset (bit 21 = I2C1)
            rcc.apb1rstr.modify(|r, w| w.bits(r.bits() | (1 << 21)));
            rcc.apb1rstr.modify(|r, w| w.bits(r.bits() & !(1 << 21)));

            // 4. Restore timing (SYSCLK 48 MHz / 100 kHz standard / 400 kHz fast) and re-enable
            i2c1.cr1.modify(|_, w| w.pe().clear_bit());
            i2c1.timingr.write(|w| w.bits(0x00B0_1A4B));
            clear_i2c_flags(&*pac::I2C1::ptr());
            i2c1.cr1.modify(|_, w| w.pe().set_bit());
        } else {
            let i2c2 = &*pac::I2C2::ptr();
            let i2c_block = &*pac::I2C2::ptr();

            // 1. Send STOP request to release lines if busy
            i2c2.cr2.modify(|r, w| w.bits(r.bits() | (1 << 14)));
            let mut timeout = 2_000u32;
            while !i2c2.isr.read().stopf().bit() && timeout > 0 {
                timeout -= 1;
            }

            // 2. Disable peripheral and clear flags
            i2c2.cr1.modify(|_, w| w.pe().disabled());
            clear_i2c_flags(i2c_block);

            // 3. Reset peripheral via APB1 peripheral reset (bit 22 = I2C2)
            rcc.apb1rstr.modify(|r, w| w.bits(r.bits() | (1 << 22)));
            rcc.apb1rstr.modify(|r, w| w.bits(r.bits() & !(1 << 22)));

            // 4. Restore timing (48 MHz APB1/SYSCLK -> 1 MHz Fast Mode Plus) and re-enable
            i2c2.cr1.modify(|_, w| w.pe().disabled());
            i2c2.timingr.write(|w| {
                w.presc().bits(0)
                    .scldel().bits(3)
                    .sdadel().bits(0)
                    .sclh().bits(9)
                    .scll().bits(25)
            });
            clear_i2c_flags(i2c_block);
            i2c2.cr1.modify(|_, w| w.pe().enabled());
        }
    }
}

#[cfg(not(test))]
unsafe fn probe_device(i2c: &pac::i2c1::RegisterBlock, bus_num: u8, addr_7bit: u8) -> bool {
    // Clear flags
    clear_i2c_flags(i2c);

    // Try a 1-byte read transaction: SADD = (addr << 1), NBYTES = 1, RD_WRN = 1 (read), START = 1, AUTOEND = 1
    i2c.cr2.write(|w| {
        w.sadd().bits((addr_7bit as u16) << 1)
            .nbytes().bits(1)
            .rd_wrn().read()
            .autoend().automatic()
            .start().start()
    });

    let mut timeout = 20_000u32;
    while timeout > 0 {
        let isr = i2c.isr.read();
        if isr.nackf().bit_is_set() || isr.berr().bit_is_set() || isr.arlo().bit_is_set() || isr.ovr().bit_is_set() {
            clear_i2c_flags(i2c);
            if isr.berr().bit_is_set() || isr.arlo().bit_is_set() || isr.ovr().bit_is_set() {
                recover_i2c(bus_num);
            }
            return false;
        }
        if isr.rxne().bit_is_set() || isr.stopf().bit_is_set() {
            let _dummy = i2c.rxdr.read().rxdata().bits();
            let mut stop_wait = 10_000u32;
            while i2c.isr.read().stopf().bit_is_clear() && stop_wait > 0 {
                let isr_stop = i2c.isr.read();
                if isr_stop.berr().bit_is_set() || isr_stop.arlo().bit_is_set() || isr_stop.ovr().bit_is_set() {
                    recover_i2c(bus_num);
                    return false;
                }
                stop_wait -= 1;
            }
            clear_i2c_flags(i2c);
            return true;
        }
        timeout -= 1;
    }

    recover_i2c(bus_num);
    false
}

#[cfg(not(test))]
impl EepromI2c {
    /// Initialize I2C2 on PB10 (SCL) and PB11 (SDA) and adaptively detect EEPROM address.
    pub fn init() {
        let rcc = unsafe { &*pac::RCC::ptr() };
        let gpiob = unsafe { &*pac::GPIOB::ptr() };
        let i2c2 = unsafe { &*pac::I2C2::ptr() };
        let syscfg = unsafe { &*pac::SYSCFG::ptr() };

        // 1. Enable GPIOB, SYSCFG, and I2C2 clocks
        rcc.ahbenr.modify(|_, w| w.iopben().enabled());
        rcc.apb2enr.modify(|_, w| w.syscfgen().enabled());
        rcc.apb1enr.modify(|_, w| w.i2c2en().enabled());

        // 2. Enable Fast Mode Plus (Fm+ 20 mA high-drive sink) on I2C2 pins in SYSCFG
        syscfg.cfgr1.modify(|_, w| w.i2c2_fmp().fmp());

        // 3. Configure PB10 and PB11 as Alternate Function 1 (I2C2), Open-Drain, High-Speed, Pull-up
        gpiob.afrh.modify(|_, w| w.afrh10().af1().afrh11().af1());
        gpiob.moder.modify(|_, w| w.moder10().alternate().moder11().alternate());
        gpiob.otyper.modify(|_, w| w.ot10().open_drain().ot11().open_drain());
        gpiob.ospeedr.modify(|_, w| w.ospeedr10().high_speed().ospeedr11().high_speed());
        gpiob.pupdr.modify(|_, w| w.pupdr10().pull_up().pupdr11().pull_up());

        // 4. Reset I2C2 peripheral via APB1 peripheral reset
        i2c2.cr1.modify(|_, w| w.pe().disabled());
        rcc.apb1rstr.modify(|r, w| unsafe { w.bits(r.bits() | (1 << 22)) });
        rcc.apb1rstr.modify(|r, w| unsafe { w.bits(r.bits() & !(1 << 22)) });
        i2c2.cr1.modify(|_, w| w.pe().disabled());

        // Timing for 48 MHz APB1/SYSCLK -> 1 MHz (1000 kHz) Fast Mode Plus (Fm+)
        // Matches proven flysky-i6x-rs timing
        i2c2.timingr.write(|w| {
            w.presc().bits(0)
                .scldel().bits(3)
                .sdadel().bits(0)
                .sclh().bits(9)
                .scll().bits(25)
        });

        // 5. Clear flags and enable I2C2 peripheral
        clear_i2c_flags(unsafe { &*pac::I2C2::ptr() });
        i2c2.cr1.modify(|_, w| w.pe().enabled());

        // 6. Adaptive Probe:
        // First check I2C2 (PB10/PB11) across addresses 0x50..=0x57
        unsafe {
            EEPROM_STATUS.bus = 0;
            EEPROM_STATUS.detected = false;

            for addr in 0x50..=0x57 {
                if probe_device(&*pac::I2C2::ptr(), 2, addr) {
                    EEPROM_STATUS.bus = 2; // I2C2
                    EEPROM_STATUS.addr = addr;
                    EEPROM_STATUS.detected = true;
                    break;
                }
            }

            // Fallback: If not found on I2C2, check I2C1 (PB8/PB9) across addresses 0x50..=0x57
            if !EEPROM_STATUS.detected {
                for addr in 0x50..=0x57 {
                    if probe_device(&*pac::I2C1::ptr(), 1, addr) {
                        EEPROM_STATUS.bus = 1; // I2C1
                        EEPROM_STATUS.addr = addr;
                        EEPROM_STATUS.detected = true;
                        break;
                    }
                }
            }
        }
    }

    /// Read sequential bytes starting at 16-bit EEPROM address.
    pub fn read(addr: u16, data: &mut [u8]) -> Result<(), StorageError> {
        if data.is_empty() {
            return Ok(());
        }

        let status = unsafe { EEPROM_STATUS };
        let bus_num = if status.bus == 1 { 1 } else { 2 };
        let dev_addr = if status.detected { status.addr } else { 0x50 };
        let i2c = unsafe {
            if bus_num == 1 {
                &*pac::I2C1::ptr()
            } else {
                &*pac::I2C2::ptr()
            }
        };

        // 1. Send 2-byte memory address write header without STOP (autoend=false)
        let addr_bytes = addr.to_be_bytes();
        i2c.cr2.write(|w| {
            w.sadd().bits((dev_addr as u16) << 1)
                .nbytes().bits(2)
                .rd_wrn().write()
                .autoend().software()
                .start().start()
        });

        for &byte in &addr_bytes {
            let mut timeout = 100_000u32;
            while i2c.isr.read().txis().bit_is_clear() {
                let isr = i2c.isr.read();
                if isr.nackf().bit_is_set() || isr.berr().bit_is_set() || isr.arlo().bit_is_set() || isr.ovr().bit_is_set() {
                    clear_i2c_flags(i2c);
                    recover_i2c(bus_num);
                    return Err(StorageError::I2cError);
                }
                timeout -= 1;
                if timeout == 0 {
                    recover_i2c(bus_num);
                    return Err(StorageError::I2cError);
                }
            }
            i2c.txdr.write(|w| w.txdata().bits(byte));
        }

        // Wait for TC (Transfer Complete)
        let mut timeout = 100_000u32;
        while i2c.isr.read().tc().bit_is_clear() {
            let isr = i2c.isr.read();
            if isr.nackf().bit_is_set() || isr.berr().bit_is_set() || isr.arlo().bit_is_set() || isr.ovr().bit_is_set() {
                clear_i2c_flags(i2c);
                recover_i2c(bus_num);
                return Err(StorageError::I2cError);
            }
            timeout -= 1;
            if timeout == 0 {
                recover_i2c(bus_num);
                return Err(StorageError::I2cError);
            }
        }

        // 2. Read sequential data bytes in chunks of up to 255 bytes (NBYTES limitation)
        let mut read_offset = 0;
        while read_offset < data.len() {
            let chunk_len = core::cmp::min(255, data.len() - read_offset);
            let is_last = (read_offset + chunk_len) == data.len();

            i2c.cr2.write(|w| {
                w.sadd().bits((dev_addr as u16) << 1)
                    .nbytes().bits(chunk_len as u8)
                    .rd_wrn().read()
                    .autoend().bit(is_last)
                    .start().start()
            });

            for i in 0..chunk_len {
                let mut byte_timeout = 100_000u32;
                while i2c.isr.read().rxne().bit_is_clear() {
                    let isr = i2c.isr.read();
                    if isr.nackf().bit_is_set() || isr.berr().bit_is_set() || isr.arlo().bit_is_set() || isr.ovr().bit_is_set() {
                        clear_i2c_flags(i2c);
                        recover_i2c(bus_num);
                        return Err(StorageError::I2cError);
                    }
                    byte_timeout -= 1;
                    if byte_timeout == 0 {
                        recover_i2c(bus_num);
                        return Err(StorageError::I2cError);
                    }
                }
                data[read_offset + i] = i2c.rxdr.read().rxdata().bits();
            }

            if !is_last {
                let mut tc_timeout = 100_000u32;
                while i2c.isr.read().tc().bit_is_clear() {
                    let isr = i2c.isr.read();
                    if isr.nackf().bit_is_set() || isr.berr().bit_is_set() || isr.arlo().bit_is_set() || isr.ovr().bit_is_set() {
                        clear_i2c_flags(i2c);
                        recover_i2c(bus_num);
                        return Err(StorageError::I2cError);
                    }
                    tc_timeout -= 1;
                    if tc_timeout == 0 {
                        recover_i2c(bus_num);
                        return Err(StorageError::I2cError);
                    }
                }
            } else {
                let mut stop_timeout = 100_000u32;
                while i2c.isr.read().stopf().bit_is_clear() {
                    let isr = i2c.isr.read();
                    if isr.berr().bit_is_set() || isr.arlo().bit_is_set() || isr.ovr().bit_is_set() {
                        clear_i2c_flags(i2c);
                        recover_i2c(bus_num);
                        return Err(StorageError::I2cError);
                    }
                    stop_timeout -= 1;
                    if stop_timeout == 0 {
                        recover_i2c(bus_num);
                        return Err(StorageError::I2cError);
                    }
                }
                i2c.icr.write(|w| w.stopcf().clear());
            }

            read_offset += chunk_len;
        }

        Ok(())
    }

    /// Write a single page chunk (up to 64 bytes) that does NOT cross a 64-byte boundary.
    fn write_page_aligned(addr: u16, chunk: &[u8]) -> Result<(), StorageError> {
        if chunk.is_empty() {
            return Ok(());
        }

        let status = unsafe { EEPROM_STATUS };
        let bus_num = if status.bus == 1 { 1 } else { 2 };
        let dev_addr = if status.detected { status.addr } else { 0x50 };
        let i2c = unsafe {
            if bus_num == 1 {
                &*pac::I2C1::ptr()
            } else {
                &*pac::I2C2::ptr()
            }
        };

        let total_bytes = (2 + chunk.len()) as u8;
        let addr_bytes = addr.to_be_bytes();

        i2c.cr2.write(|w| {
            w.sadd().bits((dev_addr as u16) << 1)
                .nbytes().bits(total_bytes)
                .rd_wrn().write()
                .autoend().automatic()
                .start().start()
        });

        // 1. Send 2 address bytes
        for &byte in &addr_bytes {
            let mut timeout = 100_000u32;
            while i2c.isr.read().txis().bit_is_clear() {
                let isr = i2c.isr.read();
                if isr.nackf().bit_is_set() || isr.berr().bit_is_set() || isr.arlo().bit_is_set() || isr.ovr().bit_is_set() {
                    clear_i2c_flags(i2c);
                    recover_i2c(bus_num);
                    return Err(StorageError::I2cError);
                }
                timeout -= 1;
                if timeout == 0 {
                    recover_i2c(bus_num);
                    return Err(StorageError::I2cError);
                }
            }
            i2c.txdr.write(|w| w.txdata().bits(byte));
        }

        // 2. Send payload bytes
        for &byte in chunk {
            let mut timeout = 100_000u32;
            while i2c.isr.read().txis().bit_is_clear() {
                let isr = i2c.isr.read();
                if isr.nackf().bit_is_set() || isr.berr().bit_is_set() || isr.arlo().bit_is_set() || isr.ovr().bit_is_set() {
                    clear_i2c_flags(i2c);
                    recover_i2c(bus_num);
                    return Err(StorageError::I2cError);
                }
                timeout -= 1;
                if timeout == 0 {
                    recover_i2c(bus_num);
                    return Err(StorageError::I2cError);
                }
            }
            i2c.txdr.write(|w| w.txdata().bits(byte));
        }

        // Wait for STOPF
        let mut stop_timeout = 100_000u32;
        while i2c.isr.read().stopf().bit_is_clear() {
            let isr = i2c.isr.read();
            if isr.berr().bit_is_set() || isr.arlo().bit_is_set() || isr.ovr().bit_is_set() {
                clear_i2c_flags(i2c);
                recover_i2c(bus_num);
                return Err(StorageError::I2cError);
            }
            stop_timeout -= 1;
            if stop_timeout == 0 {
                recover_i2c(bus_num);
                return Err(StorageError::I2cError);
            }
        }
        i2c.icr.write(|w| w.stopcf().clear());

        // 3. ACK Polling for EEPROM internal write cycle (max 5ms)
        Self::poll_ack(i2c, bus_num, dev_addr)?;

        Ok(())
    }

    /// Poll I2C address until EEPROM ACKs that its internal self-timed write cycle has completed.
    fn poll_ack(i2c: &pac::i2c1::RegisterBlock, bus_num: u8, dev_addr: u8) -> Result<(), StorageError> {
        let mut retries = 50u32; // up to ~10ms total polling

        while retries > 0 {
            crate::watchdog::feed();
            retries -= 1;

            // Send 0-byte write to test device readiness
            i2c.cr2.write(|w| {
                w.sadd().bits((dev_addr as u16) << 1)
                    .nbytes().bits(0)
                    .rd_wrn().write()
                    .autoend().automatic()
                    .start().start()
            });

            let mut wait = 20_000u32;
            let mut got_nack = false;
            while i2c.isr.read().stopf().bit_is_clear() && wait > 0 {
                let isr = i2c.isr.read();
                if isr.berr().bit_is_set() || isr.arlo().bit_is_set() || isr.ovr().bit_is_set() {
                    clear_i2c_flags(i2c);
                    recover_i2c(bus_num);
                    return Err(StorageError::I2cError);
                }
                if isr.nackf().bit_is_set() {
                    got_nack = true;
                    i2c.icr.write(|w| w.nackcf().clear());
                }
                wait -= 1;
            }
            i2c.icr.write(|w| w.stopcf().clear());

            if !got_nack && wait > 0 {
                return Ok(()); // Device ACKed, write complete!
            }

            {
                let start = crate::time::millis();
                while crate::time::millis().wrapping_sub(start) < 1 {
                    crate::watchdog::feed();
                }
            }
        }

        recover_i2c(bus_num);
        Err(StorageError::I2cError)
    }

    /// Write arbitrary slice with automatic page boundary splitting to prevent wrap-around.
    pub fn write(mut addr: u16, data: &[u8]) -> Result<(), StorageError> {
        let mut written = 0;
        while written < data.len() {
            let page_offset = (addr % (PAGE_SIZE as u16)) as usize;
            let chunk_len = core::cmp::min(PAGE_SIZE - page_offset, data.len() - written);

            let chunk = &data[written..written + chunk_len];
            Self::write_page_aligned(addr, chunk)?;

            addr = addr.saturating_add(chunk_len as u16);
            written += chunk_len;
        }
        Ok(())
    }
}

/// Legacy ModelConfig v1 (128 bytes) from prior releases.
/// Used for transparent, lossless migration to v2 (348 bytes).
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct LegacyModelConfigV1 {
    pub name: [u8; 10],
    pub model_type: u8,
    pub _pad0: u8,
    pub rx_id: u32,
    pub trims: [i8; 4],
    pub channel_reverse: u32,
    pub dr_switch: u8,
    pub thr_curve_pts: u8,
    pub thr_curve_smooth: u8,
    pub thr_curve: [u8; 9],
    pub dr_high: [u8; 3],
    pub dr_low: [u8; 3],
    pub expo_high: [i8; 3],
    pub expo_low: [i8; 3],
    pub timer_secs: u16,
    pub timer_source: u8,
    pub rf_power: u8,
    pub failsafe_thr: u16,
    pub aux_channels: [u8; 14],
    pub wing_tail_mix: u8,
    pub template_diff: i8,
    pub mixes: [LegacyMixLineV1; 8],
    pub failsafe_mode: u8,
    pub failsafe_timeout: u8,
    pub rf_protocol: u8,
    pub crsf_baud: u8,
    pub arm_switch: u8,
    pub rx_out_mode: u8,
    pub servo_rate_hz: u16,
    pub rx_serial_proto: u8,
    pub crsf_half_duplex: u8,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct LegacyMixLineV1 {
    pub target_ch: u8,
    pub source: u8,
    pub weight: i8,
    pub offset: i8,
    pub switch: u8,
    pub mode: u8,
}

const _: () = assert!(core::mem::size_of::<LegacyModelConfigV1>() == 128);
const _: () = assert!(core::mem::size_of::<LegacyMixLineV1>() == 6);

pub fn unpack_legacy_v1_model(v1: &LegacyModelConfigV1) -> ModelConfig {
    let mut v2 = ModelConfig::default_for_index(0);
    v2.name = v1.name;
    v2.model_type = v1.model_type;
    v2.rx_id = v1.rx_id;
    v2.trims = v1.trims;
    v2.channel_reverse = v1.channel_reverse;
    v2.dr_switch = v1.dr_switch;
    v2.thr_curve_pts = v1.thr_curve_pts;
    v2.thr_curve_smooth = v1.thr_curve_smooth;
    // Copy the first 9 curve points, rest are 0
    v2.thr_curve[0..9].copy_from_slice(&v1.thr_curve);
    v2.dr_high = v1.dr_high;
    v2.dr_low = v1.dr_low;
    v2.expo_high = v1.expo_high;
    v2.expo_low = v1.expo_low;
    v2.timer_secs = v1.timer_secs;
    v2.timer_source = v1.timer_source;
    v2.rf_power = v1.rf_power;
    v2.failsafe_thr = v1.failsafe_thr;
    v2.aux_channels = v1.aux_channels;
    v2.wing_tail_mix = v1.wing_tail_mix;
    v2.template_diff = v1.template_diff;

    // Migrate first 8 mixer lines
    for i in 0..8 {
        v2.mixes[i] = MixLine {
            target_ch: v1.mixes[i].target_ch,
            source: v1.mixes[i].source,
            weight_src: 0, // default fixed weight
            weight: v1.mixes[i].weight,
            offset: v1.mixes[i].offset,
            switch: v1.mixes[i].switch,
            mode: v1.mixes[i].mode,
            _reserved: 0,
        };
    }
    // Remaining lines 8..32 remain disabled

    v2.failsafe_mode = v1.failsafe_mode;
    v2.failsafe_timeout = v1.failsafe_timeout;
    v2.rf_protocol = v1.rf_protocol;
    v2.crsf_baud = v1.crsf_baud;
    v2.arm_switch = v1.arm_switch;
    v2.rx_out_mode = v1.rx_out_mode;
    v2.servo_rate_hz = v1.servo_rate_hz;
    v2.rx_serial_proto = v1.rx_serial_proto;
    v2.crsf_half_duplex = v1.crsf_half_duplex;

    v2
}

/// Helper to read a raw C struct from an EEPROM slot with 8-byte framing and CRC16 verification.
pub fn load_slot<T: Copy>(addr: u16, dst: &mut T) -> Result<(), StorageError> {
    #[cfg(test)]
    {
        let _ = addr;
        let _ = dst;
        return Err(StorageError::EmptySlot);
    }

    #[cfg(not(test))]
    {
        // 1. Read 8-byte slot header
        let mut hdr_bytes = [0u8; HEADER_SIZE];
        EepromI2c::read(addr, &mut hdr_bytes)?;
        let hdr = SlotHeader::parse(&hdr_bytes);

        let expected_size = core::mem::size_of::<T>();

        // Check if this is a ModelConfig request and the slot holds legacy v1 (128 bytes)
        if expected_size == core::mem::size_of::<ModelConfig>()
            && hdr.len == 128
            && hdr.magic == SLOT_MAGIC
            && hdr.flags == 0
        {
            // Read 128 bytes of legacy model
            let mut v1 = core::mem::MaybeUninit::<LegacyModelConfigV1>::uninit();
            let v1_bytes = unsafe {
                core::slice::from_raw_parts_mut(v1.as_mut_ptr() as *mut u8, 128)
            };
            EepromI2c::read(addr + HEADER_SIZE as u16, v1_bytes)?;

            // Verify CRC-16 of legacy payload
            let computed_crc = crc16(v1_bytes);
            if computed_crc != hdr.crc16 {
                return Err(StorageError::CrcMismatch);
            }

            let legacy_model = unsafe { v1.assume_init() };
            let v2_model = unpack_legacy_v1_model(&legacy_model);
            unsafe {
                core::ptr::copy_nonoverlapping(
                    &v2_model as *const ModelConfig as *const T,
                    dst as *mut T,
                    1,
                );
            }

            // In-place upgrade: re-save slot as v2 (348 bytes) with updated CRC-16 envelope
            let _ = save_slot(addr, &v2_model);

            return Ok(());
        }

        hdr.is_valid(expected_size)?;

        // 2. Read payload directly into destination struct
        let dst_bytes = unsafe {
            core::slice::from_raw_parts_mut(dst as *mut T as *mut u8, expected_size)
        };
        EepromI2c::read(addr + HEADER_SIZE as u16, dst_bytes)?;

        // 3. Verify CRC-16
        let computed_crc = crc16(dst_bytes);
        if computed_crc != hdr.crc16 {
            return Err(StorageError::CrcMismatch);
        }

        Ok(())
    }
}

/// Helper to write a raw C struct into an EEPROM slot with 8-byte framing and CRC16.
pub fn save_slot<T: Copy>(addr: u16, src: &T) -> Result<(), StorageError> {
    #[cfg(test)]
    {
        let _ = addr;
        let _ = src;
        return Ok(());
    }

    #[cfg(not(test))]
    {
        let size = core::mem::size_of::<T>();
        let src_bytes = unsafe {
            core::slice::from_raw_parts(src as *const T as *const u8, size)
        };

        let crc = crc16(src_bytes);
        let hdr = SlotHeader {
            magic: SLOT_MAGIC,
            version: SCHEMA_VERSION,
            flags: 0,
            len: size as u16,
            crc16: crc,
        };

        // Write header first
        let hdr_bytes = hdr.to_bytes();
        let res = (|| -> Result<(), StorageError> {
            EepromI2c::write(addr, &hdr_bytes)?;
            EepromI2c::write(addr + HEADER_SIZE as u16, src_bytes)?;
            Ok(())
        })();

        unsafe {
            if res.is_ok() {
                EEPROM_STATUS.write_ok = true;
                EEPROM_STATUS.write_err = false;
            } else {
                EEPROM_STATUS.write_err = true;
            }
        }

        res
    }
}

/// Read only model name and magic without transferring the entire ModelConfig.
pub fn peek_model_name(slot_idx: usize) -> Option<[u8; 10]> {
    #[cfg(test)]
    {
        let _ = slot_idx;
        None
    }
    #[cfg(not(test))]
    {
        let addr = model_slot_addr(slot_idx)?;
        // Read 8-byte header + first 10 bytes (ModelConfig::name) = 18 bytes
        let mut buf = [0u8; HEADER_SIZE + 10];
        if EepromI2c::read(addr, &mut buf).is_err() {
            return None;
        }

        let hdr = SlotHeader::parse(unsafe {
            &*(buf[0..HEADER_SIZE].as_ptr() as *const [u8; HEADER_SIZE])
        });
        if hdr.magic != SLOT_MAGIC || hdr.flags != 0 {
            return None;
        }
        if hdr.len != (core::mem::size_of::<ModelConfig>() as u16) && hdr.len != 128 {
            return None;
        }

        let mut name = [0u8; 10];
        name.copy_from_slice(&buf[HEADER_SIZE..HEADER_SIZE + 10]);
        Some(name)
    }
}

/// Delete a model slot in O(1) by invalidating the magic bytes in the 8-byte header.
pub fn delete_model_slot(slot_idx: usize) -> bool {
    #[cfg(test)]
    {
        let _ = slot_idx;
        true
    }
    #[cfg(not(test))]
    {
        let Some(addr) = model_slot_addr(slot_idx) else {
            return false;
        };
        // Overwrite magic with 0xFFFF to invalidate slot
        let invalid_magic = [0xFFu8, 0xFFu8];
        EepromI2c::write(addr, &invalid_magic).is_ok()
    }
}

/// Initialize EEPROM peripheral and load radio storage into memory.
#[inline(never)]
pub fn load_storage_into(storage: &mut RadioStorage) {
    #[cfg(test)]
    {
        storage.init_default();
        return;
    }

    #[cfg(not(test))]
    {
        EepromI2c::init();

        // 1. Try loading RadioConfig from Primary Slot A
        let mut radio_loaded = false;
        if load_slot(RADIO_SLOT_A_ADDR, &mut storage.radio).is_ok() {
            radio_loaded = true;
            unsafe {
                EEPROM_STATUS.read_ok = true;
            }
        } else if load_slot(RADIO_SLOT_B_ADDR, &mut storage.radio).is_ok() {
            // Primary Slot A failed, but Backup Slot B succeeded!
            radio_loaded = true;
            unsafe {
                EEPROM_STATUS.read_ok = true;
            }
            // Restore Slot A from Slot B
            let _ = save_slot(RADIO_SLOT_A_ADDR, &storage.radio);
        }

        if !radio_loaded {
            // Fresh / uninitialized EEPROM -> load factory defaults and save to Slot A and B
            storage.init_default();
            let _ = save_slot(RADIO_SLOT_A_ADDR, &storage.radio);
            let _ = save_slot(RADIO_SLOT_B_ADDR, &storage.radio);
            if let Some(addr) = model_slot_addr(0) {
                let _ = save_slot(addr, &storage.active_model);
            }
            storage.sanitize();
            return;
        }

        // 2. Load only the active model into the active-model SRAM cache
        let active_idx = (storage.radio.active_model as usize).min(NUM_MODELS - 1);
        if let Some(addr) = model_slot_addr(active_idx) {
            if load_slot(addr, &mut storage.active_model).is_err() {
                storage.active_model = ModelConfig::default_for_index(active_idx);
            }
        } else {
            storage.active_model = ModelConfig::default_for_index(0);
        }

        storage.sanitize();
    }
}

/// Load a specific model slot into .
pub fn load_active_model(storage: &mut RadioStorage) -> bool {
    let active_idx = (storage.radio.active_model as usize).min(NUM_MODELS - 1);
    if let Some(addr) = model_slot_addr(active_idx) {
        if load_slot(addr, &mut storage.active_model).is_ok() {
            storage.sanitize();
            return true;
        }
    }
    storage.active_model = ModelConfig::default_for_index(active_idx);
    storage.sanitize();
    false
}

/// Save arbitrary ModelConfig to an arbitrary model slot index.
pub fn save_model_to_slot(slot_idx: usize, model: &ModelConfig) -> bool {
    if let Some(addr) = model_slot_addr(slot_idx) {
        save_slot(addr, model).is_ok()
    } else {
        false
    }
}

#[allow(dead_code)]
pub fn load_storage() -> RadioStorage {
    let mut storage = RadioStorage::empty();
    load_storage_into(&mut storage);
    storage
}

pub fn load_config() -> RadioConfig {
    let storage = get_storage();
    storage.radio
}

/// Save the currently active model to its dedicated EEPROM slot.
pub fn save_active_model(storage: &RadioStorage) -> bool {
    #[cfg(test)]
    {
        let _ = storage;
        return true;
    }
    #[cfg(not(test))]
    {
        let idx = (storage.radio.active_model as usize).min(NUM_MODELS - 1);
        save_model_to_slot(idx, &storage.active_model)
    }
}

/// Save RadioConfig using A/B ping-pong save to ensure power-loss safety.
pub fn save_radio_config(storage: &RadioStorage) -> bool {
    #[cfg(test)]
    {
        let _ = storage;
        return true;
    }
    #[cfg(not(test))]
    {
        // Ping-pong A/B commit:
        // Step 1: Write Backup Slot B first with new CRC16
        let ok_b = save_slot(RADIO_SLOT_B_ADDR, &storage.radio).is_ok();
        // Step 2: Write Primary Slot A with new CRC16
        let ok_a = save_slot(RADIO_SLOT_A_ADDR, &storage.radio).is_ok();
        ok_a && ok_b
    }
}

/// Atomic storage commit: writes active model first, then RadioConfig A/B.
pub fn save_storage(storage: &RadioStorage) -> bool {
    let mut ok = save_radio_config(storage);
    ok = ok && save_active_model(storage);
    ok
}

/// Read persisted receiver ID from active model if previously bound.
pub fn load_saved_rx_id() -> Option<u32> {
    let storage = get_storage();
    let active = storage.active_model();
    if active.rx_id != 0 && active.rx_id != 0xFFFF_FFFF {
        Some(active.rx_id)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adc_input_mode_detent() {
        assert_eq!(AdcInputMode::from_u8(0), AdcInputMode::Default);
        assert_eq!(AdcInputMode::from_u8(1), AdcInputMode::Off);
        assert_eq!(AdcInputMode::from_u8(2), AdcInputMode::TwoPos);
        assert_eq!(AdcInputMode::from_u8(3), AdcInputMode::ThreePos);
        assert_eq!(AdcInputMode::from_u8(4), AdcInputMode::SixPos);
        assert_eq!(AdcInputMode::from_u8(5), AdcInputMode::Pot);
        assert_eq!(AdcInputMode::from_u8(6), AdcInputMode::PotDetent);
        assert_eq!(AdcInputMode::from_u8(7), AdcInputMode::InstantTrim);
        assert!(AdcInputMode::Pot.is_pot());
        assert!(AdcInputMode::PotDetent.is_pot());
        assert!(!AdcInputMode::InstantTrim.is_pot());
        assert!(!AdcInputMode::Off.is_pot());
        assert!(AdcInputMode::Off.is_off());
        assert!(!AdcInputMode::Pot.has_detent());
        assert!(AdcInputMode::PotDetent.has_detent());
    }

    #[test]
    fn test_pot_calib_and_radio_config_defaults() {
        let radio = RadioConfig::default_factory();
        assert_eq!(radio.aux_pots.len(), 11);
        assert_eq!(radio.adc_modes.len(), 11);
        for i in 0..11 {
            assert_eq!(radio.aux_pots[i].min, crate::adc::ADC_MIN);
            assert_eq!(radio.aux_pots[i].center, crate::adc::ADC_CENTER);
            assert_eq!(radio.aux_pots[i].max, crate::adc::ADC_MAX);
            assert_eq!(radio.adc_modes[i], 0);
        }

        let pot = PotCalib::new(100, 2000, 3900);
        assert_eq!(pot.min, 100);
        assert_eq!(pot.center, 2000);
        assert_eq!(pot.max, 3900);

        assert_eq!(radio.gimbal_type, 0);
        assert!(!radio.is_hall_gimbals());
        assert_eq!(radio.stick_invert_mask, 0x03);
        assert!(radio.is_stick_inverted(0)); // RH
        assert!(radio.is_stick_inverted(1)); // RV
        assert!(!radio.is_stick_inverted(2)); // LV
        assert!(!radio.is_stick_inverted(3)); // LH
    }

    #[test]
    fn test_model_config_size_and_rf_power() {
        assert_eq!(core::mem::size_of::<ModelConfig>(), 360);
        assert_eq!(core::mem::size_of::<RadioConfig>(), 148);
        assert_eq!(core::mem::size_of::<RadioStorage>(), 508);

        let default_m = ModelConfig::default_for_index(0);
        assert_eq!(default_m.rf_power, 0);

        let mut storage = RadioStorage::default_factory();
        storage.active_model.rf_power = 99;
        storage.sanitize();
        assert_eq!(storage.active_model.rf_power, 0);
    }

    #[test]
    fn test_legacy_v1_migration() {
        let v1 = LegacyModelConfigV1 {
            name: *b"PLANE 01  ",
            model_type: 1, // Glider
            _pad0: 0,
            rx_id: 0x1234_5678,
            trims: [5, -3, 2, 0],
            channel_reverse: 0x05,
            dr_switch: 2,
            thr_curve_pts: 9,
            thr_curve_smooth: 1,
            thr_curve: [0, 10, 25, 40, 55, 70, 80, 90, 100],
            dr_high: [90, 85, 80],
            dr_low: [60, 55, 50],
            expo_high: [20, -10, 0],
            expo_low: [10, -5, 0],
            timer_secs: 420,
            timer_source: 2,
            rf_power: 1,
            failsafe_thr: 980,
            aux_channels: [7, 8, 5, 6, 9, 10, 11, 12, 0, 0, 0, 0, 0, 0],
            wing_tail_mix: 2,
            template_diff: 15,
            mixes: [LegacyMixLineV1 {
                target_ch: 1,
                source: 2,
                weight: 50,
                offset: -10,
                switch: 3,
                mode: 1,
            }; 8],
            failsafe_mode: 1,
            failsafe_timeout: 30,
            rf_protocol: 1,
            crsf_baud: 3,
            arm_switch: 4,
            rx_out_mode: 1,
            servo_rate_hz: 200,
            rx_serial_proto: 1,
            crsf_half_duplex: 1,
        };
        let v2 = unpack_legacy_v1_model(&v1);
        assert_eq!(&v2.name, b"PLANE 01  ");
        assert_eq!(v2.model_type, 1);
        assert_eq!(v2.rx_id, 0x1234_5678);
        assert_eq!(v2.trims, [5, -3, 2, 0]);
        assert_eq!(v2.channel_reverse, 0x05);
        assert_eq!(v2.dr_switch, 2);
        assert_eq!(v2.thr_curve_pts, 9);
        assert_eq!(v2.thr_curve_smooth, 1);
        assert_eq!(&v2.thr_curve[0..9], &[0, 10, 25, 40, 55, 70, 80, 90, 100]);
        assert_eq!(&v2.thr_curve[9..17], &[0; 8]);
        assert_eq!(v2.timer_secs, 420);
        assert_eq!(v2.template_diff, 15);
        assert_eq!(v2.mixes.len(), 32);
        for i in 0..8 {
            assert_eq!(v2.mixes[i].target_ch, 1);
            assert_eq!(v2.mixes[i].source, 2);
            assert_eq!(v2.mixes[i].weight_src, 0);
            assert_eq!(v2.mixes[i].weight, 50);
            assert_eq!(v2.mixes[i].offset, -10);
            assert_eq!(v2.mixes[i].switch, 3);
            assert_eq!(v2.mixes[i].mode, 1);
        }
        for i in 8..32 {
            assert_eq!(v2.mixes[i], MixLine::disabled());
        }
        assert_eq!(v2.gvars, [0; 5]);
    }

    #[test]
    fn test_telem_slots_sanitize() {
        let mut storage = RadioStorage::default_factory();
        storage.radio.telem_slots = [0, 11, 12, 13];
        storage.sanitize();
        assert_eq!(storage.radio.telem_slots, [0, 11, 12, 12]);
    }

    #[test]
    fn test_trainer_config_helpers_and_sanitize() {
        let mut radio = RadioConfig::default_factory();
        assert_eq!(radio.trainer_config, 0);
        assert!(!radio.is_ppm_out_enabled());
        assert!(!radio.is_ppm_in_enabled());

        radio.set_ppm_out_enabled(true);
        assert!(radio.is_ppm_out_enabled());
        assert!(!radio.is_ppm_in_enabled());
        assert_eq!(radio.trainer_config, 1);

        radio.set_ppm_in_enabled(true);
        assert!(radio.is_ppm_out_enabled());
        assert!(radio.is_ppm_in_enabled());
        assert_eq!(radio.trainer_config, 3);

        radio.set_ppm_out_enabled(false);
        assert!(!radio.is_ppm_out_enabled());
        assert!(radio.is_ppm_in_enabled());
        assert_eq!(radio.trainer_config, 2);

        let mut storage = RadioStorage::default_factory();
        storage.radio.trainer_config = 99;
        storage.sanitize();
        assert_eq!(storage.radio.trainer_config, 0);
    }

    #[test]
    fn test_crc16_and_slot_header() {
        let test_payload = [0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0];
        let crc = crc16(&test_payload);
        assert_ne!(crc, 0);

        let hdr = SlotHeader {
            magic: SLOT_MAGIC,
            version: SCHEMA_VERSION,
            flags: 0,
            len: test_payload.len() as u16,
            crc16: crc,
        };
        let bytes = hdr.to_bytes();
        let parsed = SlotHeader::parse(&bytes);
        assert_eq!(parsed, hdr);
        assert!(parsed.is_valid(test_payload.len()).is_ok());

        // Corrupted length check
        assert_eq!(
            parsed.is_valid(test_payload.len() + 1),
            Err(StorageError::LengthMismatch(test_payload.len() as u16))
        );

        // Invalid magic check
        let mut corrupted_bytes = bytes;
        corrupted_bytes[0] = 0xFF;
        let corrupted_hdr = SlotHeader::parse(&corrupted_bytes);
        assert_eq!(corrupted_hdr.is_valid(test_payload.len()), Err(StorageError::EmptySlot));
    }

    #[test]
    fn test_eeprom_slot_addressing_and_bounds() {
        assert_eq!(RADIO_SLOT_A_ADDR, 0x0000);
        assert_eq!(RADIO_SLOT_B_ADDR, 0x0100);
        assert_eq!(MODEL_BASE_ADDR, 0x0200);

        // Verify model slot addresses are strictly 64-byte page-aligned
        for i in 0..NUM_MODELS {
            let addr = model_slot_addr(i).expect("Valid model slot");
            assert_eq!(addr % (PAGE_SIZE as u16), 0);
            assert!(addr >= MODEL_BASE_ADDR);
            assert!((addr as usize + MODEL_SLOT_SIZE) <= EEPROM_TOTAL_SIZE);
        }
        assert_eq!(model_slot_addr(NUM_MODELS), None);
    }
    #[test]
    fn test_dynamic_mixer_source_and_switch_availability() {
        let mut storage = RadioStorage::default_factory();

        // Stock config: ext_switches=0, ext_adc=0, j4_sg_sh_en=0
        // Sticks (1..4), MAX (11), CH1..CH4 (12..15), Thr+ (30) always available
        for s in 0..=4 {
            assert!(storage.is_source_available(s));
        }
        assert!(storage.is_source_available(11));
        assert!(storage.is_source_available(30));

        // Pots VRA & VRB are available by default
        assert!(storage.is_source_available(5));
        assert!(storage.is_source_available(6));

        // Switches SA..SD are available by default
        assert!(storage.is_source_available(7)); // SA
        assert!(storage.is_source_available(8)); // SB
        assert!(storage.is_source_available(9)); // SC
        assert!(storage.is_source_available(10)); // SD

        // Extended switches SE..SH and SWI..SWM / VRC..VRG unavailable by default
        assert!(!storage.is_source_available(31)); // SE
        assert!(!storage.is_source_available(32)); // SF
        assert!(!storage.is_source_available(33)); // SG
        assert!(!storage.is_source_available(34)); // SH
        assert!(!storage.is_source_available(35)); // SWI
        assert!(!storage.is_source_available(36)); // SWJ
        assert!(!storage.is_source_available(37)); // VRC
        assert!(!storage.is_source_available(38)); // VRD
        assert!(!storage.is_source_available(39)); // VRE
        assert!(!storage.is_source_available(40)); // VRF
        assert!(!storage.is_source_available(41)); // VRG
        assert!(!storage.is_source_available(42)); // SWK
        assert!(!storage.is_source_available(43)); // SWL
        assert!(!storage.is_source_available(44)); // SWM

        // Enable ext_adc
        storage.radio.ext_adc = 1;
        assert!(storage.is_source_available(37)); // VRC
        assert!(storage.is_source_available(38)); // VRD
        assert!(storage.is_source_available(39)); // VRE
        assert!(storage.is_source_available(40)); // VRF
        assert!(storage.is_source_available(41)); // VRG
        storage.radio.set_adc_mode_val(10, 2); // 2-pos switch on PC5 (2 = TwoPos)
        assert!(!storage.is_source_available(41)); // VRG is now switch, not pot
        assert!(storage.is_source_available(44));  // SWM
        assert!(storage.is_switch_cond_available(27)); // SWM^
        assert!(storage.is_switch_cond_available(28)); // SWMv

        // Turn PC5 to OFF (1 = Off)
        storage.radio.set_adc_mode_val(10, 1);
        assert!(!storage.is_source_available(41)); // VRG not available when Off
        assert!(!storage.is_source_available(44)); // SWM not available when Off
        assert!(!storage.is_switch_cond_available(27)); // SWM^ not available when Off
        assert!(!storage.is_switch_cond_available(28)); // SWMv not available when Off
        assert!(!storage.is_aux_source_available(41));  // VRG not available in AUX CHANNELS
        assert!(!storage.is_aux_source_available(44));  // SWM not available in AUX CHANNELS

        // Enable ext_switches
        storage.radio.ext_switches = 1;
        assert!(storage.is_source_available(31)); // SE
        assert!(storage.is_source_available(32)); // SF
        assert!(storage.is_switch_cond_available(11)); // SE^
        assert!(storage.is_switch_cond_available(14)); // SFv

        // Enable SG/SH
        storage.radio.j4_sg_sh_en = 1;
        assert!(storage.is_source_available(33)); // SG
        assert!(storage.is_source_available(34)); // SH
        assert!(storage.is_switch_cond_available(15)); // SG^
        assert!(storage.is_switch_cond_available(18)); // SHv

        // Channels: default model has aux_channels [7, 8, 5, 6, 9, 10, 0, 0, 0, 0, 0, 0, 0, 0]
        // CH5..CH10 are configured (sources 16..21)
        for ch in 1..=10 {
            assert!(storage.is_channel_configured(ch));
            assert!(storage.is_source_available(ch + 11));
            assert!(storage.is_weight_src_available(ch)); // Bipolar
            assert!(storage.is_weight_src_available(ch + 18)); // Unipolar
        }
        // CH11..CH18 are unmapped (source=0) and no mix line targets them
        for ch in 11..=18 {
            assert!(!storage.is_channel_configured(ch));
            assert!(!storage.is_source_available(ch + 11));
            assert!(!storage.is_weight_src_available(ch));
            assert!(!storage.is_weight_src_available(ch + 18));
        }

        // Target CH11 with a mix line -> CH11 becomes configured
        storage.active_model.mixes[0].target_ch = 11;
        assert!(storage.is_channel_configured(11));
        assert!(storage.is_source_available(22)); // CH11 source
        assert!(storage.is_weight_src_available(11));

        // Global Variables GV1..GV5 (sources 45..49 and weight sources 37..41) are always available
        for gv in 45..=49 {
            assert!(storage.is_source_available(gv));
        }
        for ws in 37..=41 {
            assert!(storage.is_weight_src_available(ws));
        }
    }
}
