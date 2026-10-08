//! Pulse Position Modulation (PPM) output generator on PF10 (J15 expansion header).
//!
//! Standard analog RC PPM frame specification:
//! - Frame duration: 22.5 ms (22,500 µs)
//! - Channel pulses: 8 channels, 1000 µs .. 2000 µs (center 1500 µs)
//! - Channel sync pulse: 300..400 µs low (active low pulse)
//! - Sync gap / idle: Frame gap fills the remaining time until 22,500 µs.

#![allow(dead_code)]

use core::sync::atomic::{AtomicBool, Ordering};

const PPM_CHANNELS: usize = 8;
const PPM_FRAME_TOTAL_US: u32 = 22_500;
const PPM_SYNC_PULSE_US: u16 = 400;

static PPM_ENABLED: AtomicBool = AtomicBool::new(false);

/// Software/hardware state tracker for PPM pulse train generation
pub struct PpmGenerator {
    channels: [u16; PPM_CHANNELS],
    current_ch: usize,
    in_sync_pulse: bool,
    frame_elapsed_us: u32,
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

    /// Update channel pulse widths (clamped to 900..2100 µs)
    pub fn update_channels(&mut self, chs: &[u16]) {
        let count = chs.len().min(PPM_CHANNELS);
        for i in 0..count {
            self.channels[i] = chs[i].clamp(900, 2100);
        }
    }

    /// Calculate next timer duration in microseconds and whether the pin is active LOW or HIGH.
    /// Returns `(duration_us, pin_high)`.
    pub fn next_interval(&mut self) -> (u16, bool) {
        if self.current_ch < PPM_CHANNELS {
            if self.in_sync_pulse {
                // Sync pulse: 400 µs LOW
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
                // Long sync gap (fill remaining time up to 22,500 µs)
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

/// Initialize PF10 pin for PPM output.
pub fn init(enabled: bool) {
    PPM_ENABLED.store(enabled, Ordering::Relaxed);
    #[cfg(not(test))]
    if enabled {
        unsafe {
            // Enable GPIOF clock (bit 22 in RCC_AHBENR)
            let ahb = core::ptr::read_volatile(0x4002_1014 as *mut u32);
            core::ptr::write_volatile(0x4002_1014 as *mut u32, ahb | (1 << 22));

            // PF10: bits 21:20 in GPIOF_MODER = 01 (General purpose output)
            let f_moder = core::ptr::read_volatile(0x4800_1400 as *mut u32);
            core::ptr::write_volatile(0x4800_1400 as *mut u32, (f_moder & !(3 << 20)) | (1 << 20));

            // Set PF10 default High (idle state)
            core::ptr::write_volatile(0x4800_1418 as *mut u32, 1 << 10);
        }
    }
}

/// Enable or disable PPM output on PF10
pub fn set_enabled(enabled: bool) {
    PPM_ENABLED.store(enabled, Ordering::Relaxed);
    #[cfg(not(test))]
    unsafe {
        if !enabled {
            // Set PF10 low / high idle
            core::ptr::write_volatile(0x4800_1418 as *mut u32, 1 << (10 + 16));
        }
    }
}

pub fn is_enabled() -> bool {
    PPM_ENABLED.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ppm_generator_frame_timing() {
        let mut gen = PpmGenerator::new();
        let test_chs = [1000, 1500, 2000, 1500, 1200, 1800, 1000, 2000];
        gen.update_channels(&test_chs);

        let mut total_duration = 0u32;
        // 8 channels * 2 intervals (sync + high) + final sync + gap = 18 intervals
        for _ in 0..18 {
            let (duration, _high) = gen.next_interval();
            total_duration += duration as u32;
        }

        assert_eq!(total_duration, PPM_FRAME_TOTAL_US);
    }
}
