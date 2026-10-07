//! Flash storage for non-volatile configuration and 20-model memory system.
//!
//! Log-structured append-only storage (`sequential-storage`):
//! - Pages 60..63: 0x0801_E000 .. 0x0802_0000 (8,192 bytes across 4 pages)
//!
//! Total capacity: 8,192 bytes. Active dataset: 2,688 bytes. Log append headroom: 3,456+ bytes.

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

/// Persistent system/radio-level configuration (exactly 128 bytes).
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
    pub pots: [ChannelCalib; 2],   // 48..64 (16 bytes: VRA, VRB)
    pub ext_module_pwr: u8,        // 64 (0: Active HIGH / N-type, 1: Active LOW / P-type)
    pub tone_style: u8,            // 65 (0: Simple / Standard, 1: Rich / Melodic)
    pub servo_rate_hz: u16,        // 66..68 (50..400 Hz, default 50 Hz for analog servo safety)
    pub rx_out_mode: u8,           // 68 (0: PWM, 1: PPM, default 0)
    pub rx_serial_proto: u8,       // 69 (0: i-BUS, 1: S.BUS, default 0)
    pub rear_left_func: u8,        // 70 (0: SW-E, 1: Timer Reset, 2: Trims)
    pub rear_right_func: u8,       // 71 (0: SW-F, 1: Instant Trim, 2: Trims)
    pub _reserved: [u8; 56],       // 72..128
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
            pots: [
                ChannelCalib::new(
                    crate::adc::ADC_MIN,
                    crate::adc::ADC_CENTER,
                    crate::adc::ADC_MAX,
                ), // VRA
                ChannelCalib::new(
                    crate::adc::ADC_MIN,
                    crate::adc::ADC_CENTER,
                    crate::adc::ADC_MAX,
                ), // VRB
            ],
            ext_module_pwr: 0,
            tone_style: 1,
            servo_rate_hz: 50,
            rx_out_mode: 0,
            rx_serial_proto: 0,
            rear_left_func: 0,
            rear_right_func: 0,
            _reserved: [0; 56],
        }
    }
}

/// A single freeform mix rule in the matrix mixer (6 bytes).
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct MixLine {
    pub target_ch: u8, // 0: Disabled, 1..18: Target Channel CH1..CH18
    pub source: u8, // 0: None, 1: Roll, 2: Pitch, 3: Thr, 4: Yaw, 5: VRA, 6: VRB, 7: SA, 8: SB, 9: SC, 10: SD, 11: MAX, 12..29: CH1..CH18, 30: ThrUnipolar
    pub weight: i8, // -100% .. +100%
    pub offset: i8, // -100% .. +100%
    pub switch: u8, // 0: Always On, 1: SA_UP, 2: SA_DN, 3: SB_UP, 4: SB_MID, 5: SB_DN, 6: SC_UP, 7: SC_MID, 8: SC_DN, 9: SD_UP, 10: SD_DN
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
    pub channel_reverse: u32,   // 20..24: Bitmask for CH1..CH18 inversion (was u16 20..22)
    pub dr_switch: u8,          // 24: 0: None, 1: SA, 2: SB, 3: SC, 4: SD
    pub thr_curve_pts: u8,      // 25: 5 or 9 points mode
    pub thr_curve_smooth: u8,   // 26: 0: Linear, 1: Spline / Smooth
    pub thr_curve: [u8; 9],     // 27..36: Points 1..9 (0..100%)
    pub dr_high: [u8; 3],       // 36..39: High rate (AIL, ELE, RUD: 50..100%)
    pub dr_low: [u8; 3],        // 39..42: Low rate (AIL, ELE, RUD: 30..100%)
    pub expo_high: [i8; 3],     // 42..45: High expo (-100..+100%)
    pub expo_low: [i8; 3],      // 45..48: Low expo (-100..+100%)
    pub timer_secs: u16,        // 48..50: Countdown timer in seconds (e.g. 300 = 5 min)
    pub timer_source: u8, // 50: 0: Off, 1: THs (Thr>5%), 2: THt (Thr Latched), 3: Always On, 4..13: SA^..SDv
    pub protocol_subtype: u8, // 51: 0: PWM, 1: PPM, 2: i-BUS, 3: S.BUS
    pub failsafe_thr: u16, // 52..54: Failsafe throttle pulse in µs (e.g. 1000)
    pub aux_channels: [u8; 14], // 54..68: Source for CH5..CH18 (was [u8; 10] 52..62)
    pub wing_tail_mix: u8, // 68: 0: Normal, 1: Elevon/Delta, 2: V-Tail, 3: Flaperon
    pub template_diff: i8, // 69: Differential / mix ratio (-100..+100)
    pub mixes: [MixLine; 8], // 70..118: 8 freeform mix rules (8 * 6 = 48 bytes)
    pub failsafe_mode: u8, // 118: 0: Hold last, 1: Custom pulses
    pub failsafe_timeout: u8, // 119: 10..50 (1.0s..5.0s)
    pub rf_protocol: u8,  // 120: 0: AFHDS 2A, 1: CRSF / ELRS
    pub crsf_baud: u8,    // 121: 0: 420k, 1: 416.6k, 2: 115.2k, 3: 921.6k
    pub arm_switch: u8, // 122: 0: None, 1: SA^, 2: SAv, 3: SB^, 4: SB-, 5: SBv, 6: SC^, 7: SC-, 8: SCv, 9: SD^, 10: SDv
    pub rx_out_mode: u8, // 123: 0: PWM, 1: PPM (default 0)
    pub servo_rate_hz: u16, // 124..126: 50..400 Hz (default 50 Hz)
    pub rx_serial_proto: u8, // 126: 0: i-BUS, 1: S.BUS (default 0)
    pub _reserved: [u8; 1], // 127..128: 1 reserved byte
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
            protocol_subtype: 0,
            failsafe_thr: 1000,
            // Defaults for CH5..CH18: CH5=SA(7), CH6=SB(8), CH7=VRA(5), CH8=VRB(6), CH9=SC(9), CH10=SD(10), CH11..18=None(0)
            aux_channels: [7, 8, 5, 6, 9, 10, 0, 0, 0, 0, 0, 0, 0, 0],
            wing_tail_mix: 0,
            template_diff: 0,
            mixes: [MixLine::disabled(); 8],
            failsafe_mode: 0,
            failsafe_timeout: 20,
            rf_protocol: 0,
            crsf_baud: 0,
            arm_switch: 0,
            servo_rate_hz: 50,
            rx_out_mode: 0,
            rx_serial_proto: 0,
            _reserved: [0; 1],
        }
    }
}

/// Complete Flash storage layout containing radio settings and 20 models (2,688 bytes).
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
        Self::empty()
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
    /// Prevents uninitialized Flash bytes or corrupted rates/mixes from impairing flight controls.
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
        if self.radio.rear_left_func > 2 {
            self.radio.rear_left_func = 0;
        }
        if self.radio.rear_right_func > 2 {
            self.radio.rear_right_func = 0;
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
        for pot in self.radio.pots.iter_mut() {
            if pot.min >= pot.center || pot.center >= pot.max || pot.max > crate::adc::ADC_MAX {
                pot.min = crate::adc::ADC_MIN;
                pot.center = crate::adc::ADC_CENTER;
                pot.max = crate::adc::ADC_MAX;
            }
        }

        for (idx, m) in self.models.iter_mut().enumerate() {
            if m.rf_protocol > 1 {
                m.rf_protocol = 0;
            }
            if m.crsf_baud > 3 {
                m.crsf_baud = 0;
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
            if m.dr_switch > 4 {
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
            // If aux_channels are all 0 (uninitialized Flash from v0.9.x and earlier), restore defaults
            if m.aux_channels[0..6] == [0; 6] {
                m.aux_channels = [7, 8, 5, 6, 9, 10, 0, 0, 0, 0, 0, 0, 0, 0];
            }
            for src in m.aux_channels.iter_mut() {
                if *src > 30 {
                    *src = 0;
                }
            }
            for mix in m.mixes.iter_mut() {
                if mix.target_ch > 18 || mix.source > 30 || mix.mode > 2 || mix.switch > 10 {
                    *mix = MixLine::disabled();
                }
            }
            if m.thr_curve_pts != 5 && m.thr_curve_pts != 9 {
                m.thr_curve_pts = 5;
                m.thr_curve = [0, 25, 50, 75, 100, 0, 0, 0, 0];
            }
            if m.arm_switch > 10 {
                m.arm_switch = 0;
            }
            if m.timer_source > 13 {
                m.timer_source = 0;
            }
            if m.timer_secs > 3600 {
                m.timer_secs = 0;
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

// Compile-time size guarantees
const _: () = assert!(core::mem::size_of::<RadioConfig>() == 128);
const _: () = assert!(core::mem::size_of::<ModelConfig>() == 128);
const _: () = assert!(core::mem::size_of::<RadioStorage>() == 2688);

static mut GLOBAL_STORAGE: RadioStorage = RadioStorage::empty();

/// Get global mutable reference to persistent RadioStorage.
/// Stored in static RAM to avoid allocating 2,688 bytes on the main stack.
#[allow(static_mut_refs)]
pub fn get_storage() -> &'static mut RadioStorage {
    unsafe { &mut GLOBAL_STORAGE }
}

pub const FLASH_STORAGE_ADDR: usize = 0x0801_E000; // Page 60 start (8 KB storage across Pages 60..63)
pub const FLASH_STORAGE_END: usize = 0x0802_0000; // Page 63 end
#[allow(dead_code)]
pub const FLASH_STORAGE_PAGES: usize = 4;
pub const FLASH_LEGACY_SNAPSHOT_ADDR: usize = 0x0801_F000; // Previous snapshot base
pub const FLASH_LEGACY_ADDR: usize = 0x0801_F800; // Legacy v1/v2 base

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

/// Zero-cost synchronous executor for non-blocking embedded leaf futures.
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

#[repr(C)]
#[derive(Copy, Clone)]
struct ModelConfigV4 {
    name: [u8; 10],
    model_type: u8,
    _pad0: u8,
    rx_id: u32,
    trims: [i8; 4],
    channel_reverse: u16,
    dr_switch: u8,
    thr_curve_pts: u8,
    thr_curve_smooth: u8,
    thr_curve: [u8; 9],
    dr_high: [u8; 3],
    dr_low: [u8; 3],
    expo_high: [i8; 3],
    expo_low: [i8; 3],
    timer_secs: u16,
    timer_source: u8,
    protocol_subtype: u8,
    failsafe_thr: u16,
    aux_channels: [u8; 10],
    wing_tail_mix: u8,
    template_diff: i8,
    mixes: [MixLine; 8],
    failsafe_mode: u8,
    failsafe_timeout: u8,
    rf_protocol: u8,
    crsf_baud: u8,
    arm_switch: u8,
    _reserved: [u8; 11],
}

impl<'a> sequential_storage::map::Value<'a> for ModelConfigV4 {
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
        let mut val = core::mem::MaybeUninit::<Self>::uninit();
        unsafe {
            core::ptr::copy_nonoverlapping(buffer.as_ptr(), val.as_mut_ptr() as *mut u8, size);
            Ok((val.assume_init(), size))
        }
    }
}

/// Helper to format 4 storage pages to fresh 0xFF state and write full initial dataset into MapStorage.
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

/// Load complete storage from Flash into caller-supplied memory (0 stack allocation for storage).
#[inline(never)]
pub fn load_storage_into(storage: &mut RadioStorage) {
    let mut map = sequential_storage::map::MapStorage::new(
        Stm32Flash,
        storage_config(),
        sequential_storage::cache::Cache::new_uncached(),
    );
    let mut buf = [0u8; 256];

    // 1. Try reading RadioConfig from sequential-storage log
    let radio_res: Result<Option<RadioConfig>, _> = block_on(map.fetch_item(&mut buf, &KEY_RADIO));
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
        } else if cfg.magic == FLASH_MAGIC && (cfg.version == 4 || cfg.version == 3) {
            // Migrate v3/v4 to v5
            storage.radio = cfg;
            storage.radio.version = CONFIG_VERSION;
            for idx in 0..NUM_MODELS {
                let key = KEY_MODEL_BASE + idx as u8;
                if let Ok(Some(m4)) = block_on(map.fetch_item::<ModelConfigV4>(&mut buf, &key)) {
                    let mut m = ModelConfig::default_for_index(idx);
                    m.name = m4.name;
                    m.model_type = m4.model_type;
                    m.rx_id = m4.rx_id;
                    m.trims = m4.trims;
                    m.channel_reverse = m4.channel_reverse as u32;
                    m.dr_switch = m4.dr_switch;
                    m.thr_curve_pts = m4.thr_curve_pts;
                    m.thr_curve_smooth = m4.thr_curve_smooth;
                    m.thr_curve = m4.thr_curve;
                    m.dr_high = m4.dr_high;
                    m.dr_low = m4.dr_low;
                    m.expo_high = m4.expo_high;
                    m.expo_low = m4.expo_low;
                    m.timer_secs = m4.timer_secs;
                    m.timer_source = m4.timer_source;
                    m.protocol_subtype = m4.protocol_subtype;
                    m.failsafe_thr = m4.failsafe_thr;
                    m.aux_channels[0..10].copy_from_slice(&m4.aux_channels);
                    m.aux_channels[10..14].fill(0);
                    m.wing_tail_mix = m4.wing_tail_mix;
                    m.template_diff = m4.template_diff;
                    m.mixes = m4.mixes;
                    m.failsafe_mode = m4.failsafe_mode;
                    m.failsafe_timeout = m4.failsafe_timeout;
                    m.rf_protocol = m4.rf_protocol;
                    m.crsf_baud = m4.crsf_baud;
                    m.arm_switch = m4.arm_switch;
                    storage.models[idx] = m;
                } else {
                    storage.models[idx] = ModelConfig::default_for_index(idx);
                }
            }
            storage.sanitize();
            format_and_save_all(storage);
            return;
        }
    }

    // 2. Migration: Check previous snapshot base at 0x0801_F000
    unsafe {
        let snap_magic = core::ptr::read_volatile(FLASH_LEGACY_SNAPSHOT_ADDR as *const u32);
        let snap_ver = core::ptr::read_volatile((FLASH_LEGACY_SNAPSHOT_ADDR + 4) as *const u32);
        if snap_magic == FLASH_MAGIC
            && (snap_ver == CONFIG_VERSION || snap_ver == 4 || snap_ver == 3)
        {
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

        // 3. Migration: Check legacy v1/v2 at 0x0801_F800
        let legacy_magic = core::ptr::read_volatile(FLASH_LEGACY_ADDR as *const u32);
        let legacy_ver = core::ptr::read_volatile((FLASH_LEGACY_ADDR + 4) as *const u32);
        if legacy_magic == FLASH_MAGIC && (legacy_ver == 1 || legacy_ver == 2) {
            storage.init_default();
            let rx_id = core::ptr::read_volatile((FLASH_LEGACY_ADDR + 8) as *const u32);
            if rx_id != 0 && rx_id != 0xFFFF_FFFF {
                storage.models[0].rx_id = rx_id;
            }
            let src_sticks = (FLASH_LEGACY_ADDR + 12) as *const ChannelCalib;
            for i in 0..4 {
                storage.radio.sticks[i] = core::ptr::read_volatile(src_sticks.add(i));
            }
            let src_pots = (FLASH_LEGACY_ADDR + 44) as *const ChannelCalib;
            for i in 0..2 {
                storage.radio.pots[i] = core::ptr::read_volatile(src_pots.add(i));
            }
            if legacy_ver == 2 {
                storage.radio.throttle_trim =
                    core::ptr::read_volatile((FLASH_LEGACY_ADDR + 60) as *const u8);
                storage.radio.audio_enabled =
                    core::ptr::read_volatile((FLASH_LEGACY_ADDR + 61) as *const u8);
                storage.radio.backlight_timeout =
                    core::ptr::read_volatile((FLASH_LEGACY_ADDR + 62) as *const u8);
                storage.radio.backlight_brightness =
                    core::ptr::read_volatile((FLASH_LEGACY_ADDR + 63) as *const u8);
            }
            storage.sanitize();
            format_and_save_all(storage);
            return;
        }

        // 4. Clean factory default initialization
        storage.init_default();
        format_and_save_all(storage);
    }
}

/// Load complete storage from Flash.
#[allow(dead_code)]
pub fn load_storage() -> RadioStorage {
    let mut storage = RadioStorage::empty();
    load_storage_into(&mut storage);
    storage
}

/// Save only the active model configuration via append-only log (~2.8 ms, 0 page erase).
pub fn save_active_model(storage: &RadioStorage) -> bool {
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

/// Save only the system radio configuration via append-only log (~2.8 ms, 0 page erase).
pub fn save_radio_config(storage: &RadioStorage) -> bool {
    let mut map = sequential_storage::map::MapStorage::new(
        Stm32Flash,
        storage_config(),
        sequential_storage::cache::Cache::new_uncached(),
    );
    let mut buf = [0u8; 256];
    block_on(map.store_item(&mut buf, &KEY_RADIO, &storage.radio)).is_ok()
}

/// Save complete storage to Flash.
/// In sequential-storage append-only architecture, writes radio config and active model delta.
pub fn save_storage(storage: &RadioStorage) -> bool {
    let mut ok = save_radio_config(storage);
    ok = ok && save_active_model(storage);
    ok
}

/// Convenience helper to load current RadioConfig directly from Flash (only 128 bytes, 0 stack bloat).
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
    fn test_storage_sanitize_uninitialized_and_corrupt_data() {
        let mut storage = RadioStorage::empty();
        // Simulate dirty/uninitialized 0xFF flash pattern
        storage.radio.active_model = 255;
        storage.radio.throttle_trim = 255;
        storage.radio.tone_style = 255;
        storage.radio.vbat_warn_deci = 255;
        storage.radio.lcd_contrast = 255;
        storage.radio.usb_mode = 255;
        storage.radio.ext_module_pwr = 255;
        storage.radio.servo_rate_hz = 65535;
        storage.radio.rx_out_mode = 255;
        storage.radio.rx_serial_proto = 255;

        for m in storage.models.iter_mut() {
            m.arm_switch = 255;
            m.timer_source = 255;
            m.timer_secs = 65535;
            m.rf_protocol = 255;
            m.crsf_baud = 255;
            m.dr_switch = 255;
            m.wing_tail_mix = 255;
            m.failsafe_thr = 65535;
            m.servo_rate_hz = 65535;
            m.rx_out_mode = 255;
            m.rx_serial_proto = 255;
            m.name[0] = 0xFF;
            m.mixes[0].target_ch = 20;
            m.mixes[0].source = 50;
        }

        storage.sanitize();

        assert_eq!(storage.radio.active_model, 0);
        assert_eq!(storage.radio.throttle_trim, 0);
        assert_eq!(storage.radio.tone_style, 1);
        assert_eq!(storage.radio.vbat_warn_deci, 44);
        assert_eq!(storage.radio.lcd_contrast, 37);
        assert_eq!(storage.radio.usb_mode, 0);
        assert_eq!(storage.radio.ext_module_pwr, 0);
        assert_eq!(storage.radio.servo_rate_hz, 50);
        assert_eq!(storage.radio.rx_out_mode, 0);
        assert_eq!(storage.radio.rx_serial_proto, 0);
        assert_eq!(storage.radio.rear_left_func, 0);
        assert_eq!(storage.radio.rear_right_func, 0);

        for (idx, m) in storage.models.iter().enumerate() {
            assert!(m.arm_switch <= 10, "arm_switch must be sanitized <= 10");
            assert_eq!(m.arm_switch, 0);
            assert!(m.timer_source <= 13, "timer_source must be sanitized <= 13");
            assert_eq!(m.timer_source, 0);
            assert_eq!(m.timer_secs, 0);
            assert_eq!(m.servo_rate_hz, 50);
            assert_eq!(m.rx_out_mode, 0);
            assert_eq!(m.rx_serial_proto, 0);
            assert_eq!(m.rf_protocol, 0);
            assert_eq!(m.crsf_baud, 0);
            assert_eq!(m.dr_switch, 0);
            assert_eq!(m.wing_tail_mix, 0);
            assert_eq!(m.failsafe_thr, 1000);
            assert_eq!(m.mixes[0].target_ch, 0);
            assert_eq!(m.name[0], b'M');
            assert_eq!(m.name[5], b' ');
            let expected_digit1 = b'0' + ((idx + 1) / 10) as u8;
            let expected_digit2 = b'0' + ((idx + 1) % 10) as u8;
            assert_eq!(m.name[6], expected_digit1);
            assert_eq!(m.name[7], expected_digit2);
        }
    }

    #[test]
    fn test_storage_buffer_capacity_for_sequential_storage() {
        use sequential_storage::map::Value;

        let model = ModelConfig::default_for_index(0);
        let radio = RadioConfig::default_factory();
        let key = 10u8;

        // In sequential_storage, buffer holds key + value.
        // A 128-byte buffer fails because key takes >= 1 byte, leaving < 128 bytes for ModelConfig!
        let mut buf_128 = [0u8; 128];
        let key_len = key.serialize_into(&mut buf_128).unwrap();
        let err = model.serialize_into(&mut buf_128[key_len..]);
        assert!(
            err.is_err(),
            "128-byte buffer must fail because ModelConfig is 128 bytes and key needs space"
        );

        // A 256-byte buffer succeeds with plenty of room for key + ModelConfig / RadioConfig
        let mut buf_256 = [0u8; 256];
        let key_len = key.serialize_into(&mut buf_256).unwrap();
        let val_len = model.serialize_into(&mut buf_256[key_len..]).unwrap();
        assert_eq!(val_len, 128);
        assert!(key_len + val_len <= 256);

        let (deserialized_model, d_len) =
            ModelConfig::deserialize_from(&buf_256[key_len..][..val_len]).unwrap();
        assert_eq!(d_len, 128);
        assert_eq!(deserialized_model.name, model.name);

        let key_radio_len = KEY_RADIO.serialize_into(&mut buf_256).unwrap();
        let radio_len = radio.serialize_into(&mut buf_256[key_radio_len..]).unwrap();
        assert_eq!(radio_len, 128);
        assert!(key_radio_len + radio_len <= 256);
    }

    #[test]
    fn test_storage_struct_sizes_and_v4_migration() {
        assert_eq!(core::mem::size_of::<RadioConfig>(), 128);
        assert_eq!(core::mem::size_of::<ModelConfig>(), 128);
        assert_eq!(core::mem::size_of::<ModelConfigV4>(), 128);
        assert_eq!(core::mem::size_of::<RadioStorage>(), 2688);

        let m4 = ModelConfigV4 {
            name: *b"TEST MODEL",
            model_type: 1,
            _pad0: 0,
            rx_id: 0x1234_5678,
            trims: [1, -2, 3, -4],
            channel_reverse: 0b1010_1100,
            dr_switch: 2,
            thr_curve_pts: 5,
            thr_curve_smooth: 1,
            thr_curve: [0, 20, 40, 60, 80, 0, 0, 0, 0],
            dr_high: [100, 100, 100],
            dr_low: [70, 70, 70],
            expo_high: [10, 20, 30],
            expo_low: [5, 10, 15],
            timer_secs: 240,
            timer_source: 2,
            protocol_subtype: 2,
            failsafe_thr: 990,
            aux_channels: [7, 8, 5, 6, 9, 10, 1, 2, 3, 4],
            wing_tail_mix: 1,
            template_diff: -20,
            mixes: [MixLine::disabled(); 8],
            failsafe_mode: 1,
            failsafe_timeout: 30,
            rf_protocol: 1,
            crsf_baud: 2,
            arm_switch: 5,
            _reserved: [0; 11],
        };

        let mut m = ModelConfig::default_for_index(0);
        m.name = m4.name;
        m.model_type = m4.model_type;
        m.rx_id = m4.rx_id;
        m.trims = m4.trims;
        m.channel_reverse = m4.channel_reverse as u32;
        m.dr_switch = m4.dr_switch;
        m.thr_curve_pts = m4.thr_curve_pts;
        m.thr_curve_smooth = m4.thr_curve_smooth;
        m.thr_curve = m4.thr_curve;
        m.dr_high = m4.dr_high;
        m.dr_low = m4.dr_low;
        m.expo_high = m4.expo_high;
        m.expo_low = m4.expo_low;
        m.timer_secs = m4.timer_secs;
        m.timer_source = m4.timer_source;
        m.protocol_subtype = m4.protocol_subtype;
        m.failsafe_thr = m4.failsafe_thr;
        m.aux_channels[0..10].copy_from_slice(&m4.aux_channels);
        m.aux_channels[10..14].fill(0);
        m.wing_tail_mix = m4.wing_tail_mix;
        m.template_diff = m4.template_diff;
        m.mixes = m4.mixes;
        m.failsafe_mode = m4.failsafe_mode;
        m.failsafe_timeout = m4.failsafe_timeout;
        m.rf_protocol = m4.rf_protocol;
        m.crsf_baud = m4.crsf_baud;
        m.arm_switch = m4.arm_switch;

        assert_eq!(m.name, *b"TEST MODEL");
        assert_eq!(m.channel_reverse, 0b1010_1100u32);
        assert_eq!(m.aux_channels[0..10], [7, 8, 5, 6, 9, 10, 1, 2, 3, 4]);
        assert_eq!(m.aux_channels[10..14], [0, 0, 0, 0]);
        assert_eq!(m.rf_protocol, 1);
        assert_eq!(m.crsf_baud, 2);
    }
}
