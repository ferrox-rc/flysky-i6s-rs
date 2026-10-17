//! Chip identification, Silicon Unique ID (UID) extraction, and DFU bootloader jump.

use core::ptr;

/// Supported MCU types.
#[allow(dead_code)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum McuType {
    Stm32f072,
    Apm32f072,
}

/// MCU-specific memory map profile.
#[allow(dead_code)]
pub struct McuProfile {
    pub mcu_type: McuType,
    pub name: &'static str,
    pub uid_addr: usize,
    pub dfu_bootloader_addr: usize,
}

/// STM32F072 memory layout definitions.
pub const STM32_PROFILE: McuProfile = McuProfile {
    mcu_type: McuType::Stm32f072,
    name: "STM32F072VB",
    uid_addr: 0x1FFF_F7AC,
    dfu_bootloader_addr: 0x1FFF_C800,
};

/// APM32F072 (Geehy) memory layout definitions.
#[allow(dead_code)]
pub const APM32_PROFILE: McuProfile = McuProfile {
    mcu_type: McuType::Apm32f072,
    name: "APM32F072VB",
    uid_addr: 0x1FFF_F7E8,
    dfu_bootloader_addr: 0x1FFF_F000,
};

/// Detect the active MCU profile.
pub fn get_mcu_profile() -> McuProfile {
    #[cfg(feature = "apm32")]
    {
        APM32_PROFILE
    }
    #[cfg(not(feature = "apm32"))]
    {
        STM32_PROFILE
    }
}

/// Read the 96-bit (12-byte) Unique Device ID from MCU silicon.
#[allow(dead_code)]
pub fn read_uid(profile: &McuProfile) -> [u8; 12] {
    let mut uid = [0u8; 12];
    unsafe {
        let src = profile.uid_addr as *const u8;
        for (i, byte) in uid.iter_mut().enumerate() {
            *byte = ptr::read_volatile(src.add(i));
        }
    }
    uid
}

/// Initialize system clock to 48 MHz using external 8 MHz crystal (HSE) + PLL.
/// Matches OpenI6X SetSysClock configuration.
pub fn init_system_clock() {
    let rcc = unsafe { &*stm32f0xx_hal::pac::RCC::ptr() };
    let flash = unsafe { &*stm32f0xx_hal::pac::FLASH::ptr() };

    unsafe {
        // 1. Enable HSE (8 MHz external crystal on FlySky FS-i6X PCB)
        rcc.cr.modify(|_, w| w.hseon().set_bit());

        // 2. Wait for HSERDY with timeout
        let mut timeout = 100_000u32;
        while !rcc.cr.read().hserdy().bit() && timeout > 0 {
            timeout -= 1;
        }

        if timeout > 0 {
            // 3. Set Flash latency to 1 wait state (required for 48 MHz) & enable prefetch buffer
            flash.acr.write(|w| w.latency().ws1().prftbe().enabled());

            // 4. Configure PLL: HSE / 1 * 6 = 48 MHz
            // PLLSRC = HSE PREDIV (bit 16), PLLMUL = 6 (0b0100 << 18)
            let cfgr = rcc.cfgr.read().bits();
            let cfgr_clean = cfgr & !(0x003F_0000 | 0x0000_0003);
            rcc.cfgr.write(|w| w.bits(cfgr_clean | (1 << 16) | 0x0010_0000));

            // 5. Enable PLL (PLLON = bit 24)
            rcc.cr.modify(|_, w| w.pllon().set_bit());

            // 6. Wait for PLLRDY (bit 25)
            timeout = 100_000;
            while !rcc.cr.read().pllrdy().bit() && timeout > 0 {
                timeout -= 1;
            }

            // 7. Select PLL as system clock source (SW = 0b10)
            let cfgr = rcc.cfgr.read().bits();
            rcc.cfgr.write(|w| w.bits((cfgr & !0x03) | 0x02));

            // 8. Wait for PLL to be used as system clock source (SWS = 0b1000)
            timeout = 100_000;
            while (rcc.cfgr.read().bits() & 0x0C) != 0x08 && timeout > 0 {
                timeout -= 1;
            }

            // 9. Select PLL (48 MHz) as USB clock source (USBSW = 1 in RCC_CFGR3 bit 7)
            rcc.cfgr3.modify(|_, w| w.usbsw().pllclk());
        }

        // 10. Enable LSI (40 kHz internal oscillator) and wait for stability
        rcc.csr.modify(|_, w| w.lsion().set_bit());
        let mut lsi_timeout = 100_000u32;
        while !rcc.csr.read().lsirdy().bit() && lsi_timeout > 0 {
            lsi_timeout -= 1;
        }
    }
}

/// Check if the MCU was rebooted warm by the hardware watchdog (IWDG or WWDG)
/// or a software reset (e.g. panic recovery via SCB::sys_reset), and clear
/// the reset status flags in RCC_CSR.
///
/// Under DO-178C and flight safety principles, a warm reboot caused by watchdog timeout
/// or panic fast-reset must bypass the throttle-at-idle and switch-safety interlocks to recover
/// flight control in under 10 ms instead of locking the pilot in a warning screen.
pub fn check_and_clear_reset_flags() -> bool {
    let rcc = unsafe { &*stm32f0xx_hal::pac::RCC::ptr() };
    let csr = rcc.csr.read();
    let was_warm_reset =
        csr.iwdgrstf().bit_is_set() || csr.wwdgrstf().bit_is_set() || csr.sftrstf().bit_is_set();

    // Clear all reset source flags by setting RMVF
    rcc.csr.modify(|_, w| w.rmvf().set_bit());

    was_warm_reset
}

/// Reset RCC to default power-on state (HSI 8MHz, no PLL, peripheral clocks disabled).
unsafe fn rcc_deinit() {
    let rcc = &*stm32f0xx_hal::pac::RCC::ptr();

    // Turn on HSI
    rcc.cr.modify(|_, w| w.hsion().set_bit());
    while rcc.cr.read().hsirdy().bit_is_clear() {
        // Wait for HSIRDY
    }

    // Reset CFGR
    rcc.cfgr.write(|w| w.bits(0x0000_0000));

    // Reset HSEON, CSSON, PLLON
    rcc.cr.modify(|_, w| w.hseon().clear_bit().csson().clear_bit().pllon().clear_bit());

    // Reset CIR interrupts
    rcc.cir.write(|w| w.bits(0x0000_0000));

    // Reset APB peripherals
    rcc.apb2rstr.write(|w| w.bits(0xFFFF_FFFF));
    rcc.apb2rstr.write(|w| w.bits(0x0000_0000));
    rcc.apb1rstr.write(|w| w.bits(0xFFFF_FFFF));
    rcc.apb1rstr.write(|w| w.bits(0x0000_0000));

    // Reset peripheral clocks except SYSCFG & SRAM
    rcc.ahbenr.write(|w| w.bits(0x0000_0014)); // SRAMEN + FLITFEN
    rcc.apb2enr.write(|w| w.bits(0x0000_0001)); // SYSCFGEN
    rcc.apb1enr.write(|w| w.bits(0x0000_0000));
}

/// Perform a clean software jump to the MCU's built-in factory DFU bootloader.
///
/// Matches the exact OpenI6X SystemBootloaderJump() sequence:
/// 1. Reset RCC to default state
/// 2. Clear SysTick timer
/// 3. Remap System Memory to 0x0000_0000 via SYSCFG
/// 4. Re-enable interrupts so the factory bootloader's USB stack functions!
/// 5. Bootstrap into the bootloader reset vector.
pub fn enter_dfu_bootloader(profile: &McuProfile) -> ! {
    unsafe {
        // 1. Reset RCC to default HSI 8MHz state
        rcc_deinit();

        // 2. Clear SysTick and 3. Clear NVIC interrupt enables and pendings
        let mut cp = cortex_m::peripheral::Peripherals::steal();
        cp.SYST.disable_counter();
        cp.SYST.disable_interrupt();
        cp.SYST.clear_current();

        cp.NVIC.icer[0].write(0xFFFF_FFFF);
        cp.NVIC.icpr[0].write(0xFFFF_FFFF);

        cortex_m::interrupt::disable();

        cortex_m::asm::dsb();

        // 4. Remap System Memory to 0x0000_0000 via SYSCFG
        let syscfg = &*stm32f0xx_hal::pac::SYSCFG::ptr();
        syscfg.cfgr1.modify(|r, w| w.bits((r.bits() & !0x03) | 0x01));

        cortex_m::asm::dsb();
        cortex_m::asm::isb();

        // 5. Read initial MSP and reset handler from the bootloader base address
        let bootloader_addr = profile.dfu_bootloader_addr;
        let initial_msp = ptr::read_volatile(bootloader_addr as *const u32);
        let reset_handler_addr = ptr::read_volatile((bootloader_addr + 4) as *const u32);

        // 6. CRITICAL: Re-enable interrupts! The ST USB DFU ROM bootloader requires IRQs!
        cortex_m::interrupt::enable();

        // 7. Bootstrap into the bootloader
        cortex_m::asm::bootstrap(initial_msp as *const u32, reset_handler_addr as *const u32);
    }
}
