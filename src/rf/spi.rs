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

use core::ptr;

// RCC registers
const RCC_AHBENR: *mut u32 = 0x4002_1014 as *mut u32;
const RCC_APB2ENR: *mut u32 = 0x4002_1018 as *mut u32;

// GPIO registers
const GPIOB_MODER: *mut u32 = 0x4800_0400 as *mut u32;
const GPIOB_PUPDR: *mut u32 = 0x4800_040C as *mut u32;

const GPIOE_MODER: *mut u32 = 0x4800_1000 as *mut u32;
const GPIOE_OSPEEDR: *mut u32 = 0x4800_1008 as *mut u32;
const GPIOE_BSRR: *mut u32 = 0x4800_1018 as *mut u32;
const GPIOE_AFRH: *mut u32 = 0x4800_1024 as *mut u32;

// SPI1 registers (Base: 0x4001_3000)
const SPI1_CR1: *mut u32 = 0x4001_3000 as *mut u32;
const SPI1_CR2: *mut u32 = 0x4001_3004 as *mut u32;
const SPI1_SR: *mut u32 = 0x4001_3008 as *mut u32;
const SPI1_DR: *mut u32 = 0x4001_300C as *mut u32;

// SYSCFG & EXTI registers
const SYSCFG_EXTICR1: *mut u32 = 0x4001_0008 as *mut u32;
const EXTI_IMR: *mut u32 = 0x4001_0400 as *mut u32;
const EXTI_FTSR: *mut u32 = 0x4001_040C as *mut u32;
const EXTI_PR: *mut u32 = 0x4001_0414 as *mut u32;

// TIM16 registers (Base: 0x4001_4400)
const TIM16_CR1: *mut u32 = 0x4001_4400 as *mut u32;
const TIM16_SMCR: *mut u32 = 0x4001_4408 as *mut u32;
const TIM16_DIER: *mut u32 = 0x4001_440C as *mut u32;
const TIM16_SR: *mut u32 = 0x4001_4410 as *mut u32;
const TIM16_PSC: *mut u32 = 0x4001_4428 as *mut u32;
const TIM16_ARR: *mut u32 = 0x4001_442C as *mut u32;

// NVIC registers
const NVIC_ISER: *mut u32 = 0xE000_E100 as *mut u32;
const NVIC_IPR1: *mut u32 = 0xE000_E404 as *mut u32; // IRQ 4..7 (EXTI2_3 = IRQ 6)
const NVIC_IPR5: *mut u32 = 0xE000_E414 as *mut u32; // IRQ 20..23 (TIM16 = IRQ 21)

// RF Mode constants matching OpenI6X / A7105 hardware
pub const RF_MODE_RX_EN: u8 = 0b0000_0001;
pub const RF_MODE_TX_EN: u8 = 0b0000_0010;
pub const RF_MODE_OFF: u8 = 0b0000_0011;

static mut ANTENNA_STATE: u8 = 0;

/// Initialize SPI1, frontend control GPIOs, EXTI2 on PB2, and TIM16.
pub fn init() {
    unsafe {
        // 1. Enable peripheral clocks
        // GPIOB (bit 18), GPIOE (bit 21)
        let ahbenr = ptr::read_volatile(RCC_AHBENR);
        ptr::write_volatile(RCC_AHBENR, ahbenr | (1 << 18) | (1 << 21));

        // SYSCFG (bit 0), SPI1 (bit 12), TIM16 (bit 17)
        let apb2enr = ptr::read_volatile(RCC_APB2ENR);
        ptr::write_volatile(RCC_APB2ENR, apb2enr | (1 << 0) | (1 << 12) | (1 << 17));

        // 2. Configure GPIOE pins:
        // PE13 (SCK), PE14 (MISO), PE15 (MOSI) -> Alternate Function (0b10)
        // PE12 (CSN) -> Output (0b01)
        // PE8 (RX_EN), PE9 (TX_EN) -> Output (0b01)
        // PE10 (RF0), PE11 (RF1) -> Output (0b01)
        let moder = ptr::read_volatile(GPIOE_MODER);
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
        ptr::write_volatile(GPIOE_MODER, moder_cleared | moder_set);

        // High speed for SPI pins PE13, PE14, PE15
        let ospeedr = ptr::read_volatile(GPIOE_OSPEEDR);
        ptr::write_volatile(
            GPIOE_OSPEEDR,
            ospeedr | (0b11 << 26) | (0b11 << 28) | (0b11 << 30),
        );

        // Set AF1 (SPI1) for PE13, PE14, PE15 in AFRH
        // PE13 -> AFRH bits [23:20], PE14 -> [27:24], PE15 -> [31:28]
        let afrh = ptr::read_volatile(GPIOE_AFRH);
        let afrh_cleared = afrh & !(0x0FFF << 20);
        let afrh_set = (1 << 20) | (1 << 24) | (1 << 28); // AF1 on pins 13, 14, 15
        ptr::write_volatile(GPIOE_AFRH, afrh_cleared | afrh_set);

        // CSN starts HIGH (idle, deselected)
        ptr::write_volatile(GPIOE_BSRR, 1 << 12);

        // Turn front-end OFF initially
        set_tx_rx_mode(RF_MODE_OFF);

        // Antenna 0 selected by default (RF0=1, RF1=0)
        ptr::write_volatile(GPIOE_BSRR, (1 << 10) | (1 << (11 + 16)));

        // 3. Configure PB2 (GIO2 / WTR) as input with pull-up
        let pb_moder = ptr::read_volatile(GPIOB_MODER);
        ptr::write_volatile(GPIOB_MODER, pb_moder & !(0b11 << 4));
        let pb_pupdr = ptr::read_volatile(GPIOB_PUPDR);
        ptr::write_volatile(GPIOB_PUPDR, (pb_pupdr & !(0b11 << 4)) | (0b01 << 4));

        // Route EXTI2 to PB2 in SYSCFG_EXTICR1 (bits [11:8] = 0b0001 for Port B)
        let exticr = ptr::read_volatile(SYSCFG_EXTICR1);
        ptr::write_volatile(SYSCFG_EXTICR1, (exticr & !(0x0F << 8)) | (0x01 << 8));

        // Falling edge trigger for EXTI2
        let ftsr = ptr::read_volatile(EXTI_FTSR);
        ptr::write_volatile(EXTI_FTSR, ftsr | (1 << 2));

        // Clear any pending EXTI2 flag
        ptr::write_volatile(EXTI_PR, 1 << 2);

        // 4. Configure SPI1:
        // Disable first
        ptr::write_volatile(SPI1_CR1, 0);

        // CR1:
        // MSTR = 1 (bit 2)
        // BR_1 = 1 (bit 4) -> fPCLK / 8 (1 MHz at 8 MHz, matching OpenI6X SPI_CR1_BR_1)
        // SSI = 1 (bit 8), SSM = 1 (bit 9) -> Software NSS management
        // Mode 0: CPOL = 0 (bit 1), CPHA = 0 (bit 0)
        let cr1 = (1 << 2) | (1 << 4) | (1 << 8) | (1 << 9);
        ptr::write_volatile(SPI1_CR1, cr1);

        // CR2:
        // DS = 0b0111 (bits 11:8) -> 8-bit data size
        // FRXTH = 1 (bit 12) -> FIFO reception threshold 1/4 (8-bit)
        let cr2 = (0b0111 << 8) | (1 << 12);
        ptr::write_volatile(SPI1_CR2, cr2);

        // Enable SPI1
        ptr::write_volatile(SPI1_CR1, cr1 | (1 << 6)); // SPE = bit 6

        // Drain any stale data in RX FIFO (max 16 bytes)
        let mut drain = 16;
        while (ptr::read_volatile(SPI1_SR) & (1 << 0)) != 0 && drain > 0 {
            drain -= 1;
            let _ = ptr::read_volatile(SPI1_DR as *const u8);
        }
    }
}

/// Assert Chip Select (Active Low).
#[inline]
pub fn csn_low() {
    unsafe {
        let mut timeout = 10_000u32;
        while (ptr::read_volatile(SPI1_SR) & (1 << 7)) != 0 && timeout > 0 {
            timeout -= 1;
        }
        ptr::write_volatile(GPIOE_BSRR, 1 << (12 + 16));
        cortex_m::asm::nop();
        cortex_m::asm::nop();
    }
}

/// Deassert Chip Select (High).
#[inline]
pub fn csn_high() {
    unsafe {
        let mut timeout = 10_000u32;
        while (ptr::read_volatile(SPI1_SR) & (1 << 7)) != 0 && timeout > 0 {
            timeout -= 1;
        }
        cortex_m::asm::nop();
        cortex_m::asm::nop();
        ptr::write_volatile(GPIOE_BSRR, 1 << 12);
        cortex_m::asm::nop();
        cortex_m::asm::nop();
    }
}

/// Transfer a single byte over SPI1 (Full Duplex).
#[inline]
pub fn transfer_byte(data: u8) -> u8 {
    unsafe {
        let mut timeout = 10_000u32;
        while (ptr::read_volatile(SPI1_SR) & (1 << 7)) != 0 && timeout > 0 {
            timeout -= 1;
        }
        timeout = 10_000;
        while (ptr::read_volatile(SPI1_SR) & (1 << 1)) == 0 && timeout > 0 {
            timeout -= 1;
        }
        ptr::write_volatile(SPI1_DR as *mut u8, data);
        timeout = 10_000;
        while (ptr::read_volatile(SPI1_SR) & (1 << 0)) == 0 && timeout > 0 {
            timeout -= 1;
        }
        ptr::read_volatile(SPI1_DR as *const u8)
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
    unsafe {
        let val = (0x0300u32 << 16) | ((tmp as u32) << 8);
        ptr::write_volatile(GPIOE_BSRR, val);
    }
}

/// Alternate antenna diversity switch between Antenna 0 and Antenna 1.
pub fn switch_antenna() {
    unsafe {
        if ANTENNA_STATE == 0 {
            // Select Antenna 0: PE10 = 1, PE11 = 0
            ptr::write_volatile(GPIOE_BSRR, (1 << 10) | (1 << (11 + 16)));
            ANTENNA_STATE = 1;
        } else {
            // Select Antenna 1: PE10 = 0, PE11 = 1
            ptr::write_volatile(GPIOE_BSRR, (1 << (10 + 16)) | (1 << 11));
            ANTENNA_STATE = 0;
        }
    }
}

/// Enable EXTI2 interrupt line (GIO2 falling edge).
#[inline(always)]
pub fn enable_gio2_irq() {
    unsafe {
        // Clear pending bit first
        ptr::write_volatile(EXTI_PR, 1 << 2);
        let imr = ptr::read_volatile(EXTI_IMR);
        ptr::write_volatile(EXTI_IMR, imr | (1 << 2));
    }
}

/// Disable EXTI2 interrupt line.
#[inline(always)]
pub fn disable_gio2_irq() {
    unsafe {
        let imr = ptr::read_volatile(EXTI_IMR);
        ptr::write_volatile(EXTI_IMR, imr & !(1 << 2));
    }
}

/// Initialize TIM16 for periodic packet generation (e.g. 3850 µs).
/// Assumes 48 MHz core clock (PSC = 47 gives 1 µs tick).
pub fn init_tim16(period_us: u16) {
    unsafe {
        // Stop timer and disable preload
        ptr::write_volatile(TIM16_CR1, 0);
        ptr::write_volatile(TIM16_SMCR, 0);

        // PSC = 47 -> 48 MHz / (47 + 1) = 1 MHz = 1.0 µs per tick
        ptr::write_volatile(TIM16_PSC, 47);
        // ARR = period_us - 1 (e.g. 3850 - 1 = 3849 for 3.850 ms period)
        ptr::write_volatile(TIM16_ARR, (period_us as u32).saturating_sub(1));

        // Clear update interrupt flag
        ptr::write_volatile(TIM16_SR, 0);

        // Enable Update Interrupt (UIE = bit 0)
        ptr::write_volatile(TIM16_DIER, 1 << 0);

        // Set priority for EXTI2_3 (IRQ 6) and TIM16 (IRQ 21) in NVIC
        let ipr1 = ptr::read_volatile(NVIC_IPR1);
        ptr::write_volatile(NVIC_IPR1, (ipr1 & !(0xFF << 16)) | (0x80 << 16)); // IRQ 6 = byte 2

        let ipr5 = ptr::read_volatile(NVIC_IPR5);
        ptr::write_volatile(NVIC_IPR5, (ipr5 & !(0xFF << 8)) | (0x80 << 8));   // IRQ 21 = byte 1

        // Enable IRQ 6 (EXTI2_3) and IRQ 21 (TIM16) in NVIC_ISER
        ptr::write_volatile(NVIC_ISER, (1 << 6) | (1 << 21));

        // Start timer (CEN = bit 0)
        ptr::write_volatile(TIM16_CR1, 1);
    }
}

/// Clear TIM16 update interrupt flag.
#[inline(always)]
pub fn clear_tim16_flag() {
    unsafe {
        ptr::write_volatile(TIM16_SR, 0);
    }
}
