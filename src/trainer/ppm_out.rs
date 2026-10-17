//! Pulse Position Modulation (PPM) output generator on PF10 (Rear Trainer Port).
//!
//! Generates a deterministic 22.5 ms (22,500 µs) CPPM pulse train:
//! - 8 Channels: 1000..2000 µs (center 1500 µs, clamped 900..2100 µs).
//! - Sync pulse: 400 µs LOW.
//! - Channel pulse: (pulse_width - 400 µs) HIGH.
//! - Sync gap: remaining frame duration HIGH (typically ~9,000..14,000 µs).
//!
//! Designed to power external RF modules (e.g. iRangeX iRX6 multi-protocol module)
//! and standard USB simulator dongles via the rear mini-DIN trainer port.

pub const PPM_CHANNELS: usize = 8;
pub const PPM_FRAME_TOTAL_US: u32 = 22_500;
pub const PPM_SYNC_PULSE_US: u16 = 400;

/// Pure, deterministic PPM pulse train generator state machine.
#[derive(Copy, Clone, Debug)]
pub struct PpmGenerator {
    pub channels: [u16; PPM_CHANNELS],
    current_ch: usize,
    in_sync_pulse: bool,
    frame_elapsed_us: u32,
}

impl Default for PpmGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl PpmGenerator {
    pub const fn new() -> Self {
        Self {
            channels: [1500; PPM_CHANNELS],
            current_ch: 0,
            in_sync_pulse: true,
            frame_elapsed_us: 0,
        }
    }

    /// Update channel pulse widths (clamped to safe PPM bounds 900..2100 µs).
    pub fn update_channels(&mut self, chs: &[u16]) {
        for (dst, &src) in self.channels.iter_mut().zip(chs.iter()) {
            *dst = src.clamp(900, 2100);
        }
    }

    /// Reset generation state to start of frame.
    pub fn reset_frame(&mut self) {
        self.current_ch = 0;
        self.in_sync_pulse = true;
        self.frame_elapsed_us = 0;
    }

    /// Calculate next timer duration in microseconds and the pin state.
    ///
    /// Returns `(duration_us, pin_high)`:
    /// - `pin_high == false`: line is driven LOW (active sync pulse).
    /// - `pin_high == true`: line is driven HIGH (channel high time or sync gap).
    pub fn next_interval(&mut self) -> (u16, bool) {
        if self.current_ch < PPM_CHANNELS {
            if self.in_sync_pulse {
                // Active sync pulse: 400 µs LOW
                self.in_sync_pulse = false;
                self.frame_elapsed_us += PPM_SYNC_PULSE_US as u32;
                (PPM_SYNC_PULSE_US, false)
            } else {
                // Channel high period: (channel_pulse - sync_pulse)
                let ch_pulse = self.channels[self.current_ch];
                let high_time = ch_pulse.saturating_sub(PPM_SYNC_PULSE_US).max(500);
                self.in_sync_pulse = true;
                self.current_ch += 1;
                self.frame_elapsed_us += high_time as u32;
                (high_time, true)
            }
        } else {
            // End of frame: Final sync pulse followed by sync gap
            if self.in_sync_pulse {
                self.in_sync_pulse = false;
                self.frame_elapsed_us += PPM_SYNC_PULSE_US as u32;
                (PPM_SYNC_PULSE_US, false)
            } else {
                // Long sync gap: fills remaining frame duration up to 22,500 µs
                let gap = PPM_FRAME_TOTAL_US.saturating_sub(self.frame_elapsed_us);
                self.current_ch = 0;
                self.in_sync_pulse = true;
                self.frame_elapsed_us = 0;
                let gap_us = (gap as u16).max(3000);
                (gap_us, true)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ppm_generator_frame_timing_standard() {
        let mut gen = PpmGenerator::new();
        let test_chs = [1000, 1500, 2000, 1500, 1200, 1800, 1000, 2000];
        gen.update_channels(&test_chs);

        let mut total_duration = 0u32;
        // 8 channels * 2 intervals (sync LOW + channel HIGH) = 16 intervals
        // plus final sync LOW + sync gap HIGH = 2 intervals -> total 18 intervals
        for _ in 0..18 {
            let (duration, _high) = gen.next_interval();
            total_duration += duration as u32;
        }

        assert_eq!(total_duration, PPM_FRAME_TOTAL_US);
    }

    #[test]
    fn test_ppm_generator_all_min_and_all_max() {
        let mut gen = PpmGenerator::new();

        // Test all minimum pulses (1000 µs)
        gen.update_channels(&[1000; 8]);
        let mut total = 0u32;
        for _ in 0..18 {
            let (dur, _) = gen.next_interval();
            total += dur as u32;
        }
        assert_eq!(total, PPM_FRAME_TOTAL_US);

        // Test all maximum pulses (2000 µs)
        gen.update_channels(&[2000; 8]);
        let mut total = 0u32;
        for _ in 0..18 {
            let (dur, _) = gen.next_interval();
            total += dur as u32;
        }
        assert_eq!(total, PPM_FRAME_TOTAL_US);
    }

    #[test]
    fn test_ppm_generator_polarity_sequence() {
        let mut gen = PpmGenerator::new();
        gen.update_channels(&[1500; 8]);

        // First interval should be sync pulse: 400 µs LOW
        let (dur0, high0) = gen.next_interval();
        assert_eq!(dur0, 400);
        assert!(!high0);

        // Second interval should be channel 0 high: (1500 - 400) = 1100 µs HIGH
        let (dur1, high1) = gen.next_interval();
        assert_eq!(dur1, 1100);
        assert!(high1);
    }
}
