//! Hardware USART2 driver and PC13 power switch control for CRSF / ExpressLRS.
//!
//! Pin mapping:
//! - PD5:  USART2_TX (AF0, Push-Pull, High Speed)
//! - PA15: USART2_RX (AF1, Pull-Up, High Speed)
//! - PC13: Module Power Switch (GPIO Output, Push-Pull, High = Power ON)

#![allow(dead_code)]

use core::ptr;

// RCC registers
const RCC_AHBENR: *mut u32 = 0x4002_1014 as *mut u32;
const RCC_APB1ENR: *mut u32 = 0x4002_101C as *mut u32;

// GPIOA registers (Base: 0x4800_0000)
const GPIOA_MODER: *mut u32 = 0x4800_0000 as *mut u32;
const GPIOA_OSPEEDR: *mut u32 = 0x4800_0008 as *mut u32;
const GPIOA_PUPDR: *mut u32 = 0x4800_000C as *mut u32;
const GPIOA_AFRH: *mut u32 = 0x4800_0024 as *mut u32;

// GPIOC registers (Base: 0x4800_0800)
const GPIOC_MODER: *mut u32 = 0x4800_0800 as *mut u32;
const GPIOC_BSRR: *mut u32 = 0x4800_0818 as *mut u32;

// GPIOD registers (Base: 0x4800_0C00)
const GPIOD_MODER: *mut u32 = 0x4800_0C00 as *mut u32;
const GPIOD_OTYPER: *mut u32 = 0x4800_0C04 as *mut u32;
const GPIOD_OSPEEDR: *mut u32 = 0x4800_0C08 as *mut u32;
const GPIOD_PUPDR: *mut u32 = 0x4800_0C0C as *mut u32;
const GPIOD_AFRL: *mut u32 = 0x4800_0C20 as *mut u32;

// USART2 registers (Base: 0x4000_4400)
const USART2_CR1: *mut u32 = 0x4000_4400 as *mut u32;
const USART2_CR2: *mut u32 = 0x4000_4404 as *mut u32;
const USART2_CR3: *mut u32 = 0x4000_4408 as *mut u32;
const USART2_BRR: *mut u32 = 0x4000_440C as *mut u32;
const USART2_ISR: *mut u32 = 0x4000_441C as *mut u32;
const USART2_ICR: *mut u32 = 0x4000_4420 as *mut u32;
const USART2_RDR: *mut u32 = 0x4000_4424 as *mut u32;
const USART2_TDR: *mut u32 = 0x4000_4428 as *mut u32;

// Status register flags
const USART_ISR_TXE: u32 = 1 << 7;
const USART_ISR_TC: u32 = 1 << 6;
const USART_ISR_RXNE: u32 = 1 << 5;
const USART_ISR_ORE: u32 = 1 << 3;

// NVIC registers
#[cfg(not(test))]
const NVIC_ICPR: *mut u32 = 0xE000_E280 as *mut u32;
#[cfg(not(test))]
const NVIC_IPR7: *mut u32 = 0xE000_E41C as *mut u32;

#[cfg(not(test))]
use stm32f0xx_hal::pac::interrupt;

use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

#[cfg(not(test))]
static mut RX_RING: [u8; 128] = [0; 128];
#[cfg(not(test))]
static RX_HEAD: AtomicUsize = AtomicUsize::new(0);
#[cfg(not(test))]
static RX_TAIL: AtomicUsize = AtomicUsize::new(0);

static ACTIVE_HIGH: AtomicBool = AtomicBool::new(true);
static POWER_ON: AtomicBool = AtomicBool::new(false);
static HALF_DUPLEX_ACTIVE: AtomicBool = AtomicBool::new(false);
static ECHO_SKIP_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Apply current PC13 pin state based on power state and active polarity.
unsafe fn apply_power_pin() {
    let pin_high = if ACTIVE_HIGH.load(Ordering::Relaxed) {
        POWER_ON.load(Ordering::Relaxed)
    } else {
        !POWER_ON.load(Ordering::Relaxed)
    };
    if pin_high {
        ptr::write_volatile(GPIOC_BSRR, 1 << 13); // High
    } else {
        ptr::write_volatile(GPIOC_BSRR, 1 << (13 + 16)); // Low
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

/// Initialize GPIO pins: PC13 as power switch (initially OFF), PD5 and PA15 peripheral clocks.
pub fn init(active_high: bool) {
    #[cfg(not(test))]
    unsafe {
        ACTIVE_HIGH.store(active_high, Ordering::Relaxed);
        POWER_ON.store(false, Ordering::Relaxed);

        // Enable GPIOA (bit 17), GPIOC (bit 19), GPIOD (bit 20) clocks
        let ahb = ptr::read_volatile(RCC_AHBENR);
        ptr::write_volatile(RCC_AHBENR, ahb | (1 << 17) | (1 << 19) | (1 << 20));

        // Configure PC13 as output (MODER 01)
        let c_moder = ptr::read_volatile(GPIOC_MODER);
        ptr::write_volatile(GPIOC_MODER, (c_moder & !(3 << 26)) | (1 << 26));

        // Apply initial OFF state according to polarity
        apply_power_pin();
    }
    #[cfg(test)]
    let _ = active_high;
}

/// Set active power polarity for PC13: true = Active HIGH (N-type), false = Active LOW (P-type).
pub fn set_power_polarity(active_high: bool) {
    #[cfg(not(test))]
    unsafe {
        ACTIVE_HIGH.store(active_high, Ordering::Relaxed);
        apply_power_pin();
    }
    #[cfg(test)]
    let _ = active_high;
}

/// Set external module power state via PC13
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
        4 => 26,  // 1,875,000 bps (48,000,000 / 1,875,000 = 25.6 -> 26 = 1,846,154 bps, 1.5% error)
        _ => 114,
    }
}

/// Returns true if half-duplex (1-wire) mode is currently active.
pub fn is_half_duplex() -> bool {
    HALF_DUPLEX_ACTIVE.load(Ordering::Relaxed)
}

/// Configure and enable/disable USART2
pub fn set_uart_enabled(enabled: bool, baud_idx: u8, half_duplex: bool) {
    HALF_DUPLEX_ACTIVE.store(half_duplex, Ordering::Relaxed);
    ECHO_SKIP_COUNT.store(0, Ordering::Relaxed);

    #[cfg(not(test))]
    unsafe {
        if enabled {
            // 1. Enable USART2 clock in RCC_APB1ENR (bit 17)
            let apb1 = ptr::read_volatile(RCC_APB1ENR);
            ptr::write_volatile(RCC_APB1ENR, apb1 | (1 << 17));

            // 2. Configure PD5 as AF0 (USART2_TX): MODER=10, AFRL bit 20..23 = 0000
            let d_moder = ptr::read_volatile(GPIOD_MODER);
            ptr::write_volatile(GPIOD_MODER, (d_moder & !(3 << 10)) | (2 << 10));
            let d_ospeedr = ptr::read_volatile(GPIOD_OSPEEDR);
            ptr::write_volatile(GPIOD_OSPEEDR, d_ospeedr | (3 << 10)); // High speed
            let d_afrl = ptr::read_volatile(GPIOD_AFRL);
            ptr::write_volatile(GPIOD_AFRL, d_afrl & !(0xF << 20)); // AF0

            let d_otyper = ptr::read_volatile(GPIOD_OTYPER);
            let d_pupdr = ptr::read_volatile(GPIOD_PUPDR);

            if half_duplex {
                // Half-duplex single-wire: PD5 Open-Drain with internal Pull-Up
                ptr::write_volatile(GPIOD_OTYPER, d_otyper | (1 << 5));
                ptr::write_volatile(GPIOD_PUPDR, (d_pupdr & !(3 << 10)) | (1 << 10)); // Pull-up on PD5

                // PA15 is not used in half-duplex; float as input
                let a_moder = ptr::read_volatile(GPIOA_MODER);
                ptr::write_volatile(GPIOA_MODER, a_moder & !(3 << 30));
            } else {
                // Full-duplex: PD5 Push-Pull, no pull resistor
                ptr::write_volatile(GPIOD_OTYPER, d_otyper & !(1 << 5));
                ptr::write_volatile(GPIOD_PUPDR, d_pupdr & !(3 << 10));

                // Configure PA15 as AF1 (USART2_RX): MODER=10, AFRH bit 28..31 = 0001, Pull-Up
                let a_moder = ptr::read_volatile(GPIOA_MODER);
                ptr::write_volatile(GPIOA_MODER, (a_moder & !(3 << 30)) | (2 << 30));
                let a_pupdr = ptr::read_volatile(GPIOA_PUPDR);
                ptr::write_volatile(GPIOA_PUPDR, (a_pupdr & !(3 << 30)) | (1 << 30)); // Pull-up
                let a_afrh = ptr::read_volatile(GPIOA_AFRH);
                ptr::write_volatile(GPIOA_AFRH, (a_afrh & !(0xF << 28)) | (1 << 28)); // AF1
            }

            // 4. Reset USART2 registers
            ptr::write_volatile(USART2_CR1, 0);
            ptr::write_volatile(USART2_CR2, 0);

            // In half-duplex mode, enable HDSEL (bit 3 in CR3)
            let cr3 = if half_duplex { 1 << 3 } else { 0 };
            ptr::write_volatile(USART2_CR3, cr3);

            // 5. Set Baud rate divisor
            let brr = get_brr_for_baud(baud_idx);
            ptr::write_volatile(USART2_BRR, brr);

            // Clear any pending error flags
            ptr::write_volatile(USART2_ICR, 0xFFFF_FFFF);

            // Reset RX ring buffer indices
            RX_HEAD.store(0, Ordering::Relaxed);
            RX_TAIL.store(0, Ordering::Relaxed);

            // Configure NVIC for USART2 (IRQ 28)
            ptr::write_volatile(NVIC_ICPR, 1 << 28);
            let ipr7 = ptr::read_volatile(NVIC_IPR7);
            ptr::write_volatile(NVIC_IPR7, (ipr7 & !0xFF) | 0x40);
            cortex_m::peripheral::NVIC::unmask(stm32f0xx_hal::pac::Interrupt::USART2);

            // 6. Enable UE (bit 0), TE (bit 3), RE (bit 2), and RXNEIE (bit 5)
            ptr::write_volatile(USART2_CR1, (1 << 0) | (1 << 3) | (1 << 2) | (1 << 5));
        } else {
            // Mask USART2 in NVIC
            cortex_m::peripheral::NVIC::mask(stm32f0xx_hal::pac::Interrupt::USART2);

            // Disable UE & RXNEIE
            ptr::write_volatile(USART2_CR1, 0);

            // Reset RX ring buffer indices
            RX_HEAD.store(0, Ordering::Relaxed);
            RX_TAIL.store(0, Ordering::Relaxed);

            // Disable USART2 clock in RCC_APB1ENR (bit 17)
            let apb1 = ptr::read_volatile(RCC_APB1ENR);
            ptr::write_volatile(RCC_APB1ENR, apb1 & !(1 << 17));

            // Set PD5 and PA15 back to inputs to float pins
            let d_moder = ptr::read_volatile(GPIOD_MODER);
            ptr::write_volatile(GPIOD_MODER, d_moder & !(3 << 10));
            let d_otyper = ptr::read_volatile(GPIOD_OTYPER);
            ptr::write_volatile(GPIOD_OTYPER, d_otyper & !(1 << 5));
            let a_moder = ptr::read_volatile(GPIOA_MODER);
            ptr::write_volatile(GPIOA_MODER, a_moder & !(3 << 30));
        }
    }
    #[cfg(test)]
    let _ = (enabled, baud_idx, half_duplex);
}

/// Transmit a buffer over USART2 (non-blocking if space available, bounded timeout)
pub fn write_bytes(bytes: &[u8]) -> usize {
    #[cfg(test)]
    {
        mock::record_tx(bytes);
        // If half-duplex mock active, self-echo bytes to RX queue if desired, or skip
        bytes.len()
    }
    #[cfg(not(test))]
    unsafe {
        let is_hd = HALF_DUPLEX_ACTIVE.load(Ordering::Relaxed);
        let mut sent = 0;
        for &b in bytes {
            let mut timeout = 2500u32;
            while (ptr::read_volatile(USART2_ISR) & USART_ISR_TXE) == 0 {
                timeout -= 1;
                if timeout == 0 {
                    return sent;
                }
            }
            if is_hd {
                ECHO_SKIP_COUNT.fetch_add(1, Ordering::SeqCst);
            }
            ptr::write_volatile(USART2_TDR, b as u32);
            sent += 1;
        }
        sent
    }
}

/// Read a byte from USART2 RX if available
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
fn USART2() {
    unsafe {
        let isr = ptr::read_volatile(USART2_ISR);
        // Clear overrun error if present
        if (isr & USART_ISR_ORE) != 0 {
            ptr::write_volatile(USART2_ICR, USART_ISR_ORE);
        }

        if (isr & USART_ISR_RXNE) != 0 {
            let b = (ptr::read_volatile(USART2_RDR) & 0xFF) as u8;
            if HALF_DUPLEX_ACTIVE.load(Ordering::Relaxed) {
                let skip = ECHO_SKIP_COUNT.load(Ordering::SeqCst);
                if skip > 0 {
                    ECHO_SKIP_COUNT.store(skip - 1, Ordering::SeqCst);
                    return;
                }
            }

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
