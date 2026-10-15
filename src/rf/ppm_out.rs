//! Pulse Position Modulation (PPM) output generator on PF10 (J15 expansion header).
//!
//! Uses hardware TIM15 on STM32F072VB to generate exact microsecond-timed PPM pulses:
//! - PF10 configured as Alternate Function AF1 (TIM15_CH2) or software GPIO pulse modulation via TIM15 IRQ.
//! - Frame period: 22.5 ms (22,500 µs)
//! - 8 Channels: 1000..2000 µs (center 1500 µs)
//! - Sync pulse: 400 µs LOW
//! - Sync gap: fills remaining frame duration (~9..14 ms) HIGH

#![allow(dead_code)]
#![allow(static_mut_refs)]

use core::sync::atomic::{AtomicBool, Ordering};

#[cfg(not(test))]
use stm32f0xx_hal::pac::{self, interrupt};

const PPM_CHANNELS: usize = 8;
const PPM_FRAME_TOTAL_US: u32 = 22_500;
const PPM_SYNC_PULSE_US: u16 = 400;

static PPM_ENABLED: AtomicBool = AtomicBool::new(false);

static mut PPM_GEN: PpmGenerator = PpmGenerator::new();

/// Software/hardware state tracker for PPM pulse train generation
pub struct PpmGenerator {
    pub channels: [u16; PPM_CHANNELS],
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

/// Initialize PF10 pin and TIM15 for PPM output.
pub fn init(enabled: bool) {
    PPM_ENABLED.store(enabled, Ordering::Relaxed);
    #[cfg(not(test))]
    unsafe {
        let rcc = &*pac::RCC::ptr();
        let gpiof = &*pac::GPIOF::ptr();
        let tim15 = &*pac::TIM15::ptr();

        // 1. Enable GPIOF clock (iopfen)
        rcc.ahbenr.modify(|_, w| w.iopfen().set_bit());

        // 2. Configure PF10 as General Purpose Output (MODER bits 21:20 = 01)
        gpiof.moder.modify(|r, w| w.bits((r.bits() & !(3 << 20)) | (1 << 20)));

        // Set PF10 default High (idle state)
        gpiof.bsrr.write(|w| w.bits(1 << 10));

        // 3. Enable TIM15 clock in RCC_APB2ENR (bit 16 / tim15en)
        rcc.apb2enr.modify(|_, w| w.tim15en().set_bit());

        // 4. Configure TIM15: 48 MHz / 48 = 1 MHz count rate (1 tick = 1 µs)
        tim15.psc.write(|w| w.psc().bits(47)); // 48 - 1 = 47
        tim15.arr.write(|w| w.arr().bits(400)); // Initial 400 µs
        tim15.egr.write(|w| w.ug().set_bit());  // Re-initialize counter & prescaler
        tim15.sr.write(|w| w.bits(0));          // Clear update flag

        // Configure NVIC for TIM15 (IRQ 20)
        let mut cp = cortex_m::peripheral::Peripherals::steal();
        cp.NVIC.set_priority(pac::Interrupt::TIM15, 0x80); // Lower priority (0x80)

        if enabled {
            // Enable Update Interrupt (UIE bit 0)
            tim15.dier.write(|w| w.uie().set_bit());
            // Unmask IRQ 20 in NVIC
            cortex_m::peripheral::NVIC::unmask(pac::Interrupt::TIM15);
            // Enable counter (CEN bit 0)
            tim15.cr1.write(|w| w.cen().set_bit());
        }
    }
}

/// Enable or disable PPM output on PF10
pub fn set_enabled(enabled: bool) {
    PPM_ENABLED.store(enabled, Ordering::Relaxed);
    #[cfg(not(test))]
    unsafe {
        let tim15 = &*pac::TIM15::ptr();
        let gpiof = &*pac::GPIOF::ptr();
        if enabled {
            tim15.dier.write(|w| w.uie().set_bit());
            cortex_m::peripheral::NVIC::unmask(pac::Interrupt::TIM15);
            tim15.cr1.write(|w| w.cen().set_bit());
        } else {
            tim15.cr1.write(|w| w.cen().clear_bit());
            tim15.dier.write(|w| w.uie().clear_bit());
            cortex_m::peripheral::NVIC::mask(pac::Interrupt::TIM15);
            // Hold PF10 High in idle
            gpiof.bsrr.write(|w| w.bits(1 << 10));
        }
    }
}

pub fn is_enabled() -> bool {
    PPM_ENABLED.load(Ordering::Relaxed)
}

/// Update output channel values for PPM generation
pub fn update_channels(chs: &[u16]) {
    unsafe {
        PPM_GEN.update_channels(chs);
    }
}

#[cfg(not(test))]
#[interrupt]
fn TIM15() {
    unsafe {
        let tim15 = &*pac::TIM15::ptr();
        let gpiof = &*pac::GPIOF::ptr();

        // Clear update interrupt flag
        tim15.sr.write(|w| w.bits(0));

        if !PPM_ENABLED.load(Ordering::Relaxed) {
            gpiof.bsrr.write(|w| w.bits(1 << 10));
            return;
        }

        // Get next interval and pin state
        let (duration_us, pin_high) = PPM_GEN.next_interval();

        // Write pin state to PF10 immediately
        if pin_high {
            gpiof.bsrr.write(|w| w.bits(1 << 10)); // High
        } else {
            gpiof.bsrr.write(|w| w.bits(1 << (10 + 16))); // Low
        }

        // Set next reload value in µs
        tim15.arr.write(|w| w.arr().bits(duration_us));
    }
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
