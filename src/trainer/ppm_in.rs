//! Pulse Position Modulation (PPM) input decoder on PF9 (Rear Trainer Port).
//!
//! Decodes incoming CPPM streams from student transmitters, buddy-box cables,
//! and FPV head-trackers into normalized channel pulse widths (1000..2000 µs).
//!
//! Features:
//! - Sub-microsecond edge-to-edge delta measurement.
//! - Deterministic frame sync detection (> 3,000 µs gap).
//! - Glitch and noise rejection on malformed pulses (< 800 µs or 2,200..3,000 µs).
//! - Multi-channel extraction (up to 8 channels, e.g. AIL, ELE, THR, RUD, AUX1..AUX4).
//! - Automatic signal loss failsafe timeout (100 ms timeout window).

pub const MAX_PPM_IN_CHANNELS: usize = 8;
pub const MIN_VALID_PULSE_US: u16 = 850;
pub const MAX_VALID_PULSE_US: u16 = 2150;
pub const MIN_SYNC_GAP_US: u16 = 3000;
pub const MAX_FRAME_GAP_US: u32 = 30000;
pub const SIGNAL_TIMEOUT_MS: u16 = 100;

/// PPM Input Decoder state machine.
#[derive(Copy, Clone, Debug)]
pub struct PpmDecoder {
    /// Active, validated channel outputs in microseconds (1000..2000 µs).
    pub channels: [u16; MAX_PPM_IN_CHANNELS],
    /// Channels currently being received in active frame.
    temp_channels: [u16; MAX_PPM_IN_CHANNELS],
    current_ch: usize,
    in_sync: bool,
    /// Milliseconds remaining before signal is flagged as lost.
    validity_timer_ms: u16,
    /// Indicates whether a valid student signal is currently active.
    is_valid: bool,
    /// Total valid frames successfully decoded.
    pub frame_count: u32,
}

impl Default for PpmDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl PpmDecoder {
    pub const fn new() -> Self {
        Self {
            channels: [1500; MAX_PPM_IN_CHANNELS],
            temp_channels: [1500; MAX_PPM_IN_CHANNELS],
            current_ch: 0,
            in_sync: false,
            validity_timer_ms: 0,
            is_valid: false,
            frame_count: 0,
        }
    }

    /// Process an edge-to-edge pulse duration in microseconds (e.g. from TIM15_CH1 capture delta).
    pub fn process_pulse(&mut self, pulse_us: u16) {
        if pulse_us >= MIN_SYNC_GAP_US && (pulse_us as u32) < MAX_FRAME_GAP_US {
            // Valid frame sync gap detected: commit frame if we received at least 4 channels
            if self.in_sync && self.current_ch >= 4 {
                // Copy decoded channels to active buffer
                for i in 0..self.current_ch {
                    self.channels[i] = self.temp_channels[i];
                }
                self.is_valid = true;
                self.validity_timer_ms = SIGNAL_TIMEOUT_MS;
                self.frame_count = self.frame_count.wrapping_add(1);
            }
            // Prepare for next frame
            self.in_sync = true;
            self.current_ch = 0;
        } else if self.in_sync {
            if (MIN_VALID_PULSE_US..=MAX_VALID_PULSE_US).contains(&pulse_us) {
                if self.current_ch < MAX_PPM_IN_CHANNELS {
                    self.temp_channels[self.current_ch] = pulse_us.clamp(988, 2012);
                    self.current_ch += 1;
                }
            } else {
                // Pulse out of range (glitch / electrical noise): abort current frame
                self.in_sync = false;
                self.current_ch = 0;
            }
        }
    }

    /// Periodic elapsed time update (call every 10 ms or dt_ms).
    pub fn tick_ms(&mut self, dt_ms: u16) {
        if self.validity_timer_ms > 0 {
            self.validity_timer_ms = self.validity_timer_ms.saturating_sub(dt_ms);
            if self.validity_timer_ms == 0 {
                self.is_valid = false;
            }
        }
    }

    /// Returns true if a valid, non-timed-out student signal is present.
    #[inline(always)]
    pub fn is_signal_valid(&self) -> bool {
        self.is_valid && self.validity_timer_ms > 0
    }

    /// Return decoded channels if signal is valid, or None if signal is lost / inactive.
    pub fn get_channels(&self) -> Option<[u16; MAX_PPM_IN_CHANNELS]> {
        if self.is_signal_valid() {
            Some(self.channels)
        } else {
            None
        }
    }

    /// Force reset decoder state.
    pub fn reset(&mut self) {
        self.current_ch = 0;
        self.in_sync = false;
        self.validity_timer_ms = 0;
        self.is_valid = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ppm_decoder_clean_8ch_frame() {
        let mut decoder = PpmDecoder::new();
        assert!(!decoder.is_signal_valid());

        // 1. Initial sync gap to establish lock
        decoder.process_pulse(10_000);
        assert!(!decoder.is_signal_valid()); // Not valid until a complete frame finishes

        // 2. Feed 8 channel pulses
        let test_channels: [u16; 8] = [1000, 1200, 1500, 1800, 2000, 1100, 1600, 1900];
        for &ch in test_channels.iter() {
            decoder.process_pulse(ch);
        }

        // 3. Ending sync gap commits the frame
        decoder.process_pulse(10_000);

        assert!(decoder.is_signal_valid());
        assert_eq!(decoder.frame_count, 1);
        let chs = decoder.get_channels().expect("Expected valid channels");
        assert_eq!(chs, test_channels);
    }

    #[test]
    fn test_ppm_decoder_noise_glitch_aborts_frame() {
        let mut decoder = PpmDecoder::new();
        decoder.process_pulse(10_000); // Sync lock

        // Valid CH 1 and CH 2
        decoder.process_pulse(1500);
        decoder.process_pulse(1500);

        // Glitch pulse: 400 µs (too short for full channel pulse)
        decoder.process_pulse(400);

        // Remaining pulses
        decoder.process_pulse(1500);
        decoder.process_pulse(1500);
        decoder.process_pulse(10_000); // Frame end

        // Should not have committed because sync was broken by glitch
        assert!(!decoder.is_signal_valid());
        assert_eq!(decoder.frame_count, 0);
    }

    #[test]
    fn test_ppm_decoder_signal_timeout() {
        let mut decoder = PpmDecoder::new();
        decoder.process_pulse(10_000);
        for _ in 0..4 {
            decoder.process_pulse(1500);
        }
        decoder.process_pulse(10_000);

        assert!(decoder.is_signal_valid());

        // Tick 50 ms (still within 100 ms timeout)
        decoder.tick_ms(50);
        assert!(decoder.is_signal_valid());

        // Tick another 60 ms (total 110 ms > 100 ms timeout)
        decoder.tick_ms(60);
        assert!(!decoder.is_signal_valid());
        assert!(decoder.get_channels().is_none());
    }
}
