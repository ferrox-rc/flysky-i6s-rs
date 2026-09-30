//! Hardware SysTick 1.000 ms monotonic timekeeping driver.

use core::sync::atomic::{AtomicU32, Ordering};
use cortex_m_rt::exception;

static SYSTEM_MILLIS: AtomicU32 = AtomicU32::new(0);

/// Initialize Cortex-M SysTick timer for 1 kHz (1.000 ms) ticks at 48 MHz core clock.
/// Configures SysTick to lowest hardware interrupt priority (0xC0) so it never
/// preempts or delays RF time-critical IRQs (TIM16 and EXTI2_3 at 0x80).
pub fn init() {
    unsafe {
        // SysTick base: 0xE000_E010
        // Set SysTick interrupt priority to lowest (0xC0) in SHPR3 (bits 31:24)
        let shpr3 = 0xE000_ED20 as *mut u32;
        let val = core::ptr::read_volatile(shpr3);
        core::ptr::write_volatile(shpr3, (val & !(0xFF << 24)) | (0xC0 << 24));

        let syst_csr = 0xE000_E010 as *mut u32;
        let syst_rvr = 0xE000_E014 as *mut u32;
        let syst_cvr = 0xE000_E018 as *mut u32;

        // 48 MHz clock -> 48,000 ticks per 1 ms
        core::ptr::write_volatile(syst_rvr, 48_000 - 1);
        core::ptr::write_volatile(syst_cvr, 0);
        // Enable counter (bit 0), enable interrupt (bit 1), processor clock (bit 2)
        core::ptr::write_volatile(syst_csr, (1 << 0) | (1 << 1) | (1 << 2));
    }
}

/// Monotonic system uptime in milliseconds since boot.
/// Lock-free, zero-overhead atomic read with Relaxed ordering.
#[inline(always)]
pub fn millis() -> u32 {
    SYSTEM_MILLIS.load(Ordering::Relaxed)
}

#[exception]
fn SysTick() {
    let current = SYSTEM_MILLIS.load(Ordering::Relaxed);
    SYSTEM_MILLIS.store(current.wrapping_add(1), Ordering::Relaxed);
}

#[cfg(test)]
pub fn set_millis(ms: u32) {
    SYSTEM_MILLIS.store(ms, Ordering::Relaxed);
}

#[cfg(test)]
pub fn advance_millis(ms: u32) {
    let cur = SYSTEM_MILLIS.load(Ordering::Relaxed);
    SYSTEM_MILLIS.store(cur.wrapping_add(ms), Ordering::Relaxed);
}

