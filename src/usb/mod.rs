//! USB Subsystem for FlySky FS-i6X.
//!
//! Orchestrates the STM32F072 USB Full-Speed (12 Mbps) peripheral,
//! HID Gamepad class (for flight simulators), and CDC-ACM Virtual COM Port (telemetry/CLI).

#![allow(static_mut_refs)]

pub mod hid;
pub mod serial;

use crate::input::Switches;
use crate::rf::afhds2a::TelemetryData;
use serial::SerialHandler;
use stm32_usbd::{MemoryAccess, UsbBus, UsbPeripheral};
use stm32f0xx_hal::pac::interrupt;
use usb_device::{
    bus::UsbBusAllocator,
    device::{StringDescriptors, UsbDevice, UsbDeviceBuilder, UsbDeviceState, UsbRev, UsbVidPid},
};
use usbd_hid::hid_class::HIDClass;
use usbd_serial::SerialPort;

/// Hardware USB peripheral mapping for STM32F072 on FlySky FS-i6X.
pub struct FlyskyUsb;

unsafe impl UsbPeripheral for FlyskyUsb {
    const REGISTERS: *const () = 0x4000_5C00 as *const ();
    const DP_PULL_UP_FEATURE: bool = true;
    const EP_MEMORY: *const () = 0x4000_6000 as *const ();
    const EP_MEMORY_SIZE: usize = 1024;
    const EP_MEMORY_ACCESS: MemoryAccess = MemoryAccess::Word16x2;

    fn enable() {
        unsafe {
            // Enable USB peripheral clock in RCC_APB1ENR bit 23
            let rcc_apb1enr = 0x4002_101C as *mut u32;
            core::ptr::write_volatile(
                rcc_apb1enr,
                core::ptr::read_volatile(rcc_apb1enr) | (1 << 23),
            );
        }
    }

    fn startup_delay() {
        // 1 ms delay at 48 MHz
        cortex_m::asm::delay(48_000);
    }
}

pub type FlyskyUsbBus = UsbBus<FlyskyUsb>;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum UsbMode {
    Off = 0,       // USB disconnected / charging only (Default)
    Joystick = 1,  // Native USB Gamepad (RF disabled for simulator play)
    Serial = 2,    // Virtual COM Port (RF active with live telemetry streaming)
    Composite = 3, // Both Joystick and Serial active simultaneously
}

impl UsbMode {
    pub fn from_u8(val: u8) -> Self {
        match val {
            1 => Self::Joystick,
            2 => Self::Serial,
            3 => Self::Composite,
            _ => Self::Off,
        }
    }

    #[allow(dead_code)]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Off => "OFF",
            Self::Joystick => "JOYSTICK",
            Self::Serial => "SERIAL",
            Self::Composite => "COMPOSITE",
        }
    }
}

pub type FlyskySerialPort<'a> = SerialPort<'a, FlyskyUsbBus, [u8; 128], [u8; 512]>;

static mut USB_ALLOCATOR: Option<UsbBusAllocator<FlyskyUsbBus>> = None;
static mut USB_DEV: Option<UsbDevice<'static, FlyskyUsbBus>> = None;
static mut USB_HID: Option<HIDClass<'static, FlyskyUsbBus>> = None;
static mut USB_SERIAL: Option<FlyskySerialPort<'static>> = None;
static mut SERIAL_HANDLER: SerialHandler = SerialHandler::new();
static mut CURRENT_MODE: UsbMode = UsbMode::Off;
static mut LAST_POLL_MS: u32 = 0;
static mut LAST_TELEM_STREAM_MS: u32 = 0;

/// Initialize the hardware USB peripheral according to the configured USB mode.
/// Supports on-the-fly switching between modes by forcing physical disconnect and core reset.
pub fn init(mode: u8) {
    let usb_mode = UsbMode::from_u8(mode);
    unsafe {
        CURRENT_MODE = usb_mode;

        // 1. Mask USB interrupt in NVIC during disconnect & reconfiguration
        cortex_m::peripheral::NVIC::mask(stm32f0xx_hal::pac::Interrupt::USB);

        // 2. Ensure USB peripheral clock is enabled so BCDR and peripheral registers can be written
        let rcc_apb1enr = 0x4002_101C as *mut u32;
        core::ptr::write_volatile(
            rcc_apb1enr,
            core::ptr::read_volatile(rcc_apb1enr) | (1 << 23),
        );

        // 3. Disable DPPU in BCDR and drive PA11 (D-) and PA12 (D+) LOW (SE0) to force physical disconnect on host PC
        let bcdr = 0x4000_5C58 as *mut u32;
        core::ptr::write_volatile(bcdr, 0); // Disable DPPU

        let gpioa_moder = 0x4800_0000 as *mut u32;
        let gpioa_bsrr = 0x4800_0018 as *mut u32;
        let moder = core::ptr::read_volatile(gpioa_moder);
        // Set PA11 (bits 23:22) and PA12 (bits 25:24) to general output (0b01) and drive LOW (SE0)
        core::ptr::write_volatile(
            gpioa_moder,
            (moder & !((0b11 << 22) | (0b11 << 24))) | ((0b01 << 22) | (0b01 << 24)),
        );
        core::ptr::write_volatile(gpioa_bsrr, (1 << (11 + 16)) | (1 << (12 + 16)));
        cortex_m::asm::delay(4_000_000); // ~250 ms delay to guarantee host root hub detects physical disconnect

        // 4. Hardware peripheral reset via RCC APB1RSTR (bit 23 = USBRST)
        let rcc_apb1rstr = 0x4002_1010 as *mut u32;
        core::ptr::write_volatile(
            rcc_apb1rstr,
            core::ptr::read_volatile(rcc_apb1rstr) | (1 << 23),
        );
        cortex_m::asm::delay(48_000);
        core::ptr::write_volatile(
            rcc_apb1rstr,
            core::ptr::read_volatile(rcc_apb1rstr) & !(1 << 23),
        );

        // 5. Drop existing USB stack instances
        USB_DEV = None;
        USB_HID = None;
        USB_SERIAL = None;
        USB_ALLOCATOR = None;
        SERIAL_HANDLER = SerialHandler::new();

        if usb_mode == UsbMode::Off {
            // Disable USB peripheral clock
            core::ptr::write_volatile(
                rcc_apb1enr,
                core::ptr::read_volatile(rcc_apb1enr) & !(1 << 23),
            );
            // Set PA11 and PA12 as analog inputs to float pins and conserve battery
            let moder = core::ptr::read_volatile(gpioa_moder);
            core::ptr::write_volatile(gpioa_moder, moder | (0b11 << 22) | (0b11 << 24));
            return;
        }

        // Configure PA11 & PA12 as AF0 (USB_DM & USB_DP)
        let gpioa_afrh = 0x4800_0024 as *mut u32;
        let moder = core::ptr::read_volatile(gpioa_moder);
        core::ptr::write_volatile(
            gpioa_moder,
            (moder & !((0b11 << 22) | (0b11 << 24))) | ((0b10 << 22) | (0b10 << 24)),
        );
        let afrh = core::ptr::read_volatile(gpioa_afrh);
        core::ptr::write_volatile(gpioa_afrh, afrh & !((0xF << 12) | (0xF << 16)));

        // Stabilization delay
        cortex_m::asm::delay(48_000);

        // 6. Setup UsbBus allocator
        let bus = FlyskyUsbBus::new(FlyskyUsb);
        USB_ALLOCATOR = Some(bus);
        let alloc = match USB_ALLOCATOR.as_ref() {
            Some(a) => a,
            None => return,
        };

        // 7. Initialize classes based on mode
        match usb_mode {
            UsbMode::Joystick => {
                let hid = HIDClass::new_ep_in(alloc, hid::GAMEPAD_REPORT_DESC, 10);
                USB_HID = Some(hid);

                let dev = create_device_builder(
                    alloc,
                    0x1209,
                    0x4F54, // OpenTX / EdgeTX Radio Joystick
                    "FS-i6X Joystick",
                    "FS-I6X-SIM",
                )
                .device_class(0x00)
                .build();
                USB_DEV = Some(dev);
            }
            UsbMode::Serial => {
                let serial = SerialPort::new_with_store(alloc, [0u8; 128], [0u8; 512]);
                USB_SERIAL = Some(serial);

                let dev = create_device_builder(
                    alloc,
                    0x0483,
                    0x5740, // Standard STM32 VCP
                    "FS-i6X Serial",
                    "FS-I6X-VCP",
                )
                .device_class(0x02) // USB_CLASS_CDC
                .build();
                USB_DEV = Some(dev);
            }
            UsbMode::Composite => {
                let hid = HIDClass::new_ep_in(alloc, hid::GAMEPAD_REPORT_DESC, 10);
                let serial = SerialPort::new_with_store(alloc, [0u8; 128], [0u8; 512]);
                USB_HID = Some(hid);
                USB_SERIAL = Some(serial);

                let dev = create_device_builder(
                    alloc,
                    0x1209,
                    0x4968, // EdgeTX Radio Composite
                    "FS-i6X Radio",
                    "FS-I6X-COMP",
                )
                .composite_with_iads()
                .build();
                USB_DEV = Some(dev);
            }
            UsbMode::Off => {}
        }

        // 8. Configure NVIC for USB Interrupt (IRQ 31)
        // Priority 0xC0 (level 3, lowest) so RF interrupts (EXTI2_3 & TIM16 at 0x80) strictly preempt USB
        const NVIC_IPR7: *mut u32 = 0xE000_E41C as *mut u32;
        let ipr7 = core::ptr::read_volatile(NVIC_IPR7);
        core::ptr::write_volatile(NVIC_IPR7, (ipr7 & !(0xFF << 24)) | (0xC0 << 24));

        // Clear any pending interrupt on IRQ 31
        const NVIC_ICPR: *mut u32 = 0xE000_E280 as *mut u32;
        core::ptr::write_volatile(NVIC_ICPR, 1 << 31);

        // Unmask USB interrupt in NVIC
        cortex_m::peripheral::NVIC::unmask(stm32f0xx_hal::pac::Interrupt::USB);
    }
}

/// Helper to safely construct a UsbDeviceBuilder without unwraps or panic risks.
fn create_device_builder<'a>(
    alloc: &'a UsbBusAllocator<FlyskyUsbBus>,
    vid: u16,
    pid: u16,
    product: &'static str,
    serial: &'static str,
) -> UsbDeviceBuilder<'a, FlyskyUsbBus> {
    let builder = UsbDeviceBuilder::new(alloc, UsbVidPid(vid, pid));
    let builder = match builder.max_packet_size_0(64) {
        Ok(b) => b,
        Err(_) => UsbDeviceBuilder::new(alloc, UsbVidPid(vid, pid)),
    };
    let builder = builder.usb_rev(UsbRev::Usb200);
    let string_desc = [StringDescriptors::default()
        .manufacturer("FlySky")
        .product(product)
        .serial_number(serial)];
    match builder.strings(&string_desc) {
        Ok(b) => b,
        Err(_) => UsbDeviceBuilder::new(alloc, UsbVidPid(vid, pid)).usb_rev(UsbRev::Usb200),
    }
}

/// USB Interrupt Handler (IRQ 31).
/// Drains pending USB peripheral hardware events with sub-microsecond latency,
/// guaranteeing timely response to enumeration requests (GET_DESCRIPTOR, SET_ADDRESS, SET_CONFIGURATION).
#[interrupt]
fn USB() {
    on_interrupt();
}

pub fn on_interrupt() {
    unsafe {
        let dev = match USB_DEV.as_mut() {
            Some(d) => d,
            None => return,
        };

        // Poll the USB stack with bounded iterations (max 2) to prevent infinite loops
        // while guaranteeing all back-to-back SETUP/CTR events are processed
        for _ in 0..2 {
            let handled = match CURRENT_MODE {
                UsbMode::Joystick => {
                    if let Some(hid) = USB_HID.as_mut() {
                        dev.poll(&mut [hid])
                    } else {
                        false
                    }
                }
                UsbMode::Serial => {
                    if let Some(serial) = USB_SERIAL.as_mut() {
                        let h = dev.poll(&mut [serial]);
                        // CRITICAL: Must drain CDC OUT endpoint to clear CTR_RX in hardware!
                        // In stm32-usbd, clear_ctr_rx() is only called inside Endpoint::read().
                        // If not read, CTR_RX stays asserted, causing an infinite interrupt storm on IRQ 31.
                        SERIAL_HANDLER.drain_rx(serial);
                        h
                    } else {
                        false
                    }
                }
                UsbMode::Composite => {
                    if let (Some(hid), Some(serial)) = (USB_HID.as_mut(), USB_SERIAL.as_mut()) {
                        let h = dev.poll(&mut [hid, serial]);
                        // CRITICAL: Must drain CDC OUT endpoint to clear CTR_RX in hardware!
                        SERIAL_HANDLER.drain_rx(serial);
                        h
                    } else {
                        false
                    }
                }
                UsbMode::Off => false,
            };

            if !handled {
                break;
            }
        }
    }
}

/// Periodic USB task (called from the main loop).
/// Dispatches HID reports at ~100 Hz (10 ms) and services serial CLI & telemetry.
pub fn poll(
    now_ms: u32,
    rf_chs: &[u16; crate::mixer::NUM_CHANNELS],
    switches: &Switches,
    telem: &TelemetryData,
    battery_mv: u16,
) {
    unsafe {
        let mode = CURRENT_MODE;
        if mode == UsbMode::Off {
            return;
        }

        // Only send reports when USB is configured and active
        let is_configured = cortex_m::interrupt::free(|_| {
            USB_DEV
                .as_ref()
                .map(|d| d.state() == UsbDeviceState::Configured)
                .unwrap_or(false)
        });

        if !is_configured {
            return;
        }

        // Throttle Gamepad HID reports to ~100 Hz (10 ms)
        if (mode == UsbMode::Joystick || mode == UsbMode::Composite)
            && now_ms.wrapping_sub(LAST_POLL_MS) >= 10
        {
            LAST_POLL_MS = now_ms;
            cortex_m::interrupt::free(|_| {
                if let Some(ref mut hid) = USB_HID.as_mut() {
                    let mut report = [0u8; hid::REPORT_SIZE];
                    hid::build_gamepad_report(rf_chs, switches, &mut report);
                    let _ = hid.push_raw_input(&report);
                }
            });
        }

        // Periodic telemetry streaming over Serial in Serial/Composite modes (at 10 Hz / 100 ms)
        if mode == UsbMode::Serial || mode == UsbMode::Composite {
            cortex_m::interrupt::free(|_| {
                if let Some(ref mut serial) = USB_SERIAL.as_mut() {
                    SERIAL_HANDLER.update(serial, rf_chs, telem, battery_mv);
                    if now_ms.wrapping_sub(LAST_TELEM_STREAM_MS) >= 100 {
                        LAST_TELEM_STREAM_MS = now_ms;
                        if SERIAL_HANDLER.is_streaming() {
                            SERIAL_HANDLER.send_telemetry_line(serial, rf_chs, telem, battery_mv);
                        }
                    }
                }
            });
        }
    }
}

/// Check if the USB device is currently enumerated and configured by the host PC.
pub fn is_connected() -> bool {
    cortex_m::interrupt::free(|_| unsafe {
        USB_DEV
            .as_ref()
            .map(|d| d.state() == UsbDeviceState::Configured)
            .unwrap_or(false)
    })
}

/// Check if the radio is actively in Joystick / Simulator mode and connected.
/// When true, the RF transceiver is placed in Standby (zero RF emission / silent running).
pub fn is_sim_mode() -> bool {
    let is_joy = unsafe { CURRENT_MODE == UsbMode::Joystick };
    is_joy && is_connected()
}

/// Get currently active USB mode.
#[allow(dead_code)]
pub fn get_mode() -> UsbMode {
    unsafe { CURRENT_MODE }
}
