//! Hardware SPI1 and RF frontend control for Amiccom A7105 on FlySky FS-i6X.
//!
//! Pin assignments:
//! - PE13: SPI1_SCK  (AF1, High Speed)
//! - PE14: SPI1_MISO (AF1)
//! - PE15: SPI1_MOSI (AF1, High Speed)
//! - PE12: A7105_CSN (GPIO Output, Active Low)
//! - PE8:  RF_RX_EN  (GPIO Output)
//! - PE9:  RF_TX_EN  (GPIO Output)
//! - PE10: RF_RF0    (Antenna diversity switch line 0)
//! - PE11: RF_RF1    (Antenna diversity switch line 1)
//! - PB2:  RF_GIO2   (GPIO Input with pull-up, Wait-for-TX/RX)
//! - TIM16: Periodic protocol timer (3.85 ms / 260 Hz)

#![allow(dead_code)]

#[cfg(not(test))]
use stm32f0xx_hal::pac;

// RF Mode constants matching OpenI6X / A7105 hardware
pub const RF_MODE_RX_EN: u8 = 0b0000_0001;
pub const RF_MODE_TX_EN: u8 = 0b0000_0010;
pub const RF_MODE_OFF: u8 = 0b0000_0011;

static mut ANTENNA_STATE: u8 = 0;

/// Initialize SPI1, frontend control GPIOs, EXTI2 on PB2, and TIM16.
pub fn init() {
    #[cfg(not(test))]
    unsafe {
        let rcc = &*pac::RCC::ptr();
        let gpiob = &*pac::GPIOB::ptr();
        let gpioe = &*pac::GPIOE::ptr();
        let syscfg = &*pac::SYSCFG::ptr();
        let exti = &*pac::EXTI::ptr();
        let spi1 = &*pac::SPI1::ptr();

        // 1. Enable peripheral clocks
        // GPIOB, GPIOE
        rcc.ahbenr.modify(|_, w| w.iopben().set_bit().iopeen().set_bit());

        // SYSCFG, SPI1, TIM16
        rcc.apb2enr.modify(|_, w| w.syscfgen().set_bit().spi1en().set_bit().tim16en().set_bit());

        // 2. Configure GPIOE pins:
        // PE13 (SCK), PE14 (MISO), PE15 (MOSI) -> Alternate Function (0b10)
        // PE12 (CSN) -> Output (0b01)
        // PE8 (RX_EN), PE9 (TX_EN) -> Output (0b01)
        // PE10 (RF0), PE11 (RF1) -> Output (0b01)
        let moder = gpioe.moder.read().bits();
        let moder_cleared = moder & !(
            (0b11 << 16) | // PE8
            (0b11 << 18) | // PE9
            (0b11 << 20) | // PE10
            (0b11 << 22) | // PE11
            (0b11 << 24) | // PE12
            (0b11 << 26) | // PE13
            (0b11 << 28) | // PE14
            (0b11 << 30)   // PE15
        );
        let moder_set = (0b01 << 16)   // PE8 Output
            | (0b01 << 18)             // PE9 Output
            | (0b01 << 20)             // PE10 Output
            | (0b01 << 22)             // PE11 Output
            | (0b01 << 24)             // PE12 Output (CSN)
            | (0b10 << 26)             // PE13 AF (SCK)
            | (0b10 << 28)             // PE14 AF (MISO)
            | (0b10 << 30);            // PE15 AF (MOSI)
        gpioe.moder.write(|w| w.bits(moder_cleared | moder_set));

        // High speed for SPI pins PE13, PE14, PE15
        gpioe.ospeedr.modify(|r, w| w.bits(r.bits() | (0b11 << 26) | (0b11 << 28) | (0b11 << 30)));

        // Set AF1 (SPI1) for PE13, PE14, PE15 in AFRH
        let afrh = gpioe.afrh.read().bits();
        let afrh_cleared = afrh & !(0x0FFF << 20);
        let afrh_set = (1 << 20) | (1 << 24) | (1 << 28); // AF1 on pins 13, 14, 15
        gpioe.afrh.write(|w| w.bits(afrh_cleared | afrh_set));

        // CSN starts HIGH (idle, deselected)
        gpioe.bsrr.write(|w| w.bs12().set_bit());

        // Turn front-end OFF initially
        set_tx_rx_mode(RF_MODE_OFF);

        // Antenna 0 selected by default (RF0=1, RF1=0)
        gpioe.bsrr.write(|w| w.bits((1 << 10) | (1 << (11 + 16))));

        // 3. Configure PB2 (GIO2 / WTR) as input with pull-up
        gpiob.moder.modify(|r, w| w.bits(r.bits() & !(0b11 << 4)));
        gpiob.pupdr.modify(|r, w| w.bits((r.bits() & !(0b11 << 4)) | (0b01 << 4)));

        // Route EXTI2 to PB2 in SYSCFG_EXTICR1
        syscfg.exticr1.modify(|r, w| w.bits((r.bits() & !(0x0F << 8)) | (0x01 << 8)));

        // Falling edge trigger for EXTI2
        exti.ftsr.modify(|_, w| w.tr2().set_bit());

        // Clear any pending EXTI2 flag
        exti.pr.write(|w| w.bits(1 << 2));

        // 4. Configure SPI1:
        // Disable first
        spi1.cr1.write(|w| w.bits(0));

        let cr1 = (1 << 2) | (1 << 4) | (1 << 8) | (1 << 9);
        spi1.cr1.write(|w| w.bits(cr1));

        // CR2: DS = 0b0111 (8-bit data), FRXTH = 1 (1/4 threshold)
        let cr2 = (0b0111 << 8) | (1 << 12);
        spi1.cr2.write(|w| w.bits(cr2));

        // Enable SPI1 (SPE = bit 6)
        spi1.cr1.write(|w| w.bits(cr1 | (1 << 6)));

        // Drain any stale data in RX FIFO (max 16 bytes)
        let mut drain = 16;
        while spi1.sr.read().rxne().bit_is_set() && drain > 0 {
            drain -= 1;
            let _ = core::ptr::read_volatile(core::ptr::addr_of!((*pac::SPI1::ptr()).dr) as *const u8);
        }
    }
}

/// Assert Chip Select (Active Low).
#[inline]
pub fn csn_low() {
    #[cfg(not(test))]
    unsafe {
        let spi1 = &*pac::SPI1::ptr();
        let gpioe = &*pac::GPIOE::ptr();
        let mut timeout = 10_000u32;
        while spi1.sr.read().bsy().bit_is_set() && timeout > 0 {
            timeout -= 1;
        }
        gpioe.bsrr.write(|w| w.br12().set_bit());
        cortex_m::asm::nop();
        cortex_m::asm::nop();
    }
}

/// Deassert Chip Select (High).
#[inline]
pub fn csn_high() {
    #[cfg(not(test))]
    unsafe {
        let spi1 = &*pac::SPI1::ptr();
        let gpioe = &*pac::GPIOE::ptr();
        let mut timeout = 10_000u32;
        while spi1.sr.read().bsy().bit_is_set() && timeout > 0 {
            timeout -= 1;
        }
        cortex_m::asm::nop();
        cortex_m::asm::nop();
        gpioe.bsrr.write(|w| w.bs12().set_bit());
        cortex_m::asm::nop();
        cortex_m::asm::nop();
    }
}

/// Transfer a single byte over SPI1 (Full Duplex).
#[inline]
pub fn transfer_byte(data: u8) -> u8 {
    #[cfg(not(test))]
    unsafe {
        let spi1 = &*pac::SPI1::ptr();
        let mut timeout = 10_000u32;
        while spi1.sr.read().bsy().bit_is_set() && timeout > 0 {
            timeout -= 1;
        }
        timeout = 10_000;
        while spi1.sr.read().txe().bit_is_clear() && timeout > 0 {
            timeout -= 1;
        }
        core::ptr::write_volatile(core::ptr::addr_of!((*pac::SPI1::ptr()).dr) as *mut u8, data);
        timeout = 10_000;
        while spi1.sr.read().rxne().bit_is_clear() && timeout > 0 {
            timeout -= 1;
        }
        core::ptr::read_volatile(core::ptr::addr_of!((*pac::SPI1::ptr()).dr) as *const u8)
    }
    #[cfg(test)]
    {
        let _ = data;
        0
    }
}

/// Write a single byte over SPI1 (discards read byte).
#[inline]
pub fn write_byte(data: u8) {
    let _ = transfer_byte(data);
}

/// Read a single byte over SPI1 (sends dummy 0x00).
#[inline]
pub fn read_byte() -> u8 {
    transfer_byte(0x00)
}

/// Set front-end PA/LNA mode matching OpenI6X hardware mapping:
/// - RF_MODE_TX_EN: PE8 = 1, PE9 = 0
/// - RF_MODE_RX_EN: PE8 = 0, PE9 = 1
/// - RF_MODE_OFF:   PE8 = 1, PE9 = 1 (or 0, 0)
pub fn set_tx_rx_mode(mode: u8) {
    let tmp = ((mode << 1) & 0x02) | ((mode >> 1) & 0x01);
    #[cfg(not(test))]
    unsafe {
        let val = (0x0300u32 << 16) | ((tmp as u32) << 8);
        let gpioe = &*pac::GPIOE::ptr();
        gpioe.bsrr.write(|w| w.bits(val));
    }
    #[cfg(test)]
    let _ = tmp;
}

/// Alternate antenna diversity switch between Antenna 0 and Antenna 1.
pub fn switch_antenna() {
    #[cfg(not(test))]
    unsafe {
        let gpioe = &*pac::GPIOE::ptr();
        if ANTENNA_STATE == 0 {
            // Select Antenna 0: PE10 = 1, PE11 = 0
            gpioe.bsrr.write(|w| w.bits((1 << 10) | (1 << (11 + 16))));
            ANTENNA_STATE = 1;
        } else {
            // Select Antenna 1: PE10 = 0, PE11 = 1
            gpioe.bsrr.write(|w| w.bits((1 << (10 + 16)) | (1 << 11)));
            ANTENNA_STATE = 0;
        }
    }
}

/// Enable EXTI2 interrupt line (GIO2 falling edge).
#[inline(always)]
pub fn enable_gio2_irq() {
    #[cfg(not(test))]
    unsafe {
        let exti = &*pac::EXTI::ptr();
        // Clear pending bit first
        exti.pr.write(|w| w.bits(1 << 2));
        exti.imr.modify(|_, w| w.mr2().set_bit());
    }
}

/// Disable EXTI2 interrupt line.
#[inline(always)]
pub fn disable_gio2_irq() {
    #[cfg(not(test))]
    unsafe {
        let exti = &*pac::EXTI::ptr();
        exti.imr.modify(|_, w| w.mr2().clear_bit());
    }
}

/// Initialize TIM16 for periodic packet generation (e.g. 3850 µs).
/// Assumes 48 MHz core clock (PSC = 47 gives 1 µs tick).
pub fn init_tim16(period_us: u16) {
    #[cfg(not(test))]
    unsafe {
        let tim16 = &*pac::TIM16::ptr();

        // Stop timer
        tim16.cr1.write(|w| w.bits(0));

        // PSC = 47 -> 48 MHz / (47 + 1) = 1 MHz = 1.0 µs per tick
        tim16.psc.write(|w| w.psc().bits(47));
        // ARR = period_us - 1 (e.g. 3850 - 1 = 3849 for 3.850 ms period)
        tim16.arr.write(|w| w.arr().bits(period_us.saturating_sub(1)));

        // Clear update interrupt flag
        tim16.sr.write(|w| w.bits(0));

        // Enable Update Interrupt (UIE = bit 0)
        tim16.dier.write(|w| w.uie().set_bit());

        // Set priority for EXTI2_3 (IRQ 6) and TIM16 (IRQ 21) in NVIC
        let mut cp = cortex_m::peripheral::Peripherals::steal();
        cp.NVIC.set_priority(pac::Interrupt::EXTI2_3, 0x80);
        cp.NVIC.set_priority(pac::Interrupt::TIM16, 0x80);

        // Enable IRQ 6 (EXTI2_3) and IRQ 21 (TIM16)
        cortex_m::peripheral::NVIC::unmask(pac::Interrupt::EXTI2_3);
        cortex_m::peripheral::NVIC::unmask(pac::Interrupt::TIM16);

        // Start timer (CEN = bit 0)
        tim16.cr1.write(|w| w.cen().set_bit());
    }
    #[cfg(test)]
    let _ = period_us;
}

/// Clear TIM16 update interrupt flag.
#[inline(always)]
pub fn clear_tim16_flag() {
    #[cfg(not(test))]
    unsafe {
        let tim16 = &*pac::TIM16::ptr();
        tim16.sr.write(|w| w.bits(0));
    }
}
