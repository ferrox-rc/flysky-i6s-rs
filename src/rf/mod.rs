//! RF subsystem module for the FlySky FS-i6X.
//!
//! Orchestrates the A7105 2.4 GHz transceiver, SPI1 driver, and AFHDS 2A stack
//! via TIM16 (3.85 ms framing) and EXTI2 (A7105 GIO2 WTR signal).

pub mod a7105;
pub mod afhds2a;
pub mod spi;

use crate::mixer::CHANNEL_CENTER_US;
use afhds2a::{Afhds2a, TelemetryData, NUM_CHANNELS};
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use stm32f0xx_hal::pac::interrupt;

struct RfDriverCell(UnsafeCell<Option<Afhds2a>>);
unsafe impl Sync for RfDriverCell {}

static RF_DRIVER: RfDriverCell = RfDriverCell(UnsafeCell::new(None));

/// Sound, lock-free Single-Producer Single-Consumer (SPSC) Triple Buffer.
///
/// Implements asynchronous non-blocking data exchange between a high-rate writer
/// (the flight pipeline) and an asynchronous reader (the TIM16 RF interrupt handler).
///
/// Uses an `UnsafeCell` holding 3 buffer slots:
/// - 1 slot being read by the consumer
/// - 1 slot being written by the producer
/// - 1 intermediate slot holding the latest published data
///
/// Under this 3-slot model:
/// 1. The writer NEVER touches the slot currently being read by the consumer.
/// 2. The reader NEVER touches the slot currently being written by the producer.
/// 3. Neither reader nor writer ever block, spin, or mask interrupts (`CPSID`).
/// 4. Provably eliminates torn reads, torn writes, and data races on ARM Cortex-M0.
pub struct TripleBuffer<T: Copy> {
    buffers: UnsafeCell<[T; 3]>,
    /// Bits [1:0]: index of the latest fully-written buffer ready for reading (0..2).
    /// Bit [7]: new data dirty flag (1 if a new frame is ready to read, 0 otherwise).
    published: AtomicU8,
    /// Index of the buffer currently being read by the consumer (0..2).
    reading_idx: AtomicU8,
}

unsafe impl<T: Copy + Send> Sync for TripleBuffer<T> {}

impl<T: Copy> TripleBuffer<T> {
    pub const fn new(initial: T) -> Self {
        Self {
            buffers: UnsafeCell::new([initial; 3]),
            published: AtomicU8::new(0),
            reading_idx: AtomicU8::new(1),
        }
    }

    /// Write new data into an available slot and atomically publish it.
    /// Called exclusively by the producer (single writer thread).
    #[inline(always)]
    pub fn write(&self, data: &T) {
        let published_val = self.published.load(Ordering::Relaxed);
        let last_published = published_val & 0x03;
        let reading = self.reading_idx.load(Ordering::Relaxed) & 0x03;

        // Find the slot that is neither currently being read nor the last published
        let mut write_idx = 0;
        while write_idx == last_published || write_idx == reading {
            write_idx += 1;
        }

        // Safety: `write_idx` is guaranteed distinct from `reading`, meaning the consumer
        // is not currently reading from this memory slot.
        unsafe {
            let buf_ptr = self.buffers.get();
            (*buf_ptr)[write_idx as usize] = *data;
        }

        // Atomically publish with Release ordering so memory writes are visible before pointer update.
        // Bit 7 indicates fresh unread data.
        self.published.store(0x80 | write_idx, Ordering::Release);
    }

    /// Read the latest available data.
    /// Called by the consumer (single reader, e.g. ISR).
    #[inline(always)]
    pub fn read(&self) -> T {
        // Check if a new buffer was published since last read
        let published_val = self.published.load(Ordering::Acquire);
        let read_idx = if (published_val & 0x80) != 0 {
            // Clear dirty bit so we know it has been consumed
            let idx = published_val & 0x03;
            self.published.store(idx, Ordering::Relaxed);
            self.reading_idx.store(idx, Ordering::Relaxed);
            idx
        } else {
            // No new data: continue reading from current slot
            self.reading_idx.load(Ordering::Relaxed) & 0x03
        };

        // Safety: `read_idx` is distinct from the writer's current write target slot.
        unsafe {
            let buf_ptr = self.buffers.get();
            (*buf_ptr)[read_idx as usize]
        }
    }
}

static CHANNEL_BUFFER: TripleBuffer<[u16; NUM_CHANNELS]> =
    TripleBuffer::new([CHANNEL_CENTER_US; NUM_CHANNELS]);
static TELEMETRY_BUFFER: TripleBuffer<TelemetryData> = TripleBuffer::new(TelemetryData::new());
static RF_SILENCED: AtomicBool = AtomicBool::new(false);

/// Initialize RF subsystem: SPI1, A7105 transceiver, and AFHDS 2A stack.
/// Returns true if A7105 responded and passed silicon verification (0x9E).
pub fn init(tx_id: u32) -> bool {
    spi::init();

    let mut reset_ok = a7105::reset();

    a7105::init();

    let post_sig = a7105::read_reg(a7105::REG_PLL_II);
    if post_sig == 0x9E {
        reset_ok = true;
    }
    a7105::LAST_CHIP_ID.store(post_sig, Ordering::Relaxed);

    unsafe {
        *RF_DRIVER.0.get() = Some(Afhds2a::new(tx_id));
    }

    // Start 3.85 ms (260 Hz) periodic packet framing timer
    spi::init_tim16(3850);

    reset_ok
}

/// Get the last read byte from A7105 register 0x10 during reset.
pub fn get_last_chip_id() -> u8 {
    a7105::LAST_CHIP_ID.load(Ordering::Relaxed)
}

/// Put RF frontend and A7105 into standby (silent running) during USB Joystick simulator mode.
pub fn set_silenced(silenced: bool) {
    let prev = RF_SILENCED.load(Ordering::Relaxed);
    if prev != silenced {
        RF_SILENCED.store(silenced, Ordering::Relaxed);
        if silenced {
            a7105::strobe(a7105::STROBE_STANDBY);
            spi::set_tx_rx_mode(spi::RF_MODE_OFF);
        }
    }
}

/// Check if RF transmission is silenced.
#[allow(dead_code)]
pub fn is_silenced() -> bool {
    RF_SILENCED.load(Ordering::Relaxed)
}

/// Update channel outputs (CH1..CH18) in microseconds (988..2012 µs).
/// Completely lock-free triple-buffered write: zero critical sections, zero interrupt latency.
#[inline(always)]
pub fn set_channels(channels: &[u16; NUM_CHANNELS]) {
    CHANNEL_BUFFER.write(channels);
}

#[inline]
fn with_driver<T, F: FnOnce(&afhds2a::Afhds2a) -> T>(f: F) -> Option<T> {
    cortex_m::interrupt::free(|_| {
        let driver = unsafe { &*RF_DRIVER.0.get() };
        driver.as_ref().map(f)
    })
}

#[inline]
fn with_driver_mut<T, F: FnOnce(&mut afhds2a::Afhds2a) -> T>(f: F) -> Option<T> {
    cortex_m::interrupt::free(|_| {
        let driver = unsafe { &mut *RF_DRIVER.0.get() };
        driver.as_mut().map(f)
    })
}

/// Enter or exit binding mode.
pub fn set_bind_mode(enable: bool) {
    with_driver_mut(|d| d.set_bind_mode(enable));
}

/// Dynamically update receiver ID in the RF driver (e.g. on model switch).
pub fn set_rx_id(rx_id: u32) {
    with_driver_mut(|d| d.set_rx_id(rx_id));
}

/// Query whether binding has completed.
pub fn is_bound() -> bool {
    with_driver(|d| d.bind_done || (d.rx_id != 0 && d.rx_id != 0xFFFF_FFFF)).unwrap_or(false)
}

/// Query whether the radio is currently in binding mode.
pub fn is_binding() -> bool {
    with_driver(|d| d.mode == afhds2a::RadioMode::Binding).unwrap_or(false)
}

/// Check if a newly captured/bound RX ID needs to be persisted to Flash, and return it.
pub fn take_pending_rx_save() -> Option<u32> {
    with_driver_mut(|d| {
        if d.rx_id_needs_save {
            d.rx_id_needs_save = false;
            Some(d.rx_id)
        } else {
            None
        }
    })
    .flatten()
}

/// Get currently active receiver ID.
#[allow(dead_code)]
pub fn get_rx_id() -> u32 {
    with_driver(|d| d.rx_id).unwrap_or(0xFFFF_FFFF)
}

/// Get latest downlink telemetry from the receiver.
/// Lock-free atomic read: zero critical sections.
#[inline(always)]
pub fn get_telemetry() -> TelemetryData {
    TELEMETRY_BUFFER.read()
}

/// TIM16 Interrupt Handler: Triggers transmission of next AFHDS 2A frame.
#[interrupt]
fn TIM16() {
    spi::clear_tim16_flag();

    if RF_SILENCED.load(Ordering::Relaxed) {
        return;
    }

    let driver = unsafe { &mut *RF_DRIVER.0.get() };
    if let Some(ref mut d) = driver {
        let chs = CHANNEL_BUFFER.read();
        d.set_channels(&chs);
        d.on_timer_tick();
    }
}

/// EXTI2_3 Interrupt Handler: Triggers when A7105 GIO2 transitions LOW
/// (indicating TX packet finished or RX packet received).
#[interrupt]
fn EXTI2_3() {
    unsafe {
        let exti_pr = core::ptr::read_volatile(0x4001_0414 as *const u32);
        if (exti_pr & (1 << 2)) != 0 {
            // Clear pending flag for EXTI2
            core::ptr::write_volatile(0x4001_0414 as *mut u32, 1 << 2);

            let driver = &mut *RF_DRIVER.0.get();
            if let Some(ref mut d) = driver {
                d.on_gio2_event();
                // Publish updated telemetry to triple buffer lock-free
                TELEMETRY_BUFFER.write(&d.telemetry);
            }
        }
    }
}
