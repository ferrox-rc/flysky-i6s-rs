//! Variometer (Vario) calculation and acoustic state engine.
//!
//! Provides non-blocking vertical speed filtering, deadband gating, motor cutoff muting,
//! and tone frequency/cadence synthesis for glider thermaling and FPV soaring.

#![allow(dead_code)]

/// Vario sink tone behavior when descending below the sink deadband threshold.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum VarioSinkMode {
    /// Silent when sinking (only beep on lift).
    Mute = 0,
    /// Slow low-frequency pulsed beeps.
    Pulse = 1,
    /// Continuous low drone tone.
    Continuous = 2,
}

impl VarioSinkMode {
    pub const fn from_u8(val: u8) -> Self {
        match val {
            1 => VarioSinkMode::Pulse,
            2 => VarioSinkMode::Continuous,
            _ => VarioSinkMode::Mute,
        }
    }
}

/// Model-specific variometer configuration.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct VarioConfig {
    /// Activation switch condition index (0: Disabled, 1..=14: SA^..SFv).
    pub switch: u8,
    /// Climb deadband threshold in 0.1 m/s (10 cm/s) units (e.g. 2 = +0.2 m/s).
    pub climb_deadband_dm_s: u8,
    /// Sink deadband threshold in 0.1 m/s (10 cm/s) units (e.g. 5 = -0.5 m/s).
    pub sink_deadband_dm_s: u8,
    /// Sink tone style (Mute, Pulse, Continuous).
    pub sink_mode: VarioSinkMode,
    /// Mute vario audio while the motor/throttle is active (> 10% throttle).
    pub motor_mute: bool,
}

impl Default for VarioConfig {
    fn default() -> Self {
        Self {
            switch: 0,
            climb_deadband_dm_s: 2, // +0.2 m/s
            sink_deadband_dm_s: 5,  // -0.5 m/s
            sink_mode: VarioSinkMode::Pulse,
            motor_mute: true,
        }
    }
}

/// Synthesized acoustic tone parameters emitted by the vario engine.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct VarioTone {
    /// Audio frequency in Hz (0 = silent).
    pub freq_hz: u16,
    /// Duration of audio pulse in milliseconds.
    pub pulse_ms: u16,
    /// Duration of silence pause between pulses in milliseconds (0 = continuous).
    pub pause_ms: u16,
}

impl VarioTone {
    pub const SILENT: Self = Self {
        freq_hz: 0,
        pulse_ms: 0,
        pause_ms: 0,
    };

    #[inline(always)]
    pub fn is_silent(&self) -> bool {
        self.freq_hz < 100 || self.pulse_ms == 0
    }
}

/// Variometer runtime state tracking and tone generator.
pub struct Vario {
    /// Filtered vertical speed in cm/s (+ climb, - sink).
    pub vspeed_cms: i16,
    /// Current barometric altitude in meters.
    pub alt_m: i16,
    last_alt_m: i16,
    last_alt_ms: u32,
    initialized: bool,
}

impl Default for Vario {
    fn default() -> Self {
        Self::new()
    }
}

impl Vario {
    pub const fn new() -> Self {
        Self {
            vspeed_cms: 0,
            alt_m: 0,
            last_alt_m: 0,
            last_alt_ms: 0,
            initialized: false,
        }
    }

    /// Reset state upon model switch or link disconnect.
    pub fn reset(&mut self) {
        self.vspeed_cms = 0;
        self.alt_m = 0;
        self.last_alt_m = 0;
        self.last_alt_ms = 0;
        self.initialized = false;
    }

    /// Update sensor metrics and calculate current acoustic tone output.
    ///
    /// - `now_ms`: Monotonic system tick.
    /// - `raw_alt_m`: Current altitude in meters from telemetry.
    /// - `raw_vspeed_cms`: Direct vertical speed in cm/s from telemetry (0 if unavailable).
    /// - `telem_connected`: True if active RF downlink packets are arriving.
    /// - `switch_active`: True if the configured activation switch condition is met.
    /// - `throttle_us`: Current throttle channel pulse in microseconds (1000..2000).
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        now_ms: u32,
        raw_alt_m: i16,
        raw_vspeed_cms: i16,
        telem_connected: bool,
        switch_active: bool,
        throttle_us: u16,
        config: &VarioConfig,
    ) -> VarioTone {
        if !telem_connected {
            self.reset();
            return VarioTone::SILENT;
        }

        // Initialize baseline altitude reference
        if !self.initialized {
            self.last_alt_m = raw_alt_m;
            self.last_alt_ms = now_ms;
            self.alt_m = raw_alt_m;
            self.vspeed_cms = raw_vspeed_cms;
            self.initialized = true;
        } else {
            self.alt_m = raw_alt_m;
        }

        // Process vertical speed
        if raw_vspeed_cms != 0 {
            // Direct telemetry sensor provided vspeed (CRSF 0x07 or i-BUS 0x09)
            // Apply 4-tap Exponential Moving Average (EMA) filter
            self.vspeed_cms = ((self.vspeed_cms as i32 * 3 + raw_vspeed_cms as i32) / 4) as i16;
            self.last_alt_m = raw_alt_m;
            self.last_alt_ms = now_ms;
        } else if raw_alt_m != self.last_alt_m && now_ms > self.last_alt_ms {
            let dt_ms = now_ms.saturating_sub(self.last_alt_ms);
            // Differentiate rate of climb when only altitude is transmitted (dt >= 80ms)
            if dt_ms >= 80 {
                let dh_m = (raw_alt_m as i32) - (self.last_alt_m as i32);
                let derived_cms = (dh_m * 100_000) / (dt_ms as i32);
                self.vspeed_cms = ((self.vspeed_cms as i32 * 3 + derived_cms) / 4) as i16;
                self.last_alt_m = raw_alt_m;
                self.last_alt_ms = now_ms;
            }
        }

        // Check activation switch
        if !switch_active {
            return VarioTone::SILENT;
        }

        // Motor/Throttle lockout check (> 10% throttle = > 1100 µs pulse)
        if config.motor_mute && throttle_us > 1100 {
            return VarioTone::SILENT;
        }

        self.compute_tone(config)
    }

    /// Calculate tone frequency and cadence based on current vertical speed and deadband thresholds.
    fn compute_tone(&self, config: &VarioConfig) -> VarioTone {
        let climb_threshold = (config.climb_deadband_dm_s as i16) * 10;
        let sink_threshold = (config.sink_deadband_dm_s as i16) * 10;

        if self.vspeed_cms >= climb_threshold {
            // --- CLIMB (LIFT) ---
            // Delta from climb deadband up to +500 cm/s (+5.0 m/s)
            let delta = (self.vspeed_cms - climb_threshold).clamp(0, 500) as u32;

            // Frequency scales smoothly from 1000 Hz at threshold up to 2800 Hz at +5 m/s
            let freq_hz = (1000 + (delta * 1800) / 500) as u16;

            // Repetition cadence scales from 550 ms down to 110 ms
            let period_ms = (550 - (delta * 440) / 500) as u16;
            let pulse_ms = period_ms / 2;
            let pause_ms = period_ms - pulse_ms;

            VarioTone {
                freq_hz,
                pulse_ms,
                pause_ms,
            }
        } else if self.vspeed_cms <= -sink_threshold {
            // --- SINK (DOWNDRAFT) ---
            match config.sink_mode {
                VarioSinkMode::Mute => VarioTone::SILENT,
                VarioSinkMode::Pulse => {
                    let sink_mag = (-self.vspeed_cms - sink_threshold).clamp(0, 500) as u32;
                    let freq_hz = (450 - (sink_mag * 100) / 500) as u16;
                    VarioTone {
                        freq_hz,
                        pulse_ms: 160,
                        pause_ms: 360,
                    }
                }
                VarioSinkMode::Continuous => {
                    let sink_mag = (-self.vspeed_cms - sink_threshold).clamp(0, 500) as u32;
                    let freq_hz = (450 - (sink_mag * 100) / 500) as u16;
                    VarioTone {
                        freq_hz,
                        pulse_ms: 1000,
                        pause_ms: 0,
                    }
                }
            }
        } else {
            // In deadband window
            VarioTone::SILENT
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vario_climb_pitch_and_cadence() {
        let mut vario = Vario::new();
        let config = VarioConfig {
            switch: 1,
            climb_deadband_dm_s: 2, // +20 cm/s
            sink_deadband_dm_s: 5,  // -50 cm/s
            sink_mode: VarioSinkMode::Pulse,
            motor_mute: true,
        };

        // Weak lift: +50 cm/s
        let tone_weak = vario.update(1000, 100, 50, true, true, 1000, &config);
        assert!(!tone_weak.is_silent());
        assert!(tone_weak.freq_hz >= 1000 && tone_weak.freq_hz <= 1300);
        assert!(tone_weak.pulse_ms >= 200);

        // Strong lift: converge to +500 cm/s over multiple updates
        let mut tone_strong = tone_weak;
        for i in 1..=16 {
            tone_strong = vario.update(1000 + i * 50, 100 + i as i16, 500, true, true, 1000, &config);
        }
        assert!(!tone_strong.is_silent());
        assert!(tone_strong.freq_hz >= 2500);
        assert!(tone_strong.pulse_ms <= 80);
        assert!(tone_strong.freq_hz > tone_weak.freq_hz);
        assert!(tone_strong.pulse_ms < tone_weak.pulse_ms);
    }

    #[test]
    fn test_vario_deadband_silence() {
        let mut vario = Vario::new();
        let config = VarioConfig {
            switch: 1,
            climb_deadband_dm_s: 2, // +20 cm/s
            sink_deadband_dm_s: 5,  // -50 cm/s
            sink_mode: VarioSinkMode::Pulse,
            motor_mute: true,
        };

        // Level cruising: +10 cm/s (inside deadband)
        let tone = vario.update(1000, 100, 10, true, true, 1000, &config);
        assert!(tone.is_silent());

        // Mild sink: -30 cm/s (inside deadband)
        let tone = vario.update(1100, 100, -30, true, true, 1000, &config);
        assert!(tone.is_silent());
    }

    #[test]
    fn test_vario_sink_modes() {
        let mut vario = Vario::new();
        let mut config = VarioConfig {
            switch: 1,
            climb_deadband_dm_s: 2,
            sink_deadband_dm_s: 5, // -50 cm/s
            sink_mode: VarioSinkMode::Mute,
            motor_mute: true,
        };

        // Heavy sink with Mute mode
        let tone_mute = vario.update(1000, 100, -150, true, true, 1000, &config);
        assert!(tone_mute.is_silent());

        // Heavy sink with Pulse mode
        config.sink_mode = VarioSinkMode::Pulse;
        let tone_pulse = vario.update(1100, 95, -150, true, true, 1000, &config);
        assert!(!tone_pulse.is_silent());
        assert_eq!(tone_pulse.pulse_ms, 160);
        assert_eq!(tone_pulse.pause_ms, 360);
        assert!(tone_pulse.freq_hz <= 450);

        // Heavy sink with Continuous mode
        config.sink_mode = VarioSinkMode::Continuous;
        let tone_cont = vario.update(1200, 90, -150, true, true, 1000, &config);
        assert!(!tone_cont.is_silent());
        assert_eq!(tone_cont.pause_ms, 0);
    }

    #[test]
    fn test_vario_throttle_lockout() {
        let mut vario = Vario::new();
        let config = VarioConfig {
            switch: 1,
            climb_deadband_dm_s: 2,
            sink_deadband_dm_s: 5,
            sink_mode: VarioSinkMode::Pulse,
            motor_mute: true,
        };

        // Motor climb (throttle at 1500 µs = 50%) -> MUST BE SILENT
        let tone = vario.update(1000, 100, 300, true, true, 1500, &config);
        assert!(tone.is_silent());

        // Gliding climb (throttle at 1000 µs = 0%) -> MUST BE AUDIBLE
        let tone = vario.update(1100, 105, 300, true, true, 1000, &config);
        assert!(!tone.is_silent());
    }

    #[test]
    fn test_vario_derivative_from_altitude() {
        let mut vario = Vario::new();
        let config = VarioConfig {
            switch: 1,
            climb_deadband_dm_s: 2,
            sink_deadband_dm_s: 5,
            sink_mode: VarioSinkMode::Pulse,
            motor_mute: false,
        };

        // Initialize at 100m at t=0
        vario.update(0, 100, 0, true, true, 1000, &config);

        // Climb to 102m after 1000ms (+2.0 m/s = +200 cm/s)
        let tone = vario.update(1000, 102, 0, true, true, 1000, &config);
        assert!(!tone.is_silent());
        assert!(vario.vspeed_cms > 40);
    }
}
