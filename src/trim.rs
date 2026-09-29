//! Digital Trim controller for FlySky FS-i6X.
//!
//! Manages 4 trim axes (Roll, Pitch, Throttle, Yaw) mapped to the 8 trim rocker keys.
//! Provides single-press edge detection, auto-repeat when held, audio feedback via Buzzer,
//! and range limiting to -25..+25 steps (±100 µs authority).

use crate::boot;
use crate::buzzer::Buzzer;
use crate::mixer::{CHANNEL_MAX_US, CHANNEL_MIN_US, CHANNEL_SPAN_US};

pub const TRIM_MIN: i8 = -25;
pub const TRIM_MAX: i8 = 25;
pub const TRIM_STEP_US: i16 = 4; // 4 µs per step -> ±100 µs authority
pub const TRIM_MAX_OFFSET_US: i16 = TRIM_MAX as i16 * TRIM_STEP_US; // ±100 µs authority

/// Trim values for all 4 primary flight control channels.
#[derive(Copy, Clone, Debug, Default)]
pub struct TrimValues {
    pub roll: i8,     // CH1 (AIL)
    pub pitch: i8,    // CH2 (ELE)
    pub throttle: i8, // CH3 (THR)
    pub yaw: i8,      // CH4 (RUD)
}

/// Identifies which trim was last modified (for temporary UI callout).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ActiveTrim {
    None,
    Roll,
    Pitch,
    Throttle,
    Yaw,
}

pub struct TrimController {
    pub values: TrimValues,
    pub last_active: ActiveTrim,
    pub active_timer_ms: u16,
    pub throttle_enabled: bool,
    prev_trim_keys: u8,
    hold_time_ms: u16,
    repeat_time_ms: u16,
}

impl TrimController {
    pub const fn new() -> Self {
        Self {
            values: TrimValues {
                roll: 0,
                pitch: 0,
                throttle: 0,
                yaw: 0,
            },
            last_active: ActiveTrim::None,
            active_timer_ms: 0,
            throttle_enabled: false, // Option 2: locked/disabled for FC safety
            prev_trim_keys: 0,
            hold_time_ms: 0,
            repeat_time_ms: 0,
        }
    }

    /// Update trims based on raw 16-bit key matrix scan.
    /// Handles single clicks, auto-repeat, DFU lockout, and buzzer tones.
    pub fn update(&mut self, keys: u16, dt_ms: u16, buzzer: &mut Buzzer) {
        // Decrease active display timer
        if self.active_timer_ms > 0 {
            self.active_timer_ms = self.active_timer_ms.saturating_sub(dt_ms);
            if self.active_timer_ms == 0 {
                self.last_active = ActiveTrim::None;
            }
        }

        // Lock out trim processing if the DFU combination is pressed
        if boot::is_dfu_requested(keys) {
            self.prev_trim_keys = 0;
            self.hold_time_ms = 0;
            self.repeat_time_ms = 0;
            return;
        }

        // Trim keys occupy bits 0..7 of the key matrix
        let trim_keys = (keys & 0x00FF) as u8;

        if trim_keys == 0 {
            self.prev_trim_keys = 0;
            self.hold_time_ms = 0;
            self.repeat_time_ms = 0;
            return;
        }

        let mut trigger_key: Option<u8> = None;

        if trim_keys != self.prev_trim_keys {
            // New key press detected (edge trigger)
            let newly_pressed = trim_keys & !self.prev_trim_keys;
            if newly_pressed != 0 {
                // Pick lowest active bit
                let key_idx = newly_pressed.trailing_zeros() as u8;
                trigger_key = Some(key_idx);
            }
            self.prev_trim_keys = trim_keys;
            self.hold_time_ms = 0;
            self.repeat_time_ms = 0;
        } else {
            // Key held down -> auto-repeat
            self.hold_time_ms = self.hold_time_ms.saturating_add(dt_ms);
            if self.hold_time_ms >= 350 {
                self.repeat_time_ms = self.repeat_time_ms.saturating_add(dt_ms);
                if self.repeat_time_ms >= 90 {
                    self.repeat_time_ms = 0;
                    let key_idx = trim_keys.trailing_zeros() as u8;
                    trigger_key = Some(key_idx);
                }
            }
        }

        if let Some(key_bit) = trigger_key {
            self.process_trim_key(key_bit, buzzer);
        }
    }

    /// Process a single trim click for a given key index (0..7).
    fn process_trim_key(&mut self, key: u8, buzzer: &mut Buzzer) {
        let (axis, delta) = match key {
            0 => (ActiveTrim::Roll, 1),
            1 => (ActiveTrim::Roll, -1),
            2 => (ActiveTrim::Pitch, 1),
            3 => (ActiveTrim::Pitch, -1),
            4 => (ActiveTrim::Throttle, 1),
            5 => (ActiveTrim::Throttle, -1),
            6 => (ActiveTrim::Yaw, 1),
            7 => (ActiveTrim::Yaw, -1),
            _ => return,
        };
        self.step_trim(axis, delta, buzzer);
    }

    /// Step a specific trim axis by delta (+1 or -1) and produce appropriate audio.
    fn step_trim(&mut self, axis: ActiveTrim, delta: i8, buzzer: &mut Buzzer) {
        if axis == ActiveTrim::Throttle && !self.throttle_enabled {
            buzzer.trim_limit();
            return; // Throttle trim locked/disabled
        }

        let val_ref = match axis {
            ActiveTrim::Roll => &mut self.values.roll,
            ActiveTrim::Pitch => &mut self.values.pitch,
            ActiveTrim::Throttle => &mut self.values.throttle,
            ActiveTrim::Yaw => &mut self.values.yaw,
            ActiveTrim::None => return,
        };

        let curr = *val_ref;
        if delta > 0 && curr >= TRIM_MAX {
            buzzer.trim_limit();
            return;
        }
        if delta < 0 && curr <= TRIM_MIN {
            buzzer.trim_limit();
            return;
        }

        let new_val = (curr + delta).clamp(TRIM_MIN, TRIM_MAX);
        *val_ref = new_val;

        self.last_active = axis;
        self.active_timer_ms = 1500; // Show trim overlay on UI for 1.5 seconds

        if new_val == 0 {
            buzzer.trim_center();
        } else {
            buzzer.trim_step(new_val);
        }
    }

    /// Apply trim offset to raw channel pulse width (CHANNEL_MIN_US..CHANNEL_MAX_US).
    #[inline(always)]
    pub fn apply(base_us: u16, trim: i8) -> u16 {
        let offset = trim as i16 * TRIM_STEP_US;
        (base_us as i16 + offset).clamp(CHANNEL_MIN_US as i16, CHANNEL_MAX_US as i16) as u16
    }

    /// Apply throttle trim based on mode:
    /// - 0: OFF (Lock) -> returns base_us unaltered
    /// - 1: IDLE (T-Trim) -> trim authority maximum at idle (CHANNEL_MIN_US), tapering to 0 at full stick (CHANNEL_MAX_US)
    /// - 2: LINEAR -> uniform trim offset across full range (±100 µs)
    #[inline(always)]
    pub fn apply_throttle(base_us: u16, trim: i8, mode: u8) -> u16 {
        match mode {
            1 => {
                let max_offset = trim as i32 * TRIM_STEP_US as i32; // -100..+100 µs
                let stick_travel = (base_us as i32 - CHANNEL_MIN_US as i32).clamp(0, CHANNEL_SPAN_US as i32);
                // At stick_travel = 0 (CHANNEL_MIN_US), factor is 1024 / 1024 = 1.0 -> full offset
                // At stick_travel = CHANNEL_SPAN_US (CHANNEL_MAX_US), factor is 0 / 1024 = 0.0 -> zero offset
                let effective_offset = (max_offset * (CHANNEL_SPAN_US as i32 - stick_travel)) / CHANNEL_SPAN_US as i32;
                let min_clamp = (CHANNEL_MIN_US as i32) - (TRIM_MAX_OFFSET_US as i32);
                let max_clamp = (CHANNEL_MAX_US as i32) + (TRIM_MAX_OFFSET_US as i32);
                (base_us as i32 + effective_offset).clamp(min_clamp, max_clamp) as u16
            }
            2 => Self::apply(base_us, trim),
            _ => base_us.clamp(CHANNEL_MIN_US, CHANNEL_MAX_US),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trim_apply_linear() {
        assert_eq!(TrimController::apply(1500, 0), 1500);
        assert_eq!(TrimController::apply(1500, 10), 1540);
        assert_eq!(TrimController::apply(1500, -25), 1400);
        assert_eq!(TrimController::apply(1500, 25), 1600);
        // Clamping to channel bounds
        assert_eq!(TrimController::apply(990, -25), CHANNEL_MIN_US);
        assert_eq!(TrimController::apply(2010, 25), CHANNEL_MAX_US);
    }

    #[test]
    fn test_apply_throttle_modes() {
        // Mode 0: OFF
        assert_eq!(TrimController::apply_throttle(988, 25, 0), 988);
        assert_eq!(TrimController::apply_throttle(1500, 25, 0), 1500);

        // Mode 1: IDLE trim (tapers from full authority at 988 to 0 at 2012)
        assert_eq!(TrimController::apply_throttle(CHANNEL_MIN_US, 25, 1), CHANNEL_MIN_US + 100);
        assert_eq!(TrimController::apply_throttle(CHANNEL_MAX_US, 25, 1), CHANNEL_MAX_US);

        // Mid-stick: roughly 50% authority
        let mid = (CHANNEL_MIN_US + CHANNEL_MAX_US) / 2; // 1500
        let mid_val = TrimController::apply_throttle(mid, 25, 1);
        assert!(mid_val >= 1545 && mid_val <= 1555, "Mid-stick idle trim should be ~50 µs offset, got {}", mid_val);

        // Mode 2: LINEAR
        assert_eq!(TrimController::apply_throttle(1500, 10, 2), 1540);
    }

    #[test]
    fn test_trim_controller_defaults() {
        let trim = TrimController::new();
        assert_eq!(trim.values.roll, 0);
        assert_eq!(trim.values.pitch, 0);
        assert_eq!(trim.values.throttle, 0);
        assert_eq!(trim.values.yaw, 0);
        assert_eq!(trim.last_active, ActiveTrim::None);
    }
}
