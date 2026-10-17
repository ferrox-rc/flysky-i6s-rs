//! Trainer Subsystem & PPM In/Out Engine.
//!
//! Orchestrates:
//! - **PPM Out on `PF10` (`TIM15_CH2`)**: 22.5 ms 8-channel CPPM generator for simulators and external RF modules.
//! - **PPM In on `PF9` (`TIM15_CH1`)**: Input capture decoder for student buddy-box and FPV head-trackers.
//! - **Safety Handover**: Instant instructor stick takeover (> 5% deflection) and signal loss fallback.

#![allow(static_mut_refs)]
#![allow(dead_code)]

pub mod handover;
pub mod ppm_in;
pub mod ppm_out;

pub use handover::{SafetyHandover, TrainerState};
pub use ppm_in::PpmDecoder;
pub use ppm_out::PpmGenerator;

use core::sync::atomic::{AtomicBool, Ordering};

#[cfg(not(test))]
use stm32f0xx_hal::pac::interrupt;

// Hardware register base addresses for STM32F072
const RCC_AHBENR: *mut u32 = 0x4002_1014 as *mut u32;
const RCC_APB2ENR: *mut u32 = 0x4002_1018 as *mut u32;
const GPIOF_MODER: *mut u32 = 0x4800_1400 as *mut u32;
const GPIOF_PUPDR: *mut u32 = 0x4800_140C as *mut u32;
const GPIOF_AFRH: *mut u32 = 0x4800_1424 as *mut u32;
const GPIOF_BSRR: *mut u32 = 0x4800_1418 as *mut u32;

// TIM15 base: 0x4001_4000
const TIM15_CR1: *mut u32 = 0x4001_4000 as *mut u32;
const TIM15_DIER: *mut u32 = 0x4001_400C as *mut u32;
const TIM15_SR: *mut u32 = 0x4001_4010 as *mut u32;
const TIM15_EGR: *mut u32 = 0x4001_4014 as *mut u32;
const TIM15_CCMR1: *mut u32 = 0x4001_4018 as *mut u32;
const TIM15_CCER: *mut u32 = 0x4001_4020 as *mut u32;
const TIM15_PSC: *mut u32 = 0x4001_4028 as *mut u32;
const TIM15_ARR: *mut u32 = 0x4001_402C as *mut u32;
const TIM15_CCR1: *mut u32 = 0x4001_4034 as *mut u32;
const TIM15_CCR2: *mut u32 = 0x4001_4038 as *mut u32;

// NVIC registers
const NVIC_ISER: *mut u32 = 0xE000_E100 as *mut u32;
const NVIC_ICER: *mut u32 = 0xE000_E180 as *mut u32;
const NVIC_IPR5: *mut u32 = 0xE000_E414 as *mut u32; // IRQ 20 = byte 0 of IPR5

static PPM_OUT_ENABLED: AtomicBool = AtomicBool::new(false);
static PPM_IN_ENABLED: AtomicBool = AtomicBool::new(false);

static mut PPM_OUT_GEN: PpmGenerator = PpmGenerator::new();
static mut PPM_IN_DEC: PpmDecoder = PpmDecoder::new();
static mut HANDOVER: SafetyHandover = SafetyHandover::new();
static mut LAST_CAPT: u16 = 0;

/// Initialize hardware TIM15 and GPIO pins for Trainer Subsystem.
pub fn init(ppm_out_en: bool, ppm_in_en: bool) {
    PPM_OUT_ENABLED.store(ppm_out_en, Ordering::Relaxed);
    PPM_IN_ENABLED.store(ppm_in_en, Ordering::Relaxed);

    #[cfg(not(test))]
    unsafe {
        // 1. Enable GPIOF clock (bit 22 in RCC_AHBENR)
        let ahb = core::ptr::read_volatile(RCC_AHBENR);
        core::ptr::write_volatile(RCC_AHBENR, ahb | (1 << 22));

        // 2. Configure PF10 (PPM Out) as General Purpose Output (MODER bits 21:20 = 01)
        let f_moder = core::ptr::read_volatile(GPIOF_MODER);
        core::ptr::write_volatile(
            GPIOF_MODER,
            (f_moder & !(3 << 20) & !(3 << 18)) | (1 << 20) | (2 << 18),
        );
        // Default PF10 HIGH (idle PPM line)
        core::ptr::write_volatile(GPIOF_BSRR, 1 << 10);

        // 3. Configure PF9 (PPM In) as Alternate Function AF0 (TIM15_CH1) with pull-up
        let f_pupdr = core::ptr::read_volatile(GPIOF_PUPDR);
        core::ptr::write_volatile(GPIOF_PUPDR, (f_pupdr & !(3 << 18)) | (1 << 18)); // Pull-up on PF9

        // AF0 for PF9 (AFRH bits 7:4 = 0000)
        let f_afrh = core::ptr::read_volatile(GPIOF_AFRH);
        core::ptr::write_volatile(GPIOF_AFRH, f_afrh & !(0xF << 4));

        // 4. Enable TIM15 clock in RCC_APB2ENR (bit 16)
        let apb2 = core::ptr::read_volatile(RCC_APB2ENR);
        core::ptr::write_volatile(RCC_APB2ENR, apb2 | (1 << 16));

        // 5. Configure TIM15: 48 MHz / 48 = 1 MHz count rate (1 tick = 1 µs)
        core::ptr::write_volatile(TIM15_PSC, 47); // 48 - 1 = 47
        core::ptr::write_volatile(TIM15_ARR, 0xFFFF); // Free-running 16-bit counter (65,535 µs)

        // CCMR1:
        // CH1 (bits 7:0): CC1S = 01 (Input on TI1), IC1F = 0011 (digital filter 8 samples) -> 0x31
        // CH2 (bits 15:8): CC2S = 00 (Output Compare)
        core::ptr::write_volatile(TIM15_CCMR1, 0x0031);

        // CCER:
        // CH1: CC1E = 1 (bit 0), CC1P = 1 (bit 1, falling edge)
        // CH2: CC2E = 1 (bit 4)
        core::ptr::write_volatile(TIM15_CCER, (1 << 0) | (1 << 1) | (1 << 4));

        // Schedule first compare match for PPM Out
        core::ptr::write_volatile(TIM15_CCR2, 400);

        // Re-initialize counter & prescaler
        core::ptr::write_volatile(TIM15_EGR, 1);
        core::ptr::write_volatile(TIM15_SR, 0); // Clear flags

        // Configure NVIC for TIM15 (IRQ 20) with priority 0x90
        let ipr5 = core::ptr::read_volatile(NVIC_IPR5);
        core::ptr::write_volatile(NVIC_IPR5, (ipr5 & !0xFF) | 0x90);

        update_timer_state(ppm_out_en, ppm_in_en);
    }
}

/// Enable or disable PPM output on PF10.
pub fn set_ppm_out(enabled: bool) {
    PPM_OUT_ENABLED.store(enabled, Ordering::Relaxed);
    #[cfg(not(test))]
    unsafe {
        update_timer_state(enabled, PPM_IN_ENABLED.load(Ordering::Relaxed));
    }
}

/// Enable or disable PPM input on PF9.
pub fn set_ppm_in(enabled: bool) {
    PPM_IN_ENABLED.store(enabled, Ordering::Relaxed);
    #[cfg(not(test))]
    unsafe {
        update_timer_state(PPM_OUT_ENABLED.load(Ordering::Relaxed), enabled);
    }
}

pub fn is_ppm_out_enabled() -> bool {
    PPM_OUT_ENABLED.load(Ordering::Relaxed)
}

pub fn is_ppm_in_enabled() -> bool {
    PPM_IN_ENABLED.load(Ordering::Relaxed)
}

/// Update output channel pulse widths for PPM transmission (clamped 900..2100 µs).
pub fn update_out_channels(chs: &[u16]) {
    unsafe {
        PPM_OUT_GEN.update_channels(chs);
    }
}

/// Periodic tick (dt_ms) to manage student signal loss timeout.
pub fn tick_ms(dt_ms: u16) {
    unsafe {
        PPM_IN_DEC.tick_ms(dt_ms);
    }
}

/// Periodic 10ms tick helper.
pub fn tick_10ms() {
    tick_ms(10);
}

/// Get decoded student channel inputs if valid.
pub fn get_student_channels() -> Option<[u16; 8]> {
    unsafe { PPM_IN_DEC.get_channels() }
}

/// Get current safety handover status.
pub fn get_handover_state() -> TrainerState {
    unsafe { HANDOVER.state }
}

/// Process safety handover between instructor sticks and decoded student stream.
pub fn process_handover(
    trainer_switch_active: bool,
    instructor_chs: &[u16],
    output_chs: &mut [u16],
) {
    unsafe {
        let student_chs = PPM_IN_DEC.get_channels();
        HANDOVER.process(
            trainer_switch_active,
            student_chs.as_ref(),
            instructor_chs,
            output_chs,
        );
    }
}

#[cfg(not(test))]
unsafe fn update_timer_state(out_en: bool, in_en: bool) {
    let mut dier = 0u32;
    if out_en {
        dier |= 1 << 2; // CC2IE
    }
    if in_en {
        dier |= 1 << 1; // CC1IE
    }

    core::ptr::write_volatile(TIM15_DIER, dier);

    if out_en || in_en {
        core::ptr::write_volatile(NVIC_ISER, 1 << 20); // Enable IRQ 20
        core::ptr::write_volatile(TIM15_CR1, 1); // Start CEN
    } else {
        core::ptr::write_volatile(TIM15_CR1, 0); // Stop CEN
        core::ptr::write_volatile(NVIC_ICER, 1 << 20); // Mask IRQ 20
        core::ptr::write_volatile(GPIOF_BSRR, 1 << 10); // Hold PF10 High in idle
    }
}

#[cfg(not(test))]
#[interrupt]
fn TIM15() {
    unsafe {
        let sr = core::ptr::read_volatile(TIM15_SR);

        // 1. Channel 2: PPM Output Compare Match (PF10)
        if (sr & (1 << 2)) != 0 {
            core::ptr::write_volatile(TIM15_SR, sr & !(1 << 2));

            if PPM_OUT_ENABLED.load(Ordering::Relaxed) {
                let (duration_us, pin_high) = PPM_OUT_GEN.next_interval();
                if pin_high {
                    core::ptr::write_volatile(GPIOF_BSRR, 1 << 10);
                } else {
                    core::ptr::write_volatile(GPIOF_BSRR, 1 << (10 + 16));
                }
                let ccr2 = core::ptr::read_volatile(TIM15_CCR2) as u16;
                core::ptr::write_volatile(TIM15_CCR2, ccr2.wrapping_add(duration_us) as u32);
            } else {
                core::ptr::write_volatile(GPIOF_BSRR, 1 << 10);
            }
        }

        // 2. Channel 1: PPM Input Capture (PF9)
        if (sr & (1 << 1)) != 0 {
            core::ptr::write_volatile(TIM15_SR, sr & !(1 << 1));

            let capt = core::ptr::read_volatile(TIM15_CCR1) as u16;
            let delta = capt.wrapping_sub(LAST_CAPT);
            LAST_CAPT = capt;

            if PPM_IN_ENABLED.load(Ordering::Relaxed) {
                PPM_IN_DEC.process_pulse(delta);
            }
        }
    }
}
