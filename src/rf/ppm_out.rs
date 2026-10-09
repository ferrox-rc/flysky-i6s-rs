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
use stm32f0xx_hal::pac::interrupt;

const PPM_CHANNELS: usize = 8;
const PPM_FRAME_TOTAL_US: u32 = 22_500;
const PPM_SYNC_PULSE_US: u16 = 400;

// Hardware register addresses for STM32F072
const RCC_AHBENR: *mut u32 = 0x4002_1014 as *mut u32;
const RCC_APB2ENR: *mut u32 = 0x4002_1018 as *mut u32;
const GPIOF_MODER: *mut u32 = 0x4800_1400 as *mut u32;
const GPIOF_BSRR: *mut u32 = 0x4800_1418 as *mut u32;

// TIM15 base: 0x4001_4000
const TIM15_CR1: *mut u32 = 0x4001_4000 as *mut u32;
const TIM15_DIER: *mut u32 = 0x4001_400C as *mut u32;
const TIM15_SR: *mut u32 = 0x4001_4010 as *mut u32;
const TIM15_PSC: *mut u32 = 0x4001_4028 as *mut u32;
const TIM15_ARR: *mut u32 = 0x4001_402C as *mut u32;
const TIM15_EGR: *mut u32 = 0x4001_4014 as *mut u32;

// NVIC
const NVIC_ISER: *mut u32 = 0xE000_E100 as *mut u32;
const NVIC_ICER: *mut u32 = 0xE000_E180 as *mut u32;
const NVIC_IPR5: *mut u32 = 0xE000_E414 as *mut u32; // IRQ 20 = byte 0 of IPR5

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
        // 1. Enable GPIOF clock (bit 22 in RCC_AHBENR)
        let ahb = core::ptr::read_volatile(RCC_AHBENR);
        core::ptr::write_volatile(RCC_AHBENR, ahb | (1 << 22));

        // 2. Configure PF10 as General Purpose Output (MODER bits 21:20 = 01)
        let f_moder = core::ptr::read_volatile(GPIOF_MODER);
        core::ptr::write_volatile(GPIOF_MODER, (f_moder & !(3 << 20)) | (1 << 20));

        // Set PF10 default High (idle state)
        core::ptr::write_volatile(GPIOF_BSRR, 1 << 10);

        // 3. Enable TIM15 clock in RCC_APB2ENR (bit 16)
        let apb2 = core::ptr::read_volatile(RCC_APB2ENR);
        core::ptr::write_volatile(RCC_APB2ENR, apb2 | (1 << 16));

        // 4. Configure TIM15: 48 MHz / 48 = 1 MHz count rate (1 tick = 1 µs)
        core::ptr::write_volatile(TIM15_PSC, 47); // 48 - 1 = 47
        core::ptr::write_volatile(TIM15_ARR, 400); // Initial 400 µs
        core::ptr::write_volatile(TIM15_EGR, 1);   // Re-initialize counter & prescaler
        core::ptr::write_volatile(TIM15_SR, 0);    // Clear update flag

        // Configure NVIC for TIM15 (IRQ 20)
        let ipr5 = core::ptr::read_volatile(NVIC_IPR5);
        core::ptr::write_volatile(NVIC_IPR5, (ipr5 & !0xFF) | 0x80); // Lower priority (0x80)

        if enabled {
            // Enable Update Interrupt (UIE bit 0)
            core::ptr::write_volatile(TIM15_DIER, 1);
            // Unmask IRQ 20 in NVIC
            core::ptr::write_volatile(NVIC_ISER, 1 << 20);
            // Enable counter (CEN bit 0)
            core::ptr::write_volatile(TIM15_CR1, 1);
        }
    }
}

/// Enable or disable PPM output on PF10
pub fn set_enabled(enabled: bool) {
    PPM_ENABLED.store(enabled, Ordering::Relaxed);
    #[cfg(not(test))]
    unsafe {
        if enabled {
            core::ptr::write_volatile(TIM15_DIER, 1);
            core::ptr::write_volatile(NVIC_ISER, 1 << 20);
            core::ptr::write_volatile(TIM15_CR1, 1);
        } else {
            core::ptr::write_volatile(TIM15_CR1, 0);
            core::ptr::write_volatile(TIM15_DIER, 0);
            core::ptr::write_volatile(NVIC_ICER, 1 << 20);
            // Hold PF10 High in idle
            core::ptr::write_volatile(GPIOF_BSRR, 1 << 10);
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
        // Clear update interrupt flag
        core::ptr::write_volatile(TIM15_SR, 0);

        if !PPM_ENABLED.load(Ordering::Relaxed) {
            core::ptr::write_volatile(GPIOF_BSRR, 1 << 10);
            return;
        }

        // Get next interval and pin state
        let (duration_us, pin_high) = PPM_GEN.next_interval();

        // Write pin state to PF10 immediately
        if pin_high {
            core::ptr::write_volatile(GPIOF_BSRR, 1 << 10); // High
        } else {
            core::ptr::write_volatile(GPIOF_BSRR, 1 << (10 + 16)); // Low
        }

        // Set next reload value in µs
        core::ptr::write_volatile(TIM15_ARR, duration_us as u32);
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
