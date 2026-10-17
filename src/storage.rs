//! Flash storage for non-volatile configuration and 20-model memory system.
//!
//! Log-structured append-only storage (`sequential-storage`):
//! - Pages 60..63: 0x0801_E000 .. 0x0802_0000 (8,192 bytes across 4 pages)
//!
//! Total capacity: 8,192 bytes. Active dataset: 2,698 bytes. Log append headroom: 3,456+ bytes.

use embedded_storage_async::nor_flash::NorFlash;
use stm32f0xx_hal::pac;

pub const FLASH_MAGIC: u32 = 0x4653_4B59; // "FSKY"
pub const CONFIG_VERSION: u32 = 5;
pub const NUM_MODELS: usize = 20;

const FLASH_KEY1: u32 = 0x4567_0123;
const FLASH_KEY2: u32 = 0xCDEF_89AB;

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
    TwoPos = 1,
    ThreePos = 2,
    SixPos = 3,
    Pot = 4,
    PotDetent = 5,
    InstantTrim = 6,
}

impl AdcInputMode {
    pub fn from_u8(val: u8) -> Self {
        match val {
            1 => Self::TwoPos,
            2 => Self::ThreePos,
            3 => Self::SixPos,
            4 => Self::Pot,
            5 => Self::PotDetent,
            6 => Self::InstantTrim,
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

    /// Resolve effective mode for a given channel index (0..3: SA..SD, 4..5: VRA..VRB, 6..9: VRC..VRF)
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
            _ => Self::Pot,                  // VRA, VRB, VRC, VRD, VRE, VRF (Pots stock)
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
    pub aux_pots: [PotCalib; 10],  // 48..108 (60 bytes: SA..SD, VRA..VRB, VRC..VRF)
    pub ext_module_pwr: u8,        // 108 (0: Active HIGH / N-type, 1: Active LOW / P-type)
    pub tone_style: u8,            // 109 (0: Simple / Standard, 1: Rich / Melodic)
    pub servo_rate_hz: u16,        // 110..112 (50..400 Hz, default 50 Hz for analog servo safety)
    pub rx_out_mode: u8,           // 112 (0: PWM, 1: PPM, default 0)
    pub rx_serial_proto: u8,       // 113 (0: i-BUS, 1: S.BUS, default 0)
    pub ext_switches: u8,          // 114 (0: Disabled, 1: Enabled / PC12+PC15)
    pub ext_adc: u8,               // 115 (0: Disabled, 1: Enabled / Header P7 AD12-AD15 or PC1-PC4)
    pub adc_modes: [u8; 10],       // 116..126: Input modes for the 10 auxiliary analog channels
    pub stick_mode: u8,            // 126 (0: Mode 1, 1: Mode 2, 2: Mode 3, 3: Mode 4)
    pub telem_slots: [u8; 4],      // 127..131: Configurable telemetry dashboard sensor slots (1..4)
    pub trainer_config: u8,        // 131: Bit 0: PPM Out on PF10, Bit 1: PPM In on PF9
    pub rear_left_func: u8,        // 132: 0: RC Switch (SWE), 1: Timer Reset, 2: Trims Modifier
    pub rear_right_func: u8,       // 133: 0: RC Switch (SWF), 1: Instant Trim, 2: Trims Modifier
    pub touch_disabled: u8,        // 134: 0: Touch Enabled, 1: Touch Disabled
    pub j4_sg_sh_en: u8,           // 135: 0: SWD Debug, 1: SG/SH Buttons on PA13/PA14
    pub switch_beep: u8,           // 136: 0: Muted, 1: Beep on switch toggle
    pub _pad_align: [u8; 3],       // 137..140: Align to 140 bytes (4-byte word boundary)
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
            ],
            ext_module_pwr: 0,
            tone_style: 1,
            servo_rate_hz: 50,
            rx_out_mode: 0,
            rx_serial_proto: 0,
            ext_switches: 0,
            ext_adc: 0,
            adc_modes: [0; 10], // Default hardware modes
            stick_mode: 1,      // Mode 2 default
            telem_slots: [0, 1, 2, 3], // Default: RxBat, RSSI/LQ, TxBat, Current/Link
            trainer_config: 0,
            rear_left_func: 0,
            rear_right_func: 0,
            touch_disabled: 0,
            j4_sg_sh_en: 0,
            switch_beep: 1,
            _pad_align: [0; 3],
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
        if ch < 10 {
            self.adc_modes[ch]
        } else {
            0
        }
    }

    /// Set raw configured ADC mode index (0..=6) for auxiliary input `ch` (`0..10`).
    #[inline(always)]
    pub fn set_adc_mode_val(&mut self, ch: usize, val: u8) {
        if ch < 10 {
            self.adc_modes[ch] = val.min(6);
        }
    }

    /// Get the effective AdcInputMode for an auxiliary channel (0..9: SA..SD, VRA..VRB, VRC..VRF).
    pub fn get_adc_mode(&self, ch: usize) -> AdcInputMode {
        if ch < 10 {
            AdcInputMode::resolve(ch, self.adc_modes[ch])
        } else {
            AdcInputMode::Default
        }
    }

    /// Get the configured raw AdcInputMode (including Default).
    pub fn get_raw_adc_mode(&self, ch: usize) -> AdcInputMode {
        if ch < 10 {
            AdcInputMode::from_u8(self.adc_modes[ch])
        } else {
            AdcInputMode::Default
        }
    }
}

/// A single freeform mix rule in the matrix mixer (6 bytes).
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct MixLine {
    pub target_ch: u8, // 0: Disabled, 1..18: Target Channel CH1..CH18
    pub source: u8, // 0: None, 1: Roll, 2: Pitch, 3: Thr, 4: Yaw, 5: VRA, 6: VRB, 7: SA, 8: SB, 9: SC, 10: SD, 11: MAX, 12..29: CH1..CH18, 30: ThrUnipolar, 31..42: SE..VRF
    pub weight: i8, // -100% .. +100%
    pub offset: i8, // -100% .. +100%
    pub switch: u8, // 0: Always On, 1..26: SA_UP..SWL_DN
    pub mode: u8,   // 0: Add (+), 1: Multiply (*), 2: Replace (:=)
}

impl MixLine {
    pub const fn disabled() -> Self {
        Self {
            target_ch: 0,
            source: 0,
            weight: 100,
            offset: 0,
            switch: 0,
            mode: 0,
        }
    }
}

/// Complete profile for an individual aircraft/model (exactly 128 bytes).
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
    pub thr_curve_pts: u8,      // 25: 5 or 9 points mode
    pub thr_curve_smooth: u8,   // 26: 0: Linear, 1: Spline / Smooth
    pub thr_curve: [u8; 9],     // 27..36: Points 1..9 (0..100%)
    pub dr_high: [u8; 3],       // 36..39: High rate (AIL, ELE, RUD: 50..100%)
    pub dr_low: [u8; 3],        // 39..42: Low rate (AIL, ELE, RUD: 30..100%)
    pub expo_high: [i8; 3],     // 42..45: High expo (-100..+100%)
    pub expo_low: [i8; 3],      // 45..48: Low expo (-100..+100%)
    pub timer_secs: u16,        // 48..50: Countdown timer in seconds (e.g. 300 = 5 min)
    pub timer_source: u8, // 50: 0: Off, 1: THs (Thr>5%), 2: THt (Thr Latched), 3: Always On, 4..17: SA^..SFv
    pub rf_power: u8, // 51: AFHDS 2A RF power (0: 100mW, 1: 10mW, 2: 1mW, 3: 100uW)
    pub failsafe_thr: u16, // 52..54: Failsafe throttle pulse in µs (e.g. 1000)
    pub aux_channels: [u8; 14], // 54..68: Source for CH5..CH18
    pub wing_tail_mix: u8, // 68: 0: Normal, 1: Elevon/Delta, 2: V-Tail, 3: Flaperon
    pub template_diff: i8, // 69: Differential / mix ratio (-100..+100)
    pub mixes: [MixLine; 8], // 70..118: 8 freeform mix rules (8 * 6 = 48 bytes)
    pub failsafe_mode: u8, // 118: 0: Hold last, 1: Custom pulses
    pub failsafe_timeout: u8, // 119: 10..50 (1.0s..5.0s)
    pub rf_protocol: u8,  // 120: 0: AFHDS 2A, 1: CRSF / ELRS
    pub crsf_baud: u8,    // 121: 0: 115.2k, 1: 416.6k, 2: 420k, 3: 921.6k, 4: 1.875M
    pub arm_switch: u8, // 122: 0: None, 1..14: SA^..SFv
    pub rx_out_mode: u8, // 123: 0: PWM, 1: PPM (default 0)
    pub servo_rate_hz: u16, // 124..126: 50..400 Hz (default 50 Hz)
    pub rx_serial_proto: u8, // 126: 0: i-BUS, 1: S.BUS (default 0)
    pub crsf_half_duplex: u8, // 127: 0: Full-Duplex (2-Wire), 1: Half-Duplex (1-Wire)
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
            thr_curve: [0, 25, 50, 75, 100, 0, 0, 0, 0],
            dr_high: [100, 100, 100],
            dr_low: [70, 70, 70],
            expo_high: [0, 0, 0],
            expo_low: [0, 0, 0],
            timer_secs: 300,
            timer_source: 1,
            rf_power: 0,
            failsafe_thr: 1000,
            aux_channels: [7, 8, 5, 6, 9, 10, 0, 0, 0, 0, 0, 0, 0, 0],
            wing_tail_mix: 0,
            template_diff: 0,
            mixes: [MixLine::disabled(); 8],
            failsafe_mode: 0,
            failsafe_timeout: 20,
            rf_protocol: 0,
            crsf_baud: 2,
            arm_switch: 0,
            servo_rate_hz: 50,
            rx_out_mode: 0,
            rx_serial_proto: 0,
            crsf_half_duplex: 0,
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

/// Complete Flash storage layout containing radio settings and 20 models (2,698 bytes).
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct RadioStorage {
    pub radio: RadioConfig,
    pub models: [ModelConfig; NUM_MODELS],
}

impl RadioStorage {
    pub const fn empty() -> Self {
        Self {
            radio: RadioConfig::default_factory(),
            models: [ModelConfig::default_for_index(0); NUM_MODELS],
        }
    }

    #[allow(dead_code)]
    pub const fn default_factory() -> Self {
        let mut models = [ModelConfig::default_for_index(0); NUM_MODELS];
        let mut i = 0;
        while i < NUM_MODELS {
            models[i] = ModelConfig::default_for_index(i);
            i += 1;
        }
        Self {
            radio: RadioConfig::default_factory(),
            models,
        }
    }

    pub fn init_default(&mut self) {
        self.radio = RadioConfig::default_factory();
        let mut i = 0;
        while i < NUM_MODELS {
            self.models[i] = ModelConfig::default_for_index(i);
            i += 1;
        }
    }

    pub fn active_model(&self) -> &ModelConfig {
        let idx = (self.radio.active_model as usize).min(NUM_MODELS - 1);
        &self.models[idx]
    }

    pub fn active_model_mut(&mut self) -> &mut ModelConfig {
        let idx = (self.radio.active_model as usize).min(NUM_MODELS - 1);
        &mut self.models[idx]
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

        for (idx, m) in self.models.iter_mut().enumerate() {
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
                if *src > 42 {
                    *src = 0;
                }
            }
            for mix in m.mixes.iter_mut() {
                if mix.target_ch > 18 || mix.source > 42 || mix.mode > 2 || mix.switch > 26 {
                    *mix = MixLine::disabled();
                }
            }
            if m.thr_curve_pts != 5 && m.thr_curve_pts != 9 {
                m.thr_curve_pts = 5;
                m.thr_curve = [0, 25, 50, 75, 100, 0, 0, 0, 0];
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
                let num = (idx + 1) as u8;
                let digit1 = b'0' + (num / 10);
                let digit2 = b'0' + (num % 10);
                m.name = [
                    b'M', b'O', b'D', b'E', b'L', b' ', digit1, digit2, b' ', b' ',
                ];
            }
        }
    }
}

const _: () = assert!(core::mem::size_of::<RadioConfig>() == 140);
const _: () = assert!(core::mem::size_of::<ModelConfig>() == 128);
const _: () = assert!(core::mem::size_of::<RadioStorage>() == 2700);

static mut GLOBAL_STORAGE: RadioStorage = RadioStorage::empty();

/// Get global mutable reference to persistent RadioStorage.
#[allow(static_mut_refs)]
pub fn get_storage() -> &'static mut RadioStorage {
    unsafe { &mut GLOBAL_STORAGE }
}

pub const FLASH_STORAGE_ADDR: usize = 0x0801_E000;
pub const FLASH_STORAGE_END: usize = 0x0802_0000;
pub const FLASH_LEGACY_SNAPSHOT_ADDR: usize = 0x0801_F000;
pub const FLASH_LEGACY_ADDR: usize = 0x0801_F800;

pub const KEY_RADIO: u8 = 0;
pub const KEY_MODEL_BASE: u8 = 1;

#[allow(dead_code)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum FlashError {
    AddressMisaligned,
    LengthMisaligned,
    OutOfBounds,
    ProgrammingError,
    WriteProtectionError,
    Timeout,
    UnlockFailed,
}

impl core::fmt::Display for FlashError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl embedded_storage_async::nor_flash::NorFlashError for FlashError {
    fn kind(&self) -> embedded_storage_async::nor_flash::NorFlashErrorKind {
        match self {
            FlashError::AddressMisaligned | FlashError::LengthMisaligned => {
                embedded_storage_async::nor_flash::NorFlashErrorKind::NotAligned
            }
            FlashError::OutOfBounds => {
                embedded_storage_async::nor_flash::NorFlashErrorKind::OutOfBounds
            }
            _ => embedded_storage_async::nor_flash::NorFlashErrorKind::Other,
        }
    }
}

#[derive(Clone, Copy)]
pub struct Stm32Flash;

impl embedded_storage_async::nor_flash::ErrorType for Stm32Flash {
    type Error = FlashError;
}

impl embedded_storage_async::nor_flash::ReadNorFlash for Stm32Flash {
    const READ_SIZE: usize = 1;

    async fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        let capacity = self.capacity() as u32;
        if offset as usize + bytes.len() > capacity as usize {
            return Err(FlashError::OutOfBounds);
        }
        let src = offset as *const u8;
        unsafe {
            for (i, b) in bytes.iter_mut().enumerate() {
                *b = core::ptr::read_volatile(src.add(i));
            }
        }
        Ok(())
    }

    fn capacity(&self) -> usize {
        0x0802_0000
    }
}

impl embedded_storage_async::nor_flash::NorFlash for Stm32Flash {
    const WRITE_SIZE: usize = 2; // 16-bit halfword programming on STM32F0
    const ERASE_SIZE: usize = 2048; // 2 KB page erase

    async fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        if !from.is_multiple_of(2048) || !to.is_multiple_of(2048) || from >= to {
            return Err(FlashError::AddressMisaligned);
        }

        cortex_m::interrupt::free(|_| unsafe {
            let flash = &*pac::FLASH::ptr();

            if flash.cr.read().lock().bit_is_set() {
                flash.keyr.write(|w| w.bits(FLASH_KEY1));
                flash.keyr.write(|w| w.bits(FLASH_KEY2));
            }
            if flash.cr.read().lock().bit_is_set() {
                return Err(FlashError::UnlockFailed);
            }

            let mut page_addr = from;
            while page_addr < to {
                crate::watchdog::feed();

                let mut timeout = 1_000_000u32;
                while flash.sr.read().bsy().bit_is_set() && timeout > 0 {
                    timeout -= 1;
                }
                if timeout == 0 {
                    return Err(FlashError::Timeout);
                }

                flash
                    .sr
                    .write(|w| w.eop().event().wrprt().error().pgerr().error());

                flash.cr.write_with_zero(|w| w.per().page_erase());
                flash.ar.write_with_zero(|w| w.far().bits(page_addr));
                flash
                    .cr
                    .write_with_zero(|w| w.per().page_erase().strt().start());

                timeout = 1_000_000;
                while flash.sr.read().bsy().bit_is_set() && timeout > 0 {
                    timeout -= 1;
                }
                if timeout == 0 {
                    return Err(FlashError::Timeout);
                }

                if flash.sr.read().wrprt().is_error() {
                    flash.sr.write(|w| w.wrprt().error());
                    return Err(FlashError::WriteProtectionError);
                }
                flash.sr.write(|w| w.eop().event());

                page_addr += 2048;
            }

            flash.cr.write_with_zero(|w| w.bits(0));
            flash.cr.write(|w| w.lock().locked());
            crate::watchdog::feed();

            Ok(())
        })
    }

    async fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        if !offset.is_multiple_of(2) || !bytes.len().is_multiple_of(2) {
            return Err(FlashError::AddressMisaligned);
        }
        if bytes.is_empty() {
            return Ok(());
        }

        cortex_m::interrupt::free(|_| unsafe {
            let flash = &*pac::FLASH::ptr();

            if flash.cr.read().lock().bit_is_set() {
                flash.keyr.write(|w| w.bits(FLASH_KEY1));
                flash.keyr.write(|w| w.bits(FLASH_KEY2));
            }
            if flash.cr.read().lock().bit_is_set() {
                return Err(FlashError::UnlockFailed);
            }

            let mut timeout = 1_000_000u32;
            while flash.sr.read().bsy().bit_is_set() && timeout > 0 {
                timeout -= 1;
            }
            if timeout == 0 {
                return Err(FlashError::Timeout);
            }

            flash
                .sr
                .write(|w| w.eop().event().wrprt().error().pgerr().error());
            flash.cr.write_with_zero(|w| w.pg().program());

            let halfwords = bytes.len() / 2;
            let src = bytes.as_ptr() as *const u16;
            let dst = offset as *mut u16;

            for i in 0..halfwords {
                let hw = core::ptr::read_unaligned(src.add(i));
                core::ptr::write_volatile(dst.add(i), hw);

                timeout = 100_000;
                while flash.sr.read().bsy().bit_is_set() && timeout > 0 {
                    timeout -= 1;
                }
                if timeout == 0 {
                    flash.cr.write_with_zero(|w| w.bits(0));
                    flash.cr.write(|w| w.lock().locked());
                    return Err(FlashError::Timeout);
                }

                let sr = flash.sr.read();
                if sr.pgerr().is_error() {
                    flash.sr.write(|w| w.pgerr().error());
                    flash.cr.write_with_zero(|w| w.bits(0));
                    flash.cr.write(|w| w.lock().locked());
                    return Err(FlashError::ProgrammingError);
                }
                if sr.wrprt().is_error() {
                    flash.sr.write(|w| w.wrprt().error());
                    flash.cr.write_with_zero(|w| w.bits(0));
                    flash.cr.write(|w| w.lock().locked());
                    return Err(FlashError::WriteProtectionError);
                }
                flash.sr.write(|w| w.eop().event());

                if (i & 0x7F) == 0 {
                    crate::watchdog::feed();
                }
            }

            flash.cr.write_with_zero(|w| w.bits(0));
            flash.cr.write(|w| w.lock().locked());
            crate::watchdog::feed();

            Ok(())
        })
    }
}

impl embedded_storage_async::nor_flash::MultiwriteNorFlash for Stm32Flash {}

fn block_on<F: core::future::Future>(mut future: F) -> F::Output {
    use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

    fn noop_clone(_: *const ()) -> RawWaker {
        RawWaker::new(core::ptr::null(), &NOOP_VTABLE)
    }
    fn noop(_: *const ()) {}

    static NOOP_VTABLE: RawWakerVTable = RawWakerVTable::new(noop_clone, noop, noop, noop);
    let raw_waker = RawWaker::new(core::ptr::null(), &NOOP_VTABLE);
    let waker = unsafe { Waker::from_raw(raw_waker) };
    let mut cx = Context::from_waker(&waker);

    let mut pinned = unsafe { core::pin::Pin::new_unchecked(&mut future) };
    loop {
        match pinned.as_mut().poll(&mut cx) {
            Poll::Ready(output) => return output,
            Poll::Pending => {
                crate::watchdog::feed();
            }
        }
    }
}

#[inline(always)]
fn storage_config() -> sequential_storage::map::MapConfig<Stm32Flash> {
    sequential_storage::map::MapConfig::new(FLASH_STORAGE_ADDR as u32..FLASH_STORAGE_END as u32)
}

impl<'a> sequential_storage::map::Value<'a> for RadioConfig {
    fn serialize_into(
        &self,
        buffer: &mut [u8],
    ) -> Result<usize, sequential_storage::map::SerializationError> {
        let size = core::mem::size_of::<Self>();
        if buffer.len() < size {
            return Err(sequential_storage::map::SerializationError::BufferTooSmall);
        }
        let src = self as *const Self as *const u8;
        unsafe {
            core::ptr::copy_nonoverlapping(src, buffer.as_mut_ptr(), size);
        }
        Ok(size)
    }

    fn deserialize_from(
        buffer: &'a [u8],
    ) -> Result<(Self, usize), sequential_storage::map::SerializationError> {
        let size = core::mem::size_of::<Self>();
        if buffer.len() < size {
            return Err(sequential_storage::map::SerializationError::BufferTooSmall);
        }
        let mut val = Self::default_factory();
        let dst = &mut val as *mut Self as *mut u8;
        unsafe {
            core::ptr::copy_nonoverlapping(buffer.as_ptr(), dst, size);
        }
        Ok((val, size))
    }
}

impl<'a> sequential_storage::map::Value<'a> for ModelConfig {
    fn serialize_into(
        &self,
        buffer: &mut [u8],
    ) -> Result<usize, sequential_storage::map::SerializationError> {
        let size = core::mem::size_of::<Self>();
        if buffer.len() < size {
            return Err(sequential_storage::map::SerializationError::BufferTooSmall);
        }
        let src = self as *const Self as *const u8;
        unsafe {
            core::ptr::copy_nonoverlapping(src, buffer.as_mut_ptr(), size);
        }
        Ok(size)
    }

    fn deserialize_from(
        buffer: &'a [u8],
    ) -> Result<(Self, usize), sequential_storage::map::SerializationError> {
        let size = core::mem::size_of::<Self>();
        if buffer.len() < size {
            return Err(sequential_storage::map::SerializationError::BufferTooSmall);
        }
        let mut val = Self::default_for_index(0);
        let dst = &mut val as *mut Self as *mut u8;
        unsafe {
            core::ptr::copy_nonoverlapping(buffer.as_ptr(), dst, size);
        }
        Ok((val, size))
    }
}

fn format_and_save_all(storage: &RadioStorage) {
    let mut flash = Stm32Flash;
    let _ = block_on(flash.erase(FLASH_STORAGE_ADDR as u32, FLASH_STORAGE_END as u32));

    let mut map = sequential_storage::map::MapStorage::new(
        flash,
        storage_config(),
        sequential_storage::cache::Cache::new_uncached(),
    );
    let mut buf = [0u8; 256];
    let _ = block_on(map.store_item(&mut buf, &KEY_RADIO, &storage.radio));
    for idx in 0..NUM_MODELS {
        let key = KEY_MODEL_BASE + idx as u8;
        let _ = block_on(map.store_item(&mut buf, &key, &storage.models[idx]));
    }
}

#[inline(never)]
pub fn load_storage_into(storage: &mut RadioStorage) {
    #[cfg(test)]
    {
        storage.init_default();
        return;
    }

    #[cfg(not(test))]
    {
        let mut map = sequential_storage::map::MapStorage::new(
            Stm32Flash,
            storage_config(),
            sequential_storage::cache::Cache::new_uncached(),
        );
        let mut buf = [0u8; 256];

        let radio_res: Result<Option<RadioConfig>, _> =
            block_on(map.fetch_item(&mut buf, &KEY_RADIO));
        if let Ok(Some(cfg)) = radio_res {
            if cfg.magic == FLASH_MAGIC && cfg.version == CONFIG_VERSION {
                storage.radio = cfg;
                for idx in 0..NUM_MODELS {
                    let key = KEY_MODEL_BASE + idx as u8;
                    if let Ok(Some(m)) = block_on(map.fetch_item(&mut buf, &key)) {
                        storage.models[idx] = m;
                    } else {
                        storage.models[idx] = ModelConfig::default_for_index(idx);
                    }
                }
                storage.sanitize();
                return;
            }
        }

        // Migration: Check legacy snapshot base
        unsafe {
            let snap_magic = core::ptr::read_volatile(FLASH_LEGACY_SNAPSHOT_ADDR as *const u32);
            let snap_ver = core::ptr::read_volatile((FLASH_LEGACY_SNAPSHOT_ADDR + 4) as *const u32);
            if snap_magic == FLASH_MAGIC && snap_ver == CONFIG_VERSION {
                let src = FLASH_LEGACY_SNAPSHOT_ADDR as *const u32;
                let dst = storage as *mut RadioStorage as *mut u32;
                let word_count = core::mem::size_of::<RadioStorage>() / 4;
                for i in 0..word_count {
                    *dst.add(i) = core::ptr::read_volatile(src.add(i));
                }
                storage.sanitize();
                format_and_save_all(storage);
                return;
            }

            // Clean factory default initialization
            storage.init_default();
            format_and_save_all(storage);
        }
    }
}

#[allow(dead_code)]
pub fn load_storage() -> RadioStorage {
    let mut storage = RadioStorage::empty();
    load_storage_into(&mut storage);
    storage
}

pub fn save_active_model(storage: &RadioStorage) -> bool {
    #[cfg(test)]
    {
        let _ = storage;
        return true;
    }
    #[cfg(not(test))]
    {
        let mut map = sequential_storage::map::MapStorage::new(
            Stm32Flash,
            storage_config(),
            sequential_storage::cache::Cache::new_uncached(),
        );
        let mut buf = [0u8; 256];
        let idx = (storage.radio.active_model as usize).min(NUM_MODELS - 1);
        let key = KEY_MODEL_BASE + idx as u8;
        block_on(map.store_item(&mut buf, &key, &storage.models[idx])).is_ok()
    }
}

pub fn save_radio_config(storage: &RadioStorage) -> bool {
    #[cfg(test)]
    {
        let _ = storage;
        return true;
    }
    #[cfg(not(test))]
    {
        let mut map = sequential_storage::map::MapStorage::new(
            Stm32Flash,
            storage_config(),
            sequential_storage::cache::Cache::new_uncached(),
        );
        let mut buf = [0u8; 256];
        block_on(map.store_item(&mut buf, &KEY_RADIO, &storage.radio)).is_ok()
    }
}

pub fn save_storage(storage: &RadioStorage) -> bool {
    let mut ok = save_radio_config(storage);
    ok = ok && save_active_model(storage);
    ok
}

pub fn load_config() -> RadioConfig {
    let storage = get_storage();
    load_storage_into(storage);
    storage.radio
}

/// Read persisted receiver ID from Flash if previously bound.
pub fn load_saved_rx_id() -> Option<u32> {
    let mut map = sequential_storage::map::MapStorage::new(
        Stm32Flash,
        storage_config(),
        sequential_storage::cache::Cache::new_uncached(),
    );
    let mut buf = [0u8; 256];
    if let Ok(Some(radio)) = block_on(map.fetch_item::<RadioConfig>(&mut buf, &KEY_RADIO)) {
        let active_idx = (radio.active_model as usize).min(NUM_MODELS - 1);
        let key = KEY_MODEL_BASE + active_idx as u8;
        if let Ok(Some(model)) = block_on(map.fetch_item::<ModelConfig>(&mut buf, &key)) {
            if model.rx_id != 0 && model.rx_id != 0xFFFF_FFFF {
                return Some(model.rx_id);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adc_input_mode_detent() {
        assert_eq!(AdcInputMode::from_u8(4), AdcInputMode::Pot);
        assert_eq!(AdcInputMode::from_u8(5), AdcInputMode::PotDetent);
        assert_eq!(AdcInputMode::from_u8(6), AdcInputMode::InstantTrim);
        assert!(AdcInputMode::Pot.is_pot());
        assert!(AdcInputMode::PotDetent.is_pot());
        assert!(!AdcInputMode::InstantTrim.is_pot());
        assert!(!AdcInputMode::Pot.has_detent());
        assert!(AdcInputMode::PotDetent.has_detent());
    }

    #[test]
    fn test_pot_calib_and_radio_config_defaults() {
        let radio = RadioConfig::default_factory();
        assert_eq!(radio.aux_pots.len(), 10);
        assert_eq!(radio.adc_modes.len(), 10);
        for i in 0..10 {
            assert_eq!(radio.aux_pots[i].min, crate::adc::ADC_MIN);
            assert_eq!(radio.aux_pots[i].center, crate::adc::ADC_CENTER);
            assert_eq!(radio.aux_pots[i].max, crate::adc::ADC_MAX);
            assert_eq!(radio.adc_modes[i], 0);
        }

        let pot = PotCalib::new(100, 2000, 3900);
        assert_eq!(pot.min, 100);
        assert_eq!(pot.center, 2000);
        assert_eq!(pot.max, 3900);
    }

    #[test]
    fn test_model_config_size_and_rf_power() {
        assert_eq!(core::mem::size_of::<ModelConfig>(), 128);
        assert_eq!(core::mem::size_of::<RadioConfig>(), 140);

        let default_m = ModelConfig::default_for_index(0);
        assert_eq!(default_m.rf_power, 0);

        let mut storage = RadioStorage::default_factory();
        storage.models[0].rf_power = 99;
        storage.sanitize();
        assert_eq!(storage.models[0].rf_power, 0);
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
}
