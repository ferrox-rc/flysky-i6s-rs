//! Hardware USART1 driver (PB6/PB7 on J15) and PF6 power switch control (H1) for CRSF / ExpressLRS.
//!
//! Pin mapping:
//! - PB6: USART1_TX (AF0, Push-Pull, High Speed; bi-directional half-duplex in single-wire mode)
//! - PB7: USART1_RX (AF0, Pull-Up, High Speed in full-duplex mode)
//! - PF6: Module Power Switch on H1 (GPIO Output, Push-Pull; Right pin PF6, Left pin GND)

#![allow(dead_code)]

#[cfg(not(test))]
use stm32f0xx_hal::pac::{self, interrupt};

use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

#[cfg(not(test))]
const RX_RING_SIZE: usize = 128;
#[cfg(not(test))]
static mut RX_RING: [u8; RX_RING_SIZE] = [0; RX_RING_SIZE];
#[cfg(not(test))]
static RX_HEAD: AtomicUsize = AtomicUsize::new(0);
#[cfg(not(test))]
static RX_TAIL: AtomicUsize = AtomicUsize::new(0);

static ACTIVE_HIGH: AtomicBool = AtomicBool::new(true);
static POWER_ON: AtomicBool = AtomicBool::new(false);
static HALF_DUPLEX_ACTIVE: AtomicBool = AtomicBool::new(false);
static ECHO_SKIP_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Apply current PF6 pin state based on power state and active polarity.
#[cfg(not(test))]
unsafe fn apply_power_pin() {
    let pin_high = if ACTIVE_HIGH.load(Ordering::Relaxed) {
        POWER_ON.load(Ordering::Relaxed)
    } else {
        !POWER_ON.load(Ordering::Relaxed)
    };
    let gpiof = &*pac::GPIOF::ptr();
    if pin_high {
        gpiof.bsrr.write(|w| w.bs6().set_bit()); // High
    } else {
        gpiof.bsrr.write(|w| w.br6().set_bit()); // Low
    }
}

#[cfg(test)]
unsafe fn apply_power_pin() {}

/// Initialize GPIO pins: PF6 as power switch (initially OFF), PB6 and PB7 peripheral clocks.
pub fn init(active_high: bool) {
    #[cfg(not(test))]
    unsafe {
        ACTIVE_HIGH.store(active_high, Ordering::Relaxed);
        POWER_ON.store(false, Ordering::Relaxed);

        // Enable GPIOB, GPIOF clocks in RCC_AHBENR
        let rcc = &*pac::RCC::ptr();
        rcc.ahbenr.modify(|_, w| w.iopben().set_bit().iopfen().set_bit());

        // Configure PF6 as general purpose output
        let gpiof = &*pac::GPIOF::ptr();
        gpiof.moder.modify(|_, w| w.moder6().output());

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
        0 => 417, // 115,200 bps (48,000,000 / 115,200 = 416.66)
        1 => 115, // 416,666 bps (48,000,000 / 416,666.67 = 115.20)
        2 => 114, // 420,000 bps (48,000,000 / 420,000 = 114.285)
        3 => 52,  // 921,600 bps (48,000,000 / 921,600 = 52.08)
        4 => 26,  // 1,875,000 bps (48,000,000 / 1,875,000 = 25.6 -> 26 = 1,846,154 bps, 1.5% error)
        _ => 114, // default 420k (ELRS)
    }
}

/// Returns true if half-duplex (1-wire) mode is currently active.
pub fn is_half_duplex() -> bool {
    HALF_DUPLEX_ACTIVE.load(Ordering::Relaxed)
}

/// Configure and enable/disable USART1 (full-duplex or single-wire half-duplex on PB6)
pub fn set_uart_enabled(enabled: bool, baud_idx: u8, half_duplex: bool) {
    HALF_DUPLEX_ACTIVE.store(half_duplex, Ordering::Relaxed);
    ECHO_SKIP_COUNT.store(0, Ordering::Relaxed);

    #[cfg(not(test))]
    unsafe {
        let rcc = &*pac::RCC::ptr();
        let gpiob = &*pac::GPIOB::ptr();
        let usart1 = &*pac::USART1::ptr();

        if enabled {
            // 1. Enable USART1 clock in RCC_APB2ENR and GPIOB clock in RCC_AHBENR
            rcc.apb2enr.modify(|_, w| w.usart1en().set_bit());
            rcc.ahbenr.modify(|_, w| w.iopben().set_bit());

            // 2. Configure PB6 as AF0 (USART1_TX): MODER=10, OSPEEDR=11, AFRL6=0000
            gpiob.moder.modify(|_, w| w.moder6().alternate());
            gpiob.ospeedr.modify(|_, w| w.ospeedr6().high_speed());
            gpiob.afrl.modify(|_, w| w.afrl6().af0());

            if half_duplex {
                // Half-duplex single-wire on PB6: Open-Drain with internal Pull-Up
                gpiob.otyper.modify(|_, w| w.ot6().open_drain());
                gpiob.pupdr.modify(|_, w| w.pupdr6().pull_up());

                // Leave PB7 as general floating input
                gpiob.moder.modify(|_, w| w.moder7().input());
            } else {
                // Full-duplex: PB6 Push-Pull, no pull resistor
                gpiob.otyper.modify(|_, w| w.ot6().push_pull());
                gpiob.pupdr.modify(|_, w| w.pupdr6().floating());

                // 3. Configure PB7 as AF0 (USART1_RX): MODER=10, PUPDR=01 (pull-up), AFRL7=0000
                gpiob.moder.modify(|_, w| w.moder7().alternate());
                gpiob.pupdr.modify(|_, w| w.pupdr7().pull_up());
                gpiob.afrl.modify(|_, w| w.afrl7().af0());
            }

            // 4. Reset USART1 registers
            usart1.cr1.reset();
            usart1.cr2.reset();

            // In half-duplex mode, enable HDSEL (bit 3 in CR3)
            if half_duplex {
                usart1.cr3.write(|w| w.hdsel().set_bit());
            } else {
                usart1.cr3.reset();
            }

            // 5. Set Baud rate divisor (standard 16x oversampling OVER8 = 0)
            let brr = get_brr_for_baud(baud_idx);
            usart1.brr.write(|w| w.brr().bits(brr as u16));

            // Clear any pending error flags
            usart1.icr.write(|w| w.bits(0xFFFF_FFFF));

            // Reset RX ring buffer indices
            RX_HEAD.store(0, Ordering::Relaxed);
            RX_TAIL.store(0, Ordering::Relaxed);

            // Configure NVIC for USART1 (IRQ 27)
            cortex_m::peripheral::NVIC::unpend(pac::Interrupt::USART1);
            let mut cp = cortex_m::peripheral::Peripherals::steal();
            cp.NVIC.set_priority(pac::Interrupt::USART1, 0x40);
            cortex_m::peripheral::NVIC::unmask(pac::Interrupt::USART1);

            // 6. Enable UE, TE, RE, and RXNEIE
            usart1.cr1.write(|w| {
                w.ue().enabled()
                 .te().enabled()
                 .re().enabled()
                 .rxneie().enabled()
            });
        } else {
            // Mask USART1 in NVIC
            cortex_m::peripheral::NVIC::mask(pac::Interrupt::USART1);

            // Disable UE & RXNEIE
            usart1.cr1.reset();

            // Reset RX ring buffer indices
            RX_HEAD.store(0, Ordering::Relaxed);
            RX_TAIL.store(0, Ordering::Relaxed);

            // Disable USART1 clock in RCC_APB2ENR
            rcc.apb2enr.modify(|_, w| w.usart1en().clear_bit());

            // Set PB6 and PB7 back to inputs
            gpiob.moder.modify(|_, w| w.moder6().input().moder7().input());
            gpiob.otyper.modify(|_, w| w.ot6().push_pull());
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
        let is_hd = HALF_DUPLEX_ACTIVE.load(Ordering::Relaxed);
        let usart1 = &*pac::USART1::ptr();
        let mut sent = 0;
        for &b in bytes {
            let mut timeout = 2500u32;
            while usart1.isr.read().txe().bit_is_clear() {
                timeout -= 1;
                if timeout == 0 {
                    return sent;
                }
            }
            if is_hd {
                cortex_m::interrupt::free(|_| {
                    let cur = ECHO_SKIP_COUNT.load(Ordering::Relaxed);
                    ECHO_SKIP_COUNT.store(cur + 1, Ordering::Relaxed);
                });
            }
            usart1.tdr.write(|w| w.tdr().bits(b as u16));
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
            RX_TAIL.store((tail + 1) & (RX_RING_SIZE - 1), Ordering::Release);
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
        let usart1 = &*pac::USART1::ptr();
        let isr = usart1.isr.read();
        // Clear overrun error if present
        if isr.ore().bit_is_set() {
            usart1.icr.write(|w| w.orecf().clear());
        }

        if isr.rxne().bit_is_set() {
            let b = (usart1.rdr.read().rdr().bits() & 0xFF) as u8;
            if HALF_DUPLEX_ACTIVE.load(Ordering::Relaxed) {
                let skip = ECHO_SKIP_COUNT.load(Ordering::SeqCst);
                if skip > 0 {
                    ECHO_SKIP_COUNT.store(skip - 1, Ordering::SeqCst);
                    return;
                }
            }

            let head = RX_HEAD.load(Ordering::Relaxed);
            let tail = RX_TAIL.load(Ordering::Acquire);
            let next_head = (head + 1) & (RX_RING_SIZE - 1);
            if next_head != tail {
                RX_RING[head] = b;
                RX_HEAD.store(next_head, Ordering::Release);
            }
        }
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
