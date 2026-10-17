//! Safety and preflight checks for flysky-i6x-rs.
//! Guarantees all checks operate strictly on normalized input channels.

use crate::storage::ChannelCalib;

/// Model type definition.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ModelType {
    /// Standard fixed-wing airplane: unipolar throttle (0% at stick bottom).
    Airplane = 0,
    /// Unpowered / powered glider: unipolar throttle (0% at stick bottom).
    Glider = 1,
    /// Helicopter: throttle managed via collective pitch & flight mode / throttle hold.
    Heli = 2,
    /// Multirotor / Quadcopter: unipolar throttle (0% at stick bottom).
    Quad = 3,
    /// General surface vehicle (cars/boats/rovers): spring-centered bidirectional throttle.
    General = 4,
}

impl ModelType {
    pub const fn from_u8(val: u8) -> Self {
        match val {
            0 => Self::Airplane,
            1 => Self::Glider,
            2 => Self::Heli,
            3 => Self::Quad,
            _ => Self::General,
        }
    }

    pub const fn to_u8(self) -> u8 {
        self as u8
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Airplane => "AIRPLANE",
            Self::Glider => "GLIDER",
            Self::Heli => "HELICOPTER",
            Self::Quad => "MULTI / QUAD",
            Self::General => "GENERAL",
        }
    }

    pub const fn next(self) -> Self {
        match self {
            Self::Airplane => Self::Glider,
            Self::Glider => Self::Heli,
            Self::Heli => Self::Quad,
            Self::Quad => Self::General,
            Self::General => Self::Airplane,
        }
    }

    #[inline(always)]
    pub const fn is_general(self) -> bool {
        matches!(self, Self::General)
    }
}

/// Radio stick mode configuration.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum StickMode {
    Mode1 = 0, // Throttle: Right vertical
    Mode2 = 1, // Throttle: Left vertical (most common / default)
    Mode3 = 2, // Throttle: Right vertical
    Mode4 = 3, // Throttle: Left vertical
}

impl StickMode {
    pub const fn from_u8(val: u8) -> Self {
        match val {
            0 => Self::Mode1,
            1 => Self::Mode2,
            2 => Self::Mode3,
            3 => Self::Mode4,
            _ => Self::Mode2,
        }
    }

    pub const fn to_u8(self) -> u8 {
        self as u8
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Mode1 => "MODE 1",
            Self::Mode2 => "MODE 2",
            Self::Mode3 => "MODE 3",
            Self::Mode4 => "MODE 4",
        }
    }

    /// Maps the logical throttle function to the correct physical analog axis.
    pub const fn throttle_channel(self) -> AnalogChannel {
        match self {
            StickMode::Mode2 | StickMode::Mode4 => AnalogChannel::LeftVertical,
            StickMode::Mode1 | StickMode::Mode3 => AnalogChannel::RightVertical,
        }
    }

    /// Maps the logical pitch / elevator function to the physical analog axis.
    pub const fn pitch_channel(self) -> AnalogChannel {
        match self {
            StickMode::Mode1 | StickMode::Mode3 => AnalogChannel::LeftVertical,
            StickMode::Mode2 | StickMode::Mode4 => AnalogChannel::RightVertical,
        }
    }

    /// Maps the logical roll / aileron function to the physical analog axis.
    pub const fn roll_channel(self) -> AnalogChannel {
        match self {
            StickMode::Mode1 | StickMode::Mode2 => AnalogChannel::RightHorizontal,
            StickMode::Mode3 | StickMode::Mode4 => AnalogChannel::LeftHorizontal,
        }
    }

    /// Maps the logical yaw / rudder function to the physical analog axis.
    pub const fn yaw_channel(self) -> AnalogChannel {
        match self {
            StickMode::Mode1 | StickMode::Mode2 => AnalogChannel::LeftHorizontal,
            StickMode::Mode3 | StickMode::Mode4 => AnalogChannel::RightHorizontal,
        }
    }
}

/// Physical analog stick indices from the 11-channel DMA ADC scan on STM32F072.
/// - PA0: Right Horizontal (RH) -> index 0
/// - PA1: Right Vertical (RV)   -> index 1
/// - PA2: Left Vertical (LV)    -> index 2
/// - PA3: Left Horizontal (LH)  -> index 3
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AnalogChannel {
    RightHorizontal = 0,
    RightVertical = 1,
    LeftVertical = 2,
    LeftHorizontal = 3,
}

impl AnalogChannel {
    #[inline(always)]
    pub const fn raw_adc_index(self) -> usize {
        self as usize
    }
}

/// Calibration limits for a 12-bit ADC channel (0..4095 counts).
#[derive(Copy, Clone, Debug)]
pub struct ChannelCalibration {
    pub min: u16,
    pub center: u16,
    pub max: u16,
}

impl ChannelCalibration {
    /// Nominal factory default bounds when flash calibration data is uninitialized.
    /// Provides safe, guaranteed normalization without division-by-zero.
    pub const DEFAULT: Self = Self {
        min: 800,
        center: 2048,
        max: 3300,
    };

    /// Construct from stored ChannelCalib, falling back to nominal defaults if uninitialized.
    pub fn from_channel_calib(calib: &ChannelCalib) -> Self {
        if calib.min > 200 && calib.max > calib.center && calib.center > calib.min {
            Self {
                min: calib.min,
                center: calib.center,
                max: calib.max,
            }
        } else {
            Self::DEFAULT
        }
    }

    /// Normalizes raw 12-bit ADC counts to a signed permille scale (-1000..=1000).
    /// Uses integer math suitable for Cortex-M0 (zero FPU instructions).
    pub fn normalize_bipolar(&self, raw: u16) -> i16 {
        let raw = raw.clamp(self.min, self.max);
        if raw >= self.center {
            let span = (self.max - self.center).max(1) as i32;
            let offset = (raw - self.center) as i32;
            ((offset * 1000) / span).clamp(0, 1000) as i16
        } else {
            let span = (self.center - self.min).max(1) as i32;
            let offset = (self.center - raw) as i32;
            (-((offset * 1000) / span)).clamp(-1000, 0) as i16
        }
    }

    /// Normalizes raw 12-bit ADC counts to a unipolar scale (0..=1000, 0%..=100%).
    pub fn normalize_unipolar(&self, raw: u16) -> u16 {
        let raw = raw.clamp(self.min, self.max);
        let span = (self.max - self.min).max(1) as u32;
        let offset = (raw - self.min) as u32;
        ((offset * 1000) / span).min(1000) as u16
    }
}

/// Preflight check result.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PreflightError {
    ThrottleHigh { current_pct: u8, threshold_pct: u8 },
    ThrottleNotCentered { offset_pct: i8 },
    ThrottleHoldDisengaged,
}

/// Evaluates throttle safety during the preflight startup sequence.
///
/// Operates exclusively on normalized values regardless of whether the gimbals
/// have stored EEPROM calibration data or are using nominal defaults.
pub fn check_preflight_throttle(
    raw_adc_values: &[u16; 4],
    calibration: Option<&[ChannelCalibration; 4]>,
    stick_mode: StickMode,
    model_type: ModelType,
    throttle_hold_active: bool,
) -> Result<(), PreflightError> {
    // 1. Resolve physical throttle channel from stick mode
    let channel = stick_mode.throttle_channel();
    let channel_idx = channel.raw_adc_index();

    let raw = raw_adc_values[channel_idx];
    let cal = calibration
        .map(|c| &c[channel_idx])
        .unwrap_or(&ChannelCalibration::DEFAULT);

    // 2. Evaluate safety rule according to ModelType
    match model_type {
        ModelType::Airplane | ModelType::Glider | ModelType::Quad => {
            // Unipolar scale: 0..=1000 permille (0% to 100%)
            // 2% threshold = 20 permille
            const THROTTLE_PERMILLE_THRESHOLD: u16 = 20;

            let normalized = cal.normalize_unipolar(raw);
            if normalized > THROTTLE_PERMILLE_THRESHOLD {
                return Err(PreflightError::ThrottleHigh {
                    current_pct: (normalized / 10) as u8,
                    threshold_pct: (THROTTLE_PERMILLE_THRESHOLD / 10) as u8,
                });
            }
        }

        ModelType::General => {
            // Surface / rover models use spring-centered gimbals (-1000..=1000)
            // Neutral check: stick must rest within ±2% (±20 permille) of center
            const CENTER_PERMILLE_THRESHOLD: i16 = 20;

            let normalized = cal.normalize_bipolar(raw);
            if normalized.abs() > CENTER_PERMILLE_THRESHOLD {
                return Err(PreflightError::ThrottleNotCentered {
                    offset_pct: (normalized / 10) as i8,
                });
            }
        }

        ModelType::Heli => {
            // Helicopters utilize collective pitch / throttle curves.
            // Safety requires Throttle Hold/Cut switch active on startup.
            if !throttle_hold_active {
                return Err(PreflightError::ThrottleHoldDisengaged);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stick_mode_channel_mapping() {
        // Mode 2 and Mode 4 have Throttle on LeftVertical (PA2 / index 2)
        assert_eq!(StickMode::Mode2.throttle_channel(), AnalogChannel::LeftVertical);
        assert_eq!(StickMode::Mode4.throttle_channel(), AnalogChannel::LeftVertical);
        assert_eq!(AnalogChannel::LeftVertical.raw_adc_index(), 2);

        // Mode 1 and Mode 3 have Throttle on RightVertical (PA1 / index 1)
        assert_eq!(StickMode::Mode1.throttle_channel(), AnalogChannel::RightVertical);
        assert_eq!(StickMode::Mode3.throttle_channel(), AnalogChannel::RightVertical);
        assert_eq!(AnalogChannel::RightVertical.raw_adc_index(), 1);
    }

    #[test]
    fn test_channel_calibration_normalization() {
        let cal = ChannelCalibration {
            min: 1000,
            center: 2000,
            max: 3000,
        };

        // Bipolar: -1000 at min, 0 at center, +1000 at max
        assert_eq!(cal.normalize_bipolar(1000), -1000);
        assert_eq!(cal.normalize_bipolar(2000), 0);
        assert_eq!(cal.normalize_bipolar(3000), 1000);
        assert_eq!(cal.normalize_bipolar(2010), 10);
        assert_eq!(cal.normalize_bipolar(1990), -10);

        // Unipolar: 0 at min, 500 at center, 1000 at max
        assert_eq!(cal.normalize_unipolar(1000), 0);
        assert_eq!(cal.normalize_unipolar(2000), 500);
        assert_eq!(cal.normalize_unipolar(3000), 1000);
        assert_eq!(cal.normalize_unipolar(1020), 10);
        assert_eq!(cal.normalize_unipolar(1040), 20);
    }

    #[test]
    fn test_preflight_check_airplane_mode2() {
        let cal = [
            ChannelCalibration::DEFAULT,
            ChannelCalibration::DEFAULT,
            ChannelCalibration {
                min: 1000,
                center: 2000,
                max: 3000,
            },
            ChannelCalibration::DEFAULT,
        ];

        // Raw ADC at 1020 (1% unipolar throttle = 10 permille <= 20) -> Safe
        let raw = [2048, 2048, 1020, 2048];
        assert_eq!(
            check_preflight_throttle(&raw, Some(&cal), StickMode::Mode2, ModelType::Airplane, false),
            Ok(())
        );

        // Raw ADC at 1040 (2% unipolar throttle = 20 permille <= 20) -> Safe
        let raw = [2048, 2048, 1040, 2048];
        assert_eq!(
            check_preflight_throttle(&raw, Some(&cal), StickMode::Mode2, ModelType::Airplane, false),
            Ok(())
        );

        // Raw ADC at 1060 (3% unipolar throttle = 30 permille > 20) -> Error ThrottleHigh
        let raw = [2048, 2048, 1060, 2048];
        assert_eq!(
            check_preflight_throttle(&raw, Some(&cal), StickMode::Mode2, ModelType::Airplane, false),
            Err(PreflightError::ThrottleHigh {
                current_pct: 3,
                threshold_pct: 2,
            })
        );
    }

    #[test]
    fn test_preflight_check_mode1_throttle_on_right_vertical() {
        let cal = [
            ChannelCalibration::DEFAULT,
            ChannelCalibration {
                min: 1000,
                center: 2000,
                max: 3000,
            },
            ChannelCalibration::DEFAULT,
            ChannelCalibration::DEFAULT,
        ];

        // Mode 1: index 1 is RightVertical (Throttle)
        // High throttle at index 1 -> Error
        let raw = [2048, 1100, 1000, 2048];
        assert_eq!(
            check_preflight_throttle(&raw, Some(&cal), StickMode::Mode1, ModelType::Quad, false),
            Err(PreflightError::ThrottleHigh {
                current_pct: 5,
                threshold_pct: 2,
            })
        );

        // Low throttle at index 1 -> Safe
        let raw = [2048, 1010, 1000, 2048];
        assert_eq!(
            check_preflight_throttle(&raw, Some(&cal), StickMode::Mode1, ModelType::Quad, false),
            Ok(())
        );
    }

    #[test]
    fn test_preflight_check_general_surface_centered() {
        let cal = [
            ChannelCalibration::DEFAULT,
            ChannelCalibration::DEFAULT,
            ChannelCalibration {
                min: 1000,
                center: 2000,
                max: 3000,
            },
            ChannelCalibration::DEFAULT,
        ];

        // Centered (raw=2000, offset=0 permille) -> Safe
        let raw = [2048, 2048, 2000, 2048];
        assert_eq!(
            check_preflight_throttle(&raw, Some(&cal), StickMode::Mode2, ModelType::General, false),
            Ok(())
        );

        // Within ±2% (raw=2015, offset=15 permille) -> Safe
        let raw = [2048, 2048, 2015, 2048];
        assert_eq!(
            check_preflight_throttle(&raw, Some(&cal), StickMode::Mode2, ModelType::General, false),
            Ok(())
        );

        // Outside neutral (raw=2050, offset=50 permille / 5%) -> Error ThrottleNotCentered
        let raw = [2048, 2048, 2050, 2048];
        assert_eq!(
            check_preflight_throttle(&raw, Some(&cal), StickMode::Mode2, ModelType::General, false),
            Err(PreflightError::ThrottleNotCentered { offset_pct: 5 })
        );

        // Full reverse / bottom (raw=1000, offset=-1000 permille) -> Error ThrottleNotCentered
        let raw = [2048, 2048, 1000, 2048];
        assert_eq!(
            check_preflight_throttle(&raw, Some(&cal), StickMode::Mode2, ModelType::General, false),
            Err(PreflightError::ThrottleNotCentered { offset_pct: -100 })
        );
    }

    #[test]
    fn test_preflight_check_heli_throttle_hold() {
        let raw = [2048, 2048, 2048, 2048];

        // Throttle hold active -> Ok
        assert_eq!(
            check_preflight_throttle(&raw, None, StickMode::Mode2, ModelType::Heli, true),
            Ok(())
        );

        // Throttle hold disengaged -> Error
        assert_eq!(
            check_preflight_throttle(&raw, None, StickMode::Mode2, ModelType::Heli, false),
            Err(PreflightError::ThrottleHoldDisengaged)
        );
    }

    #[test]
    fn test_preflight_check_uncalibrated_uses_defaults() {
        // Without calibration provided, uses ChannelCalibration::DEFAULT
        // DEFAULT has min:800, center:2048, max:3300. Span = 2500.
        // raw = 825 -> offset = 25 -> 25 * 1000 / 2500 = 10 permille (1%) <= 20 -> Ok
        let raw = [2048, 2048, 825, 2048];
        assert_eq!(
            check_preflight_throttle(&raw, None, StickMode::Mode2, ModelType::Airplane, false),
            Ok(())
        );

        // raw = 900 -> offset = 100 -> 100 * 1000 / 2500 = 40 permille (4%) > 20 -> Error
        let raw = [2048, 2048, 900, 2048];
        assert_eq!(
            check_preflight_throttle(&raw, None, StickMode::Mode2, ModelType::Airplane, false),
            Err(PreflightError::ThrottleHigh {
                current_pct: 4,
                threshold_pct: 2,
            })
        );
    }
}
