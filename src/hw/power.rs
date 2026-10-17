//! Electronic power management subsystem for FlySky FS-i6S.
//!
//! Controls the hardware power latch (`PB15`), dual power button sense line (`PB14`),
//! and power button blue illumination LEDs (`PD10`, `PD11`).
//!
//! Power Lifecycle:
//! 1. User presses both front power buttons, physically completing the power circuit to the MCU.
//! 2. Firmware boots and calls `power::init()` as its very first instruction, asserting `PB15` HIGH
//!    to electronically latch the power rail before the user releases the buttons.
//! 3. `PD10` and `PD11` are driven HIGH to turn on the blue power LEDs.
//! 4. `PB14` is continuously monitored: if held for >= 1.5 seconds, `PowerManager` performs a safe
//!    graceful shutdown (plays shutdown chime, flushes flash storage, turns off LEDs, and drives `PB15` LOW).

use crate::buzzer::Buzzer;
use crate::storage::{self, RadioStorage};

#[cfg(not(test))]
use stm32f0xx_hal::pac;

#[cfg(test)]
use core::sync::atomic::{AtomicBool, Ordering};

#[cfg(test)]
static TEST_PB15_LATCH: AtomicBool = AtomicBool::new(false);
#[cfg(test)]
static TEST_LEDS_ON: AtomicBool = AtomicBool::new(false);
#[cfg(test)]
static TEST_PB14_PRESSED: AtomicBool = AtomicBool::new(false);
#[cfg(test)]
static TEST_POWERED_OFF: AtomicBool = AtomicBool::new(false);

/// Required hold duration (in milliseconds) on PB14 to trigger graceful shutdown.
pub const SHUTDOWN_HOLD_MS: u16 = 1500;

/// Initialize GPIO clocks and immediately latch system power ON.
///
/// MUST be called as the very first instruction in `main()` to prevent the radio
/// from powering off when the user releases the front power buttons.
pub fn init() {
    #[cfg(not(test))]
    {
        let rcc = unsafe { &*pac::RCC::ptr() };
        let gpiob = unsafe { &*pac::GPIOB::ptr() };
        let gpiod = unsafe { &*pac::GPIOD::ptr() };

        unsafe {
            // Enable GPIOB (bit 18) and GPIOD (bit 20) in RCC_AHBENR
            rcc.ahbenr.modify(|r, w| w.bits(r.bits() | (1 << 18) | (1 << 20)));

            // Configure PB15 as push-pull output (MODER bits 31:30 = 01)
            gpiob.moder.modify(|r, w| {
                let val = r.bits();
                w.bits((val & !(3 << 30)) | (1 << 30))
            });

            // Latch power ON immediately by driving PB15 HIGH (BS15 = bit 15)
            gpiob.bsrr.write(|w| w.bits(1 << 15));

            // Configure PB14 as input (MODER bits 29:28 = 00) with pull-up (PUPDR bits 29:28 = 01)
            gpiob.moder.modify(|r, w| w.bits(r.bits() & !(3 << 28)));
            gpiob.pupdr.modify(|r, w| {
                let val = r.bits();
                w.bits((val & !(3 << 28)) | (1 << 28))
            });

            // Configure PD10 and PD11 as push-pull outputs (MODER bits 21:20 = 01, 23:22 = 01)
            gpiod.moder.modify(|r, w| {
                let val = r.bits();
                w.bits((val & !(0x0F << 20)) | (0x05 << 20))
            });

            // Turn on power button blue LEDs (BS10 = bit 10, BS11 = bit 11)
            gpiod.bsrr.write(|w| w.bits((1 << 10) | (1 << 11)));
        }
    }

    #[cfg(test)]
    {
        TEST_PB15_LATCH.store(true, Ordering::SeqCst);
        TEST_LEDS_ON.store(true, Ordering::SeqCst);
        TEST_POWERED_OFF.store(false, Ordering::SeqCst);
    }
}

/// Ensure PB15 is actively driven HIGH.
///
/// Called before jumping to the STM32 factory ROM DFU bootloader so the radio
/// does not lose power when operating unattached to a USB host.
pub fn latch_on() {
    #[cfg(not(test))]
    {
        let rcc = unsafe { &*pac::RCC::ptr() };
        let gpiob = unsafe { &*pac::GPIOB::ptr() };

        unsafe {
            // Guarantee GPIOB clock is active
            rcc.ahbenr.modify(|r, w| w.bits(r.bits() | (1 << 18)));

            // Guarantee PB15 is push-pull output
            gpiob.moder.modify(|r, w| {
                let val = r.bits();
                w.bits((val & !(3 << 30)) | (1 << 30))
            });

            // Assert PB15 HIGH
            gpiob.bsrr.write(|w| w.bits(1 << 15));
        }
    }

    #[cfg(test)]
    {
        TEST_PB15_LATCH.store(true, Ordering::SeqCst);
    }
}

/// Control power button blue LEDs on PD10 and PD11.
pub fn set_leds(on: bool) {
    #[cfg(not(test))]
    {
        let gpiod = unsafe { &*pac::GPIOD::ptr() };
        unsafe {
            if on {
                gpiod.bsrr.write(|w| w.bits((1 << 10) | (1 << 11)));
            } else {
                gpiod.bsrr.write(|w| w.bits(((1 << 10) | (1 << 11)) << 16));
            }
        }
    }

    #[cfg(test)]
    {
        TEST_LEDS_ON.store(on, Ordering::SeqCst);
    }
}

/// Check if the dual front power buttons are currently pressed (PB14 Active LOW).
pub fn is_power_button_pressed() -> bool {
    #[cfg(not(test))]
    {
        let gpiob = unsafe { &*pac::GPIOB::ptr() };
        let idr = gpiob.idr.read().bits();
        // Bit 14 is 0 when grounded / pressed
        (idr & (1 << 14)) == 0
    }

    #[cfg(test)]
    {
        TEST_PB14_PRESSED.load(Ordering::SeqCst)
    }
}

/// Cut power to the transmitter by driving PB15 LOW.
///
/// Halts the CPU in an infinite wait-for-interrupt loop while the hardware power rail decays.
pub fn power_off() -> ! {
    set_leds(false);

    #[cfg(not(test))]
    {
        let gpiob = unsafe { &*pac::GPIOB::ptr() };
        unsafe {
            // Drive PB15 LOW (BR15 = bit 31)
            gpiob.bsrr.write(|w| w.bits(1 << 31));

            // Sleep while hardware power turns off
            loop {
                cortex_m::asm::wfi();
            }
        }
    }

    #[cfg(test)]
    {
        TEST_PB15_LATCH.store(false, Ordering::SeqCst);
        TEST_POWERED_OFF.store(true, Ordering::SeqCst);
        panic!("simulated power_off");
    }
}

/// State machine for graceful shutdown monitoring on PB14.
pub struct PowerManager {
    /// Guard: user must release the power buttons after turning on before shutdown can be armed.
    boot_release_detected: bool,
    /// Continuous duration (in ms) the power button has been held.
    hold_duration_ms: u16,
    /// True once shutdown sequence has been initiated.
    is_shutting_down: bool,
}

impl Default for PowerManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PowerManager {
    pub const fn new() -> Self {
        Self {
            boot_release_detected: false,
            hold_duration_ms: 0,
            is_shutting_down: false,
        }
    }

    /// True if the shutdown sequence has completed or is in progress.
    pub fn is_shutting_down(&self) -> bool {
        self.is_shutting_down
    }

    /// Return the shutdown hold progress as a percentage (0..100) for UI display.
    pub fn shutdown_progress_pct(&self) -> u8 {
        if !self.boot_release_detected {
            0
        } else {
            ((self.hold_duration_ms as u32 * 100) / (SHUTDOWN_HOLD_MS as u32)).min(100) as u8
        }
    }

    /// Update shutdown monitoring.
    ///
    /// If held for >= 1.5 seconds after initial boot release, executes graceful shutdown:
    /// - Plays power-off sound
    /// - Flushes active model and radio configuration to flash
    /// - Turns off LEDs
    /// - Drives PB15 LOW to cut power
    pub fn update(
        &mut self,
        dt_ms: u16,
        storage: &mut RadioStorage,
        buzzer: &mut Buzzer,
    ) {
        if self.is_shutting_down {
            return;
        }

        let pressed = is_power_button_pressed();

        // 1. Enforce initial button release after cold boot
        if !self.boot_release_detected {
            if !pressed {
                self.boot_release_detected = true;
            }
            return;
        }

        // 2. Accumulate hold duration
        if pressed {
            self.hold_duration_ms = self.hold_duration_ms.saturating_add(dt_ms);

            // 3. Trigger graceful shutdown once hold threshold is reached
            if self.hold_duration_ms >= SHUTDOWN_HOLD_MS {
                self.is_shutting_down = true;

                // Play power-down tones
                buzzer.play_tone_pattern(1800, 80, 40, 2);

                // Commit model and radio settings safely to flash
                storage::save_active_model(storage);
                storage::save_storage(storage);

                // Turn off power latch
                power_off();
            }
        } else {
            self.hold_duration_ms = 0;
        }
    }
}

#[cfg(test)]
pub fn test_set_power_button_pressed(pressed: bool) {
    TEST_PB14_PRESSED.store(pressed, Ordering::SeqCst);
}

#[cfg(test)]
pub fn test_is_latch_on() -> bool {
    TEST_PB15_LATCH.load(Ordering::SeqCst)
}

#[cfg(test)]
pub fn test_is_leds_on() -> bool {
    TEST_LEDS_ON.load(Ordering::SeqCst)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_power_init_and_latch() {
        init();
        assert!(test_is_latch_on());
        assert!(test_is_leds_on());
    }

    #[test]
    fn test_power_manager_requires_boot_release() {
        init();
        let mut pm = PowerManager::new();
        let mut storage = RadioStorage::empty();
        let mut buzzer = Buzzer::new();

        // Simulate held at boot
        test_set_power_button_pressed(true);
        pm.update(2000, &mut storage, &mut buzzer);
        assert!(!pm.is_shutting_down);
        assert_eq!(pm.shutdown_progress_pct(), 0);

        // Release button
        test_set_power_button_pressed(false);
        pm.update(50, &mut storage, &mut buzzer);
        assert!(pm.boot_release_detected);

        // Press and hold for partial duration
        test_set_power_button_pressed(true);
        pm.update(750, &mut storage, &mut buzzer);
        assert!(!pm.is_shutting_down);
        assert!(pm.shutdown_progress_pct() >= 49);

        // Release early: resets hold timer
        test_set_power_button_pressed(false);
        pm.update(50, &mut storage, &mut buzzer);
        assert_eq!(pm.shutdown_progress_pct(), 0);
    }
}
