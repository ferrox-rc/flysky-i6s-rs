//! Hardware USART1 driver (PB6/PB7 on J15) and PF6 power switch control (H1) for CRSF / ExpressLRS.
//!
//! Pin mapping:
//! - PB6: USART1_TX (AF0, Push-Pull, High Speed; bi-directional half-duplex in single-wire mode)
//! - PB7: USART1_RX (AF0, Pull-Up, High Speed in full-duplex mode)
//! - PF6: Module Power Switch on H1 (GPIO Output, Push-Pull; Right pin PF6, Left pin GND)

#![allow(dead_code)]

#[cfg(not(test))]
use stm32f0xx_hal::pac::{self, interrupt};

use core::sync::atomic::AtomicBool;
#[cfg(not(test))]
use core::sync::atomic::{AtomicUsize, Ordering};

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
    #[cfg(not(test))]
    {
        let gpiof = &*pac::GPIOF::ptr();
        let pin_high = if ACTIVE_HIGH.load(Ordering::Relaxed) {
            POWER_ON.load(Ordering::Relaxed)
        } else {
            !POWER_ON.load(Ordering::Relaxed)
        };
        if pin_high {
            gpiof.bsrr.write(|w| w.bs6().set_bit());
        } else {
            gpiof.bsrr.write(|w| w.br6().set_bit());
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

/// Initialize GPIO pins: PF6 as power switch (initially OFF), PB6 and PB7 peripheral clocks.
pub fn init(active_high: bool) {
    #[cfg(not(test))]
    unsafe {
        ACTIVE_HIGH.store(active_high, Ordering::Relaxed);
        POWER_ON.store(false, Ordering::Relaxed);

        let rcc = &*pac::RCC::ptr();
        let gpiof = &*pac::GPIOF::ptr();

        // Enable GPIOB (bit 18), GPIOF (bit 22) clocks in RCC_AHBENR
        rcc.ahbenr.modify(|_, w| w.iopben().set_bit().iopfen().set_bit());

        // Configure PF6 as output (MODER bits 13:12 = 01)
        gpiof.moder.modify(|r, w| w.bits((r.bits() & !(3 << 12)) | (1 << 12)));

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
        let rcc = &*pac::RCC::ptr();
        let gpiob = &*pac::GPIOB::ptr();
        let usart1 = &*pac::USART1::ptr();

        if enabled {
            // 1. Enable USART1 clock in RCC_APB2ENR (bit 14 / usart1en)
            rcc.apb2enr.modify(|_, w| w.usart1en().set_bit());

            // Ensure GPIOB clock is enabled (iopben)
            rcc.ahbenr.modify(|_, w| w.iopben().set_bit());

            // 2. Configure PB6 as AF0 (USART1_TX): MODER=10 (bits 13:12), OSPEEDR=11 (bits 13:12), AFRL=0000 (bits 27:24)
            gpiob.moder.modify(|r, w| w.bits((r.bits() & !(3 << 12)) | (2 << 12)));
            gpiob.ospeedr.modify(|r, w| w.bits(r.bits() | (3 << 12))); // High speed
            gpiob.afrl.modify(|r, w| w.bits(r.bits() & !(0xF << 24))); // AF0

            if half_duplex {
                // In half-duplex (single-wire on PB6):
                // Configure PB6 as Open-Drain with internal Pull-Up (OTYPER bit 6 = 1, PUPDR bits 13:12 = 01)
                gpiob.otyper.modify(|r, w| w.bits(r.bits() | (1 << 6)));
                gpiob.pupdr.modify(|r, w| w.bits((r.bits() & !(3 << 12)) | (1 << 12)));

                // Leave PB7 as general floating input
                gpiob.moder.modify(|r, w| w.bits(r.bits() & !(3 << 14)));
            } else {
                // In full-duplex:
                // PB6 is Push-Pull (OTYPER bit 6 = 0)
                gpiob.otyper.modify(|r, w| w.bits(r.bits() & !(1 << 6)));

                // 3. Configure PB7 as AF0 (USART1_RX): MODER=10 (bits 15:14), PUPDR=01 (bits 15:14 pull-up), AFRL=0000 (bits 31:28)
                gpiob.moder.modify(|r, w| w.bits((r.bits() & !(3 << 14)) | (2 << 14)));
                gpiob.pupdr.modify(|r, w| w.bits((r.bits() & !(3 << 14)) | (1 << 14))); // Pull-up
                gpiob.afrl.modify(|r, w| w.bits(r.bits() & !(0xF << 28))); // AF0
            }

            // 4. Reset USART1 registers
            usart1.cr1.reset();
            usart1.cr2.reset();
            // CR3: bit 3 is HDSEL (Half-Duplex Selection)
            if half_duplex {
                usart1.cr3.write(|w| w.hdsel().set_bit());
            } else {
                usart1.cr3.reset();
            }

            // 5. Set Baud rate divisor
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

            // Set PB6 and PB7 back to inputs (floating/pull-up default)
            gpiob.moder.modify(|r, w| w.bits(r.bits() & !((3 << 12) | (3 << 14))));
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
        let usart1 = &*pac::USART1::ptr();
        let isr = usart1.isr.read();
        // Clear overrun error if present
        if isr.ore().bit_is_set() {
            usart1.icr.write(|w| w.orecf().clear());
        }

        if isr.rxne().bit_is_set() {
            let b = (usart1.rdr.read().rdr().bits() & 0xFF) as u8;
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
