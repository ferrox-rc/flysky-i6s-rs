//! Key matrix scanning and DFU bootloader detection.

use stm32f0xx_hal::pac;
use crate::chip::{self, McuProfile};

#[inline(always)]
fn delay_cycles(n: u32) {
    cortex_m::asm::delay(n);
}

/// Initialize GPIO clocks and pins for keys and trims.
pub fn init_keys() {
    let rcc = unsafe { &*pac::RCC::ptr() };
    let gpioc = unsafe { &*pac::GPIOC::ptr() };
    let gpiod = unsafe { &*pac::GPIOD::ptr() };
    let gpiof = unsafe { &*pac::GPIOF::ptr() };

    unsafe {
        // Enable GPIOC, GPIOD, GPIOF clocks (bits 19, 20, 22)
        rcc.ahbenr.modify(|r, w| w.bits(r.bits() | (1 << 19) | (1 << 20) | (1 << 22)));

        // Configure PC6, PC7, PC8 as outputs (MODER = 01)
        gpioc.moder.modify(|r, w| {
            let val = r.bits();
            w.bits((val & !(0x3F << 12)) | (0x15 << 12))
        });

        // Set PC6, PC7, PC8 initially HIGH
        gpioc.bsrr.write(|w| w.bits((1 << 6) | (1 << 7) | (1 << 8)));

        // Configure PD12..PD15 as inputs (MODER = 00) with pull-ups (PUPDR = 01)
        gpiod.moder.modify(|r, w| w.bits(r.bits() & !(0xFF << 24)));
        gpiod.pupdr.modify(|r, w| {
            let val = r.bits();
            w.bits((val & !(0xFF << 24)) | (0x55 << 24))
        });

        // Configure PF2 (Bind button) as input with pull-up (MODER = 00, PUPDR = 01)
        gpiof.moder.modify(|r, w| w.bits(r.bits() & !(3 << 4)));
        gpiof.pupdr.modify(|r, w| {
            let val = r.bits();
            w.bits((val & !(3 << 4)) | (1 << 4))
        });
    }
}

/// Scan key matrix and return a 16-bit bitfield of pressed buttons:
/// - bit 0:  Col 0 Line 0 (PC6, PD12) -> Roll R
/// - bit 1:  Col 0 Line 1 (PC6, PD13) -> Roll L (Inward trim for Roll)
/// - bit 2:  Col 0 Line 2 (PC6, PD14) -> Pitch U
/// - bit 3:  Col 0 Line 3 (PC6, PD15) -> Pitch D
/// - bit 4:  Col 1 Line 0 (PC7, PD12) -> Throttle U
/// - bit 5:  Col 1 Line 1 (PC7, PD13) -> Throttle D
/// - bit 6:  Col 1 Line 2 (PC7, PD14) -> Yaw R (Inward trim for Yaw)
/// - bit 7:  Col 1 Line 3 (PC7, PD15) -> Yaw L
/// - bit 8:  Col 2 Line 0 (PC8, PD12) -> Down
/// - bit 9:  Col 2 Line 1 (PC8, PD13) -> Up
/// - bit 10: Col 2 Line 2 (PC8, PD14) -> OK / Enter
/// - bit 11: Col 2 Line 3 (PC8, PD15) -> Cancel / Exit
/// - bit 12: Dedicated Bind key (PF2)
pub fn scan_keys() -> u16 {
    let mut result = 0u16;
    let gpioc = unsafe { &*pac::GPIOC::ptr() };
    let gpiod = unsafe { &*pac::GPIOD::ptr() };
    let gpiof = unsafe { &*pac::GPIOF::ptr() };

    unsafe {
        let cols = [6, 7, 8];
        for (col_idx, &col) in cols.iter().enumerate() {
            // Drive active column LOW (BRy = bit col + 16)
            gpioc.bsrr.write(|w| w.bits(1 << (col + 16)));
            delay_cycles(150); // allow line capacitance to discharge

            // Read lines PD12..PD15 (active LOW)
            let idr = gpiod.idr.read().bits();
            let lines = ((idr >> 12) & 0x0F) as u8;
            let pressed = (!lines) & 0x0F;

            result |= (pressed as u16) << (col_idx * 4);

            // Restore column to HIGH (BSy = bit col)
            gpioc.bsrr.write(|w| w.bits(1 << col));
            delay_cycles(100);
        }

        // Check Bind button (PF2, active LOW)
        let fidr = gpiof.idr.read().bits();
        if (fidr & (1 << 2)) == 0 {
            result |= 1 << 12; // Bind pressed
        }
    }

    result
}

/// Check if the DFU bootloader key combination is held:
/// Both horizontal trims pushed inward towards power switch:
/// Roll Left (bit 1) + Yaw Right (bit 6).
pub fn is_dfu_requested(keys: u16) -> bool {
    let rh_inward = (keys & (1 << 1)) != 0; // Roll Left
    let lh_inward = (keys & (1 << 6)) != 0; // Yaw Right

    rh_inward && lh_inward
}

/// Power-on boot check for DFU entry.
pub fn check_dfu_entry(profile: &McuProfile) {
    init_keys();

    // Wait ~20ms for power rails and switch contacts to settle at power-on
    delay_cycles(40_000);

    let mut match_count = 0;
    for _ in 0..5 {
        let keys = scan_keys();
        if is_dfu_requested(keys) {
            match_count += 1;
        }
        delay_cycles(2_000);
    }

    if match_count >= 3 {
        chip::enter_dfu_bootloader(profile);
    }
}
