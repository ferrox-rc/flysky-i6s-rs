//! Hardware USART1 driver (PB6/PB7 on J15) and PF6 power switch control (H1) for CRSF / ExpressLRS.
//!
//! Pin mapping:
//! - PB6: USART1_TX (AF0, Push-Pull, High Speed; bi-directional half-duplex in single-wire mode)
//! - PB7: USART1_RX (AF0, Pull-Up, High Speed in full-duplex mode)
//! - PF6: Module Power Switch on H1 (GPIO Output, Push-Pull; Right pin PF6, Left pin GND)

#![allow(dead_code)]

use core::ptr;

// RCC registers
const RCC_AHBENR: *mut u32 = 0x4002_1014 as *mut u32;
const RCC_APB2ENR: *mut u32 = 0x4002_1018 as *mut u32;

// GPIOB registers (Base: 0x4800_0400)
const GPIOB_MODER: *mut u32 = 0x4800_0400 as *mut u32;
const GPIOB_OTYPER: *mut u32 = 0x4800_0404 as *mut u32;
const GPIOB_OSPEEDR: *mut u32 = 0x4800_0408 as *mut u32;
const GPIOB_PUPDR: *mut u32 = 0x4800_040C as *mut u32;
const GPIOB_AFRL: *mut u32 = 0x4800_0420 as *mut u32;

// GPIOF registers (Base: 0x4800_1400)
const GPIOF_MODER: *mut u32 = 0x4800_1400 as *mut u32;
const GPIOF_BSRR: *mut u32 = 0x4800_1418 as *mut u32;

// USART1 registers (Base: 0x4001_3800)
const USART1_CR1: *mut u32 = 0x4001_3800 as *mut u32;
const USART1_CR2: *mut u32 = 0x4001_3804 as *mut u32;
const USART1_CR3: *mut u32 = 0x4001_3808 as *mut u32;
const USART1_BRR: *mut u32 = 0x4001_380C as *mut u32;
const USART1_ISR: *mut u32 = 0x4001_381C as *mut u32;
const USART1_ICR: *mut u32 = 0x4001_3820 as *mut u32;
const USART1_RDR: *mut u32 = 0x4001_3824 as *mut u32;
const USART1_TDR: *mut u32 = 0x4001_3828 as *mut u32;

// Status register flags
const USART_ISR_TXE: u32 = 1 << 7;
const USART_ISR_TC: u32 = 1 << 6;
const USART_ISR_RXNE: u32 = 1 << 5;
const USART_ISR_ORE: u32 = 1 << 3;

// NVIC registers
#[cfg(not(test))]
const NVIC_ICPR: *mut u32 = 0xE000_E280 as *mut u32;
#[cfg(not(test))]
const NVIC_IPR6: *mut u32 = 0xE000_E418 as *mut u32; // IRQ 27 is byte 3 of IPR6

#[cfg(not(test))]
use stm32f0xx_hal::pac::interrupt;

use core::sync::atomic::{AtomicBool, Ordering};
#[cfg(not(test))]
use core::sync::atomic::AtomicUsize;

#[cfg(not(test))]
static mut RX_RING: [u8; 128] = [0; 128];
#[cfg(not(test))]
static RX_HEAD: AtomicUsize = AtomicUsize::new(0);
#[cfg(not(test))]
static RX_TAIL: AtomicUsize = AtomicUsize::new(0);

static ACTIVE_HIGH: AtomicBool = AtomicBool::new(true);
static POWER_ON: AtomicBool = AtomicBool::new(false);

/// Apply current PF6 pin state based on power state and active polarity.
unsafe fn apply_power_pin() {
    let pin_high = if ACTIVE_HIGH.load(Ordering::Relaxed) {
        POWER_ON.load(Ordering::Relaxed)
    } else {
        !POWER_ON.load(Ordering::Relaxed)
    };
    if pin_high {
        ptr::write_volatile(GPIOF_BSRR, 1 << 6); // High
    } else {
        ptr::write_volatile(GPIOF_BSRR, 1 << (6 + 16)); // Low
    }
}

#[cfg(test)]
pub mod mock {
    extern crate std;
    use std::sync::Mutex;
    use std::vec::Vec;

    static RX_QUEUE: Mutex<Vec<u8>> = Mutex::new(Vec::new());
    static TX_LOG: Mutex<Vec<Vec<u8>>> = Mutex::new(Vec::new());

    pub fn push_rx_bytes(bytes: &[u8]) {
        let mut q = RX_QUEUE.lock().unwrap();
        q.extend_from_slice(bytes);
    }

    pub fn pop_rx_byte() -> Option<u8> {
        let mut q = RX_QUEUE.lock().unwrap();
        if q.is_empty() {
            None
        } else {
            Some(q.remove(0))
        }
    }

    pub fn record_tx(data: &[u8]) {
        let mut log = TX_LOG.lock().unwrap();
        log.push(data.to_vec());
    }

    pub fn take_tx() -> Vec<Vec<u8>> {
        let mut log = TX_LOG.lock().unwrap();
        std::mem::take(&mut *log)
    }

    pub fn tx_count() -> usize {
        TX_LOG.lock().unwrap().len()
    }

    pub fn clear() {
        RX_QUEUE.lock().unwrap().clear();
        TX_LOG.lock().unwrap().clear();
    }
}

/// Initialize GPIO pins: PF6 as power switch (initially OFF), PB6 and PB7 peripheral clocks.
pub fn init(active_high: bool) {
    #[cfg(not(test))]
    unsafe {
        ACTIVE_HIGH.store(active_high, Ordering::Relaxed);
        POWER_ON.store(false, Ordering::Relaxed);

        // Enable GPIOB (bit 18), GPIOF (bit 22) clocks in RCC_AHBENR
        let ahb = ptr::read_volatile(RCC_AHBENR);
        ptr::write_volatile(RCC_AHBENR, ahb | (1 << 18) | (1 << 22));

        // Configure PF6 as output (MODER bits 13:12 = 01)
        let f_moder = ptr::read_volatile(GPIOF_MODER);
        ptr::write_volatile(GPIOF_MODER, (f_moder & !(3 << 12)) | (1 << 12));

        // Apply initial OFF state according to polarity
        apply_power_pin();
    }
    #[cfg(test)]
    let _ = active_high;
}

/// Set active power polarity for PF6 (H1): true = Active HIGH (N-type), false = Active LOW (P-type).
pub fn set_power_polarity(active_high: bool) {
    #[cfg(not(test))]
    unsafe {
        ACTIVE_HIGH.store(active_high, Ordering::Relaxed);
        apply_power_pin();
    }
    #[cfg(test)]
    let _ = active_high;
}

/// Set external module power state via PF6 (H1 right pin)
pub fn set_module_power(power_on: bool) {
    #[cfg(not(test))]
    unsafe {
        POWER_ON.store(power_on, Ordering::Relaxed);
        apply_power_pin();
    }
    #[cfg(test)]
    let _ = power_on;
}

/// Baud rate divisors for 48.000 MHz clock tree
pub fn get_brr_for_baud(baud_idx: u8) -> u32 {
    match baud_idx {
        0 => 114, // 420,000 bps (48,000,000 / 420,000 = 114.285)
        1 => 115, // 416,666 bps (48,000,000 / 416,666.67 = 115.20)
        2 => 417, // 115,200 bps (48,000,000 / 115,200 = 416.66)
        3 => 52,  // 921,600 bps (48,000,000 / 921,600 = 52.08)
        _ => 114,
    }
}

/// Configure and enable/disable USART1 (full-duplex or single-wire half-duplex on PB6)
pub fn set_uart_enabled(enabled: bool, baud_idx: u8, half_duplex: bool) {
    #[cfg(not(test))]
    unsafe {
        if enabled {
            // 1. Enable USART1 clock in RCC_APB2ENR (bit 14)
            let apb2 = ptr::read_volatile(RCC_APB2ENR);
            ptr::write_volatile(RCC_APB2ENR, apb2 | (1 << 14));

            // Ensure GPIOB clock is enabled (bit 18)
            let ahb = ptr::read_volatile(RCC_AHBENR);
            ptr::write_volatile(RCC_AHBENR, ahb | (1 << 18));

            // 2. Configure PB6 as AF0 (USART1_TX): MODER=10 (bits 13:12), OSPEEDR=11 (bits 13:12), AFRL=0000 (bits 27:24)
            let b_moder = ptr::read_volatile(GPIOB_MODER);
            ptr::write_volatile(GPIOB_MODER, (b_moder & !(3 << 12)) | (2 << 12));
            let b_ospeedr = ptr::read_volatile(GPIOB_OSPEEDR);
            ptr::write_volatile(GPIOB_OSPEEDR, b_ospeedr | (3 << 12)); // High speed
            let b_afrl = ptr::read_volatile(GPIOB_AFRL);
            ptr::write_volatile(GPIOB_AFRL, b_afrl & !(0xF << 24)); // AF0

            if half_duplex {
                // In half-duplex (single-wire on PB6):
                // Configure PB6 as Open-Drain with internal Pull-Up (OTYPER bit 6 = 1, PUPDR bits 13:12 = 01)
                let b_otyper = ptr::read_volatile(GPIOB_OTYPER);
                ptr::write_volatile(GPIOB_OTYPER, b_otyper | (1 << 6));
                let b_pupdr = ptr::read_volatile(GPIOB_PUPDR);
                ptr::write_volatile(GPIOB_PUPDR, (b_pupdr & !(3 << 12)) | (1 << 12));

                // Leave PB7 as general floating input
                let b_moder2 = ptr::read_volatile(GPIOB_MODER);
                ptr::write_volatile(GPIOB_MODER, b_moder2 & !(3 << 14));
            } else {
                // In full-duplex:
                // PB6 is Push-Pull (OTYPER bit 6 = 0)
                let b_otyper = ptr::read_volatile(GPIOB_OTYPER);
                ptr::write_volatile(GPIOB_OTYPER, b_otyper & !(1 << 6));

                // 3. Configure PB7 as AF0 (USART1_RX): MODER=10 (bits 15:14), PUPDR=01 (bits 15:14 pull-up), AFRL=0000 (bits 31:28)
                let b_moder2 = ptr::read_volatile(GPIOB_MODER);
                ptr::write_volatile(GPIOB_MODER, (b_moder2 & !(3 << 14)) | (2 << 14));
                let b_pupdr = ptr::read_volatile(GPIOB_PUPDR);
                ptr::write_volatile(GPIOB_PUPDR, (b_pupdr & !(3 << 14)) | (1 << 14)); // Pull-up
                let b_afrl2 = ptr::read_volatile(GPIOB_AFRL);
                ptr::write_volatile(GPIOB_AFRL, b_afrl2 & !(0xF << 28)); // AF0
            }

            // 4. Reset USART1 registers
            ptr::write_volatile(USART1_CR1, 0);
            ptr::write_volatile(USART1_CR2, 0);
            // CR3: bit 3 is HDSEL (Half-Duplex Selection)
            let cr3_val = if half_duplex { 1 << 3 } else { 0 };
            ptr::write_volatile(USART1_CR3, cr3_val);

            // 5. Set Baud rate divisor
            let brr = get_brr_for_baud(baud_idx);
            ptr::write_volatile(USART1_BRR, brr);

            // Clear any pending error flags
            ptr::write_volatile(USART1_ICR, 0xFFFF_FFFF);

            // Reset RX ring buffer indices
            RX_HEAD.store(0, Ordering::Relaxed);
            RX_TAIL.store(0, Ordering::Relaxed);

            // Configure NVIC for USART1 (IRQ 27)
            ptr::write_volatile(NVIC_ICPR, 1 << 27);
            let ipr6 = ptr::read_volatile(NVIC_IPR6);
            ptr::write_volatile(NVIC_IPR6, (ipr6 & !(0xFF << 24)) | (0x40 << 24));
            cortex_m::peripheral::NVIC::unmask(stm32f0xx_hal::pac::Interrupt::USART1);

            // 6. Enable UE (bit 0), TE (bit 3), RE (bit 2), and RXNEIE (bit 5)
            ptr::write_volatile(USART1_CR1, (1 << 0) | (1 << 3) | (1 << 2) | (1 << 5));
        } else {
            // Mask USART1 in NVIC
            cortex_m::peripheral::NVIC::mask(stm32f0xx_hal::pac::Interrupt::USART1);

            // Disable UE & RXNEIE
            ptr::write_volatile(USART1_CR1, 0);

            // Reset RX ring buffer indices
            RX_HEAD.store(0, Ordering::Relaxed);
            RX_TAIL.store(0, Ordering::Relaxed);

            // Disable USART1 clock in RCC_APB2ENR (bit 14)
            let apb2 = ptr::read_volatile(RCC_APB2ENR);
            ptr::write_volatile(RCC_APB2ENR, apb2 & !(1 << 14));

            // Set PB6 and PB7 back to inputs (floating/pull-up default)
            let b_moder = ptr::read_volatile(GPIOB_MODER);
            ptr::write_volatile(GPIOB_MODER, b_moder & !((3 << 12) | (3 << 14)));
        }
    }
    #[cfg(test)]
    let _ = (enabled, baud_idx, half_duplex);
}

/// Transmit a buffer over USART1 (non-blocking if space available, bounded timeout)
pub fn write_bytes(bytes: &[u8]) -> usize {
    #[cfg(test)]
    {
        mock::record_tx(bytes);
        bytes.len()
    }
    #[cfg(not(test))]
    unsafe {
        let mut sent = 0;
        for &b in bytes {
            let mut timeout = 2500u32;
            while (ptr::read_volatile(USART1_ISR) & USART_ISR_TXE) == 0 {
                timeout -= 1;
                if timeout == 0 {
                    return sent;
                }
            }
            ptr::write_volatile(USART1_TDR, b as u32);
            sent += 1;
        }
        sent
    }
}

/// Read a byte from USART1 RX if available
pub fn read_byte() -> Option<u8> {
    #[cfg(test)]
    {
        mock::pop_rx_byte()
    }
    #[cfg(not(test))]
    {
        let head = RX_HEAD.load(Ordering::Acquire);
        let tail = RX_TAIL.load(Ordering::Relaxed);
        if head != tail {
            let b = unsafe { RX_RING[tail] };
            RX_TAIL.store((tail + 1) & (unsafe { RX_RING.len() } - 1), Ordering::Release);
            Some(b)
        } else {
            None
        }
    }
}

#[cfg(not(test))]
#[interrupt]
fn USART1() {
    unsafe {
        let isr = ptr::read_volatile(USART1_ISR);
        // Clear overrun error if present
        if (isr & USART_ISR_ORE) != 0 {
            ptr::write_volatile(USART1_ICR, USART_ISR_ORE);
        }

        if (isr & USART_ISR_RXNE) != 0 {
            let b = (ptr::read_volatile(USART1_RDR) & 0xFF) as u8;
            let head = RX_HEAD.load(Ordering::Relaxed);
            let tail = RX_TAIL.load(Ordering::Acquire);
            let next_head = (head + 1) & (RX_RING.len() - 1);
            if next_head != tail {
                RX_RING[head] = b;
                RX_HEAD.store(next_head, Ordering::Release);
            }
        }
    }
}
