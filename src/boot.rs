//! Boot manager, Front Tactile Button scanning, and DFU bootloader detection for FlySky FS-i6S.
//!
//! Provides three distinct zero-disassembly mechanisms to enter the STM32 factory ROM DFU bootloader:
//! 1. Cold-Boot Front Button Combo: Hold Left (`PA9`) + Right (`PA10`) during power-on.
//! 2. Touch Menu Reboot: Settings -> System Info -> [Reboot to DFU Mode] writes `0xDEADBEEF`
//!    to SRAM flag `0x2000_3FF0` and performs a warm reboot into DFU.
//! 3. USB CDC Serial CLI Command: Sending `dfu` or `reboot bootloader` over USB serial.

use crate::chip::{self, McuProfile};

#[cfg(not(test))]
use stm32f0xx_hal::pac;

#[cfg(test)]
use core::sync::atomic::{AtomicBool, AtomicU32};

/// Magic token in high SRAM used to signal a warm reboot directly into the DFU bootloader.
pub const DFU_MAGIC_FLAG: u32 = 0xDEAD_BEEF;

/// Address in top 16 bytes of STM32F072 16KB SRAM (0x2000_0000 .. 0x2000_4000).
pub const DFU_FLAG_ADDR: *mut u32 = 0x2000_3FF0 as *mut u32;

#[cfg(test)]
static TEST_SRAM_FLAG: AtomicU32 = AtomicU32::new(0);
#[cfg(test)]
static TEST_FRONT_LEFT: AtomicBool = AtomicBool::new(false);
#[cfg(test)]
static TEST_FRONT_RIGHT: AtomicBool = AtomicBool::new(false);

/// Atomic holder for touch-injected navigation & virtual trim keys.
static TOUCH_NAV_KEYS: core::sync::atomic::AtomicU16 = core::sync::atomic::AtomicU16::new(0);

#[inline(always)]
fn delay_cycles(n: u32) {
    #[cfg(not(test))]
    cortex_m::asm::delay(n);
    #[cfg(test)]
    let _ = n;
}

/// Initialize GPIO clocks and pins for the FS-i6S rear tactile buttons:
/// - PA9: Rear Left Button (Active LOW with internal pull-up) -> Cancel / Back
/// - PA10: Rear Right Button (Active LOW with internal pull-up) -> OK / Select
pub fn init_keys() {
    #[cfg(not(test))]
    {
        let rcc = unsafe { &*pac::RCC::ptr() };
        let gpioa = unsafe { &*pac::GPIOA::ptr() };

        unsafe {
            // Enable GPIOA clock (bit 17 in RCC_AHBENR)
            rcc.ahbenr.modify(|r, w| w.bits(r.bits() | (1 << 17)));

            // Configure PA9 and PA10 as inputs (MODER bits 19:18 = 00, 21:20 = 00)
            gpioa.moder.modify(|r, w| w.bits(r.bits() & !(0x0F << 18)));

            // Configure PA9 and PA10 with internal pull-ups (PUPDR bits 19:18 = 01, 21:20 = 01)
            gpioa.pupdr.modify(|r, w| {
                let val = r.bits();
                w.bits((val & !(0x0F << 18)) | (0x05 << 18))
            });
        }
    }

    #[cfg(test)]
    {
        // Reset test state
        TOUCH_NAV_KEYS.store(0, core::sync::atomic::Ordering::SeqCst);
    }
}

/// Inject virtual touch keys and gestures into the main `scan_keys()` bitfield.
pub fn set_touch_keys(keys: u16) {
    TOUCH_NAV_KEYS.store(keys, core::sync::atomic::Ordering::Relaxed);
}

/// Read physical front tactile buttons:
/// Returns `(left_pressed, right_pressed)`.
/// - Left button on `PA9`: Cancel
/// - Right button on `PA10`: OK
pub fn read_front_buttons() -> (bool, bool) {
    #[cfg(not(test))]
    {
        let gpioa = unsafe { &*pac::GPIOA::ptr() };
        let idr = gpioa.idr.read().bits();
        // PA9 is bit 9 (Active LOW), PA10 is bit 10 (Active LOW)
        let left = (idr & (1 << 9)) == 0;
        let right = (idr & (1 << 10)) == 0;
        (left, right)
    }

    #[cfg(test)]
    {
        (
            TEST_FRONT_LEFT.load(core::sync::atomic::Ordering::SeqCst),
            TEST_FRONT_RIGHT.load(core::sync::atomic::Ordering::SeqCst),
        )
    }
}

/// Backward-compatible alias for `read_front_buttons()`.
#[inline(always)]
pub fn read_rear_buttons() -> (bool, bool) {
    read_front_buttons()
}

/// Scan physical front buttons and merge with virtual touch keys.
///
/// Output 16-bit key bitfield:
/// - bit 0:  Roll R (virtual trim)
/// - bit 1:  Roll L (virtual trim)
/// - bit 2:  Pitch U (virtual trim)
/// - bit 3:  Pitch D (virtual trim)
/// - bit 4:  Throttle U (virtual trim)
/// - bit 5:  Throttle D (virtual trim)
/// - bit 6:  Yaw R (virtual trim)
/// - bit 7:  Yaw L (virtual trim)
/// - bit 8:  Down (touch gesture swipe down or tap down)
/// - bit 9:  Up (touch gesture swipe up or tap up)
/// - bit 10: OK / Select (PA10 Right Front button OR touch tap/swipe)
/// - bit 11: Cancel / Back (PA9 Left Front button OR touch tap/swipe)
/// - bit 12: Bind
pub fn scan_keys() -> u16 {
    let (left, right) = read_front_buttons();
    let mut result = TOUCH_NAV_KEYS.load(core::sync::atomic::Ordering::Relaxed);

    if right {
        result |= 1 << 10; // OK
    }
    if left {
        result |= 1 << 11; // Cancel
    }

    result
}

/// Check if the cold-boot DFU entry combination is held:
/// Both Left (`PA9`) and Right (`PA10`) front buttons held simultaneously at power-on.
pub fn is_dfu_combo_held() -> bool {
    let (left, right) = read_front_buttons();
    left && right
}

/// Compatibility check for DFU request from key bitfield.
/// Triggers when both Left (bit 11) and Right (bit 10) are active.
pub fn is_dfu_requested(keys: u16) -> bool {
    (keys & ((1 << 10) | (1 << 11))) == ((1 << 10) | (1 << 11))
}

/// Trigger a warm reboot into the STM32 factory ROM DFU bootloader.
///
/// Sets `0xDEADBEEF` at SRAM `0x2000_3FF0`, keeps `PB15` power latch HIGH,
/// and triggers an immediate software reset.
pub fn reboot_to_dfu() -> ! {
    crate::power::latch_on();

    #[cfg(not(test))]
    unsafe {
        core::ptr::write_volatile(DFU_FLAG_ADDR, DFU_MAGIC_FLAG);
        cortex_m::peripheral::SCB::sys_reset();
    }

    #[cfg(test)]
    {
        TEST_SRAM_FLAG.store(DFU_MAGIC_FLAG, core::sync::atomic::Ordering::SeqCst);
        panic!("simulated reboot_to_dfu");
    }
}

/// Power-on boot check for DFU entry.
///
/// Checks:
/// 1. SRAM warm-reboot flag at `0x2000_3FF0 == 0xDEADBEEF`: If set, clear flag and jump to DFU.
/// 2. Zero-disassembly cold-boot combo: If both Left (`PA9`) and Right (`PA10`) front buttons
///    are held at power-on for ~20ms, latch `PB15` HIGH and jump to DFU.
pub fn check_dfu_entry(profile: &McuProfile) {
    let _ = profile;
    // 1. Check warm-reboot DFU flag in SRAM
    #[cfg(not(test))]
    unsafe {
        if core::ptr::read_volatile(DFU_FLAG_ADDR) == DFU_MAGIC_FLAG {
            core::ptr::write_volatile(DFU_FLAG_ADDR, 0); // clear flag
            crate::power::latch_on();
            chip::enter_dfu_bootloader(profile);
        }
    }

    #[cfg(test)]
    {
        if TEST_SRAM_FLAG.load(core::sync::atomic::Ordering::SeqCst) == DFU_MAGIC_FLAG {
            TEST_SRAM_FLAG.store(0, core::sync::atomic::Ordering::SeqCst);
            return;
        }
    }

    // 2. Initialize GPIOA for front buttons
    init_keys();

    // Wait ~20ms for power rails and contacts to settle
    delay_cycles(40_000);

    // Debounce check for Left (PA9) + Right (PA10) front buttons
    let mut match_count = 0;
    for _ in 0..5 {
        if is_dfu_combo_held() {
            match_count += 1;
        }
        delay_cycles(2_000);
    }

    if match_count >= 3 {
        crate::power::latch_on();
        #[cfg(not(test))]
        chip::enter_dfu_bootloader(profile);
    }
}

#[cfg(test)]
pub fn test_set_front_buttons(left: bool, right: bool) {
    TEST_FRONT_LEFT.store(left, core::sync::atomic::Ordering::SeqCst);
    TEST_FRONT_RIGHT.store(right, core::sync::atomic::Ordering::SeqCst);
}

#[cfg(test)]
pub fn test_set_rear_buttons(left: bool, right: bool) {
    test_set_front_buttons(left, right);
}

#[cfg(test)]
pub fn test_set_sram_dfu_flag(flag: u32) {
    TEST_SRAM_FLAG.store(flag, core::sync::atomic::Ordering::SeqCst);
}

#[cfg(test)]
pub fn test_get_sram_dfu_flag() -> u32 {
    TEST_SRAM_FLAG.load(core::sync::atomic::Ordering::SeqCst)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_front_buttons_mapping() {
        init_keys();

        // Neither pressed
        test_set_front_buttons(false, false);
        assert_eq!(scan_keys() & ((1 << 10) | (1 << 11)), 0);

        // Left (Cancel) pressed
        test_set_front_buttons(true, false);
        let keys = scan_keys();
        assert_ne!(keys & (1 << 11), 0);
        assert_eq!(keys & (1 << 10), 0);

        // Right (OK) pressed
        test_set_front_buttons(false, true);
        let keys = scan_keys();
        assert_eq!(keys & (1 << 11), 0);
        assert_ne!(keys & (1 << 10), 0);

        // Both pressed (DFU combo)
        test_set_front_buttons(true, true);
        assert!(is_dfu_combo_held());
    }

    #[test]
    fn test_touch_keys_merge() {
        init_keys();
        test_set_front_buttons(false, false);

        set_touch_keys((1 << 8) | (1 << 9)); // Up and Down from touch
        let keys = scan_keys();
        assert_ne!(keys & (1 << 8), 0);
        assert_ne!(keys & (1 << 9), 0);

        // Right merges with touch keys
        test_set_front_buttons(false, true);
        let keys = scan_keys();
        assert_ne!(keys & (1 << 10), 0);
        assert_ne!(keys & (1 << 8), 0);
    }

    #[test]
    fn test_sram_flag_detection() {
        let profile = chip::get_mcu_profile();
        test_set_sram_dfu_flag(DFU_MAGIC_FLAG);
        assert_eq!(test_get_sram_dfu_flag(), DFU_MAGIC_FLAG);
        check_dfu_entry(&profile);
        assert_eq!(test_get_sram_dfu_flag(), 0); // Cleared after detection
    }
}
