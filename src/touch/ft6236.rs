//! FocalTech FT6236 Capacitive Touchscreen Hardware I2C1 Driver for FlySky FS-i6S.
//!
//! Pin Assignments:
//! - PB8:  I2C1_SCL (AF1, Open-Drain, High-Speed, Pull-up) @ 400 kHz Fast Mode
//! - PB9:  I2C1_SDA (AF1, Open-Drain, High-Speed, Pull-up) @ 400 kHz Fast Mode
//! - PA15: TOUCH_RST (Active LOW Hardware Reset, pulsed LOW for 20 ms at init)
//! - PC12: TOUCH_INT (Active LOW Interrupt / Touch Detected)
//!
//! Protocol:
//! - 7-bit I2C Slave Address: `0x38`
//! - Continuous 7-byte read from register `0x00`:
//!   - Byte 0: `dev_mode`
//!   - Byte 1: `gest_id` (0x10: Swipe UP, 0x14: Swipe RIGHT, 0x18: Swipe DOWN, 0x1C: Swipe LEFT)
//!   - Byte 2: `touches` (Bits 3:0 count active touch contacts)
//!   - Byte 3: `xhi` / `event` (Bits 7:6 event flag, Bits 3:0 raw_x high nibble)
//!   - Byte 4: `xlo` (raw_x low byte)
//!   - Byte 5: `yhi` / `touch_id` (Bits 7:4 ID, Bits 3:0 raw_y high nibble)
//!   - Byte 6: `ylo` (raw_y low byte)
//!
//! Coordinate Transformation:
//! - The FT6236 sensor coordinates are swapped and scaled to match the ST7567 128×64 LCD:
//!   - LCD Y (0..63)  = (raw_x >> 1).min(63)
//!   - LCD X (0..127) = 128 - (raw_y >> 1).min(127)

#[cfg(not(test))]
use stm32f0xx_hal::pac;

pub const FT6236_I2C_ADDR: u8 = 0x38;

/// FT6236 Recognized Hardware Gestures.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Gesture {
    None,
    SwipeUp,
    SwipeRight,
    SwipeDown,
    SwipeLeft,
}

/// Touch Contact Event Kind.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TouchEventKind {
    PressDown,
    LiftUp,
    Contact,
    NoEvent,
}

/// Decoded Single Touch Point in 128×64 LCD space.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct TouchPoint {
    pub x: u8, // 0..127
    pub y: u8, // 0..63
    pub raw_x: u16,
    pub raw_y: u16,
    pub event: TouchEventKind,
}

/// Complete Touch Sample parsed from a 7-byte FT6236 packet.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct TouchSample {
    pub gesture: Gesture,
    pub point: Option<TouchPoint>,
}

#[allow(dead_code)]
#[inline(always)]
fn delay_cycles(n: u32) {
    #[cfg(not(test))]
    cortex_m::asm::delay(n);
    #[cfg(test)]
    let _ = n;
}

/// Parse a raw 7-byte FT6236 register frame (`0x00..0x06`).
///
/// Decodes hardware gestures and translates raw capacitive coordinates into 128×64 LCD space.
pub fn parse_packet(buf: &[u8; 7]) -> TouchSample {
    let gest_id = buf[1];
    let gesture = match gest_id {
        0x10 => Gesture::SwipeUp,
        0x14 => Gesture::SwipeRight,
        0x18 => Gesture::SwipeDown,
        0x1C => Gesture::SwipeLeft,
        _ => Gesture::None,
    };

    let touch_count = buf[2] & 0x0F;
    let point = if touch_count > 0 {
        let raw_x = (((buf[3] & 0x0F) as u16) << 8) | (buf[4] as u16);
        let raw_y = (((buf[5] & 0x0F) as u16) << 8) | (buf[6] as u16);

        let event = match (buf[3] >> 6) & 0x03 {
            0 => TouchEventKind::PressDown,
            1 => TouchEventKind::LiftUp,
            2 => TouchEventKind::Contact,
            _ => TouchEventKind::NoEvent,
        };

        // Axis Swap & Scaling:
        // raw_x (0..128 approx) -> LCD Y (0..63)
        // raw_y (0..256 approx) -> LCD X (0..127, inverted)
        let lcd_y = ((raw_x >> 1) as u8).min(63);
        let y_scaled = (raw_y >> 1) as u8;
        let lcd_x = 128u8.saturating_sub(y_scaled).min(127);

        Some(TouchPoint {
            x: lcd_x,
            y: lcd_y,
            raw_x,
            raw_y,
            event,
        })
    } else {
        None
    };

    TouchSample { gesture, point }
}

/// Initialize I2C1 and GPIOs for FT6236 touch controller:
/// - PB8: SCL (AF1)
/// - PB9: SDA (AF1)
/// - PA15: RST (Push-pull output, pulsed LOW)
/// - PC12: INT (Input with pull-up)
pub fn init() {
    #[cfg(not(test))]
    {
        let rcc = unsafe { &*pac::RCC::ptr() };
        let gpioa = unsafe { &*pac::GPIOA::ptr() };
        let gpiob = unsafe { &*pac::GPIOB::ptr() };
        let gpioc = unsafe { &*pac::GPIOC::ptr() };
        let i2c1 = unsafe { &*pac::I2C1::ptr() };

        unsafe {
            // 1. Enable clocks for GPIOA, GPIOB, GPIOC, and I2C1
            // AHBENR: IOPAEN (bit 17), IOPBEN (bit 18), IOPCEN (bit 19)
            rcc.ahbenr.modify(|r, w| w.bits(r.bits() | (1 << 17) | (1 << 18) | (1 << 19)));
            // APB1ENR: I2C1EN (bit 21)
            rcc.apb1enr.modify(|r, w| w.bits(r.bits() | (1 << 21)));

            // 2. Configure I2C1 clock source in RCC_CFGR3: Select SYSCLK (48 MHz)
            // I2C1SW = bit 4
            rcc.cfgr3.modify(|r, w| w.bits(r.bits() | (1 << 4)));

            // 3. Configure PA15 (TOUCH_RST): General Purpose Output (MODER = 01)
            gpioa.moder.modify(|r, w| {
                let val = r.bits();
                w.bits((val & !(3 << 30)) | (1 << 30))
            });

            // 4. Configure PC12 (TOUCH_INT): Input (MODER = 00) with pull-up (PUPDR = 01)
            gpioc.moder.modify(|r, w| w.bits(r.bits() & !(3 << 24)));
            gpioc.pupdr.modify(|r, w| {
                let val = r.bits();
                w.bits((val & !(3 << 24)) | (1 << 24))
            });

            // 5. Configure PB8 (SCL) and PB9 (SDA):
            // - Alternate Function mode (MODER = 10)
            // - Open-drain (OTYPER = 1)
            // - High speed (OSPEEDR = 11)
            // - Pull-up enabled (PUPDR = 01)
            // - AF1 mapping (AFRH bits 3:0 = 0001 for PB8, bits 7:4 = 0001 for PB9)
            gpiob.moder.modify(|r, w| {
                let val = r.bits();
                w.bits((val & !(0x0F << 16)) | (0x0A << 16))
            });
            gpiob.otyper.modify(|r, w| w.bits(r.bits() | (1 << 8) | (1 << 9)));
            gpiob.ospeedr.modify(|r, w| w.bits(r.bits() | (0x0F << 16)));
            gpiob.pupdr.modify(|r, w| {
                let val = r.bits();
                w.bits((val & !(0x0F << 16)) | (0x05 << 16))
            });
            gpiob.afrh.modify(|r, w| {
                let val = r.bits();
                w.bits((val & !0xFF) | 0x11)
            });

            // 6. Pulse PA15 LOW for 20 ms to reset FT6236 controller
            gpioa.bsrr.write(|w| w.bits(1 << (15 + 16))); // PA15 LOW
            delay_cycles(48_000 * 20); // ~20 ms @ 48 MHz
            gpioa.bsrr.write(|w| w.bits(1 << 15)); // PA15 HIGH
            delay_cycles(48_000 * 50); // ~50 ms power-up boot wait

            // 7. Configure I2C1 peripheral:
            // Disable I2C1 (PE = 0) to configure timings
            i2c1.cr1.modify(|_, w| w.pe().clear_bit());

            // 400 kHz Fast Mode @ 48 MHz SYSCLK (CubeMX standard timing: 0x00B0_1A4B)
            i2c1.timingr.write(|w| w.bits(0x00B0_1A4B));

            // Enable I2C1 peripheral (PE = 1)
            i2c1.cr1.modify(|_, w| w.pe().set_bit());
        }
    }
}

/// Check if the touch interrupt pin (PC12) is asserted (Active LOW).
pub fn is_touch_int_asserted() -> bool {
    #[cfg(not(test))]
    {
        let gpioc = unsafe { &*pac::GPIOC::ptr() };
        (gpioc.idr.read().bits() & (1 << 12)) == 0
    }
    #[cfg(test)]
    {
        false
    }
}

/// Read the 7-byte touch packet from FT6236 over I2C1 at address `0x38`.
///
/// Returns `Some(TouchSample)` if read successfully, or `None` on bus NACK/timeout.
pub fn read_touch() -> Option<TouchSample> {
    #[cfg(not(test))]
    {
        let i2c1 = unsafe { &*pac::I2C1::ptr() };

        // 1. Write register start address (0x00)
        unsafe {
            // SADD = 0x38 << 1, NBYTES = 1, RD_WRN = 0 (write), START = 1, AUTOEND = 0
            let cr2_val = ((FT6236_I2C_ADDR as u32) << 1) | (1 << 16) | (1 << 13);
            i2c1.cr2.write(|w| w.bits(cr2_val));

            // Wait for TXIS (TX empty) or NACKF
            let mut timeout = 10_000u32;
            while !i2c1.isr.read().txis().bit() && timeout > 0 {
                if i2c1.isr.read().nackf().bit() {
                    i2c1.icr.write(|w| w.nackcf().set_bit());
                    return None;
                }
                timeout -= 1;
            }
            if timeout == 0 {
                return None;
            }

            // Write register address 0x00
            i2c1.txdr.write(|w| w.txdata().bits(0x00));

            // Wait for TC (Transfer Complete)
            timeout = 10_000;
            while !i2c1.isr.read().tc().bit() && timeout > 0 {
                timeout -= 1;
            }
            if timeout == 0 {
                return None;
            }

            // 2. Repeated START to read 7 bytes (0x00..0x06)
            // SADD = 0x38 << 1, NBYTES = 7, RD_WRN = 1 (read), START = 1, AUTOEND = 1
            let cr2_read = ((FT6236_I2C_ADDR as u32) << 1) | (7 << 16) | (1 << 10) | (1 << 13) | (1 << 25);
            i2c1.cr2.write(|w| w.bits(cr2_read));

            let mut buf = [0u8; 7];
            for byte in buf.iter_mut() {
                timeout = 10_000;
                while !i2c1.isr.read().rxne().bit() && timeout > 0 {
                    if i2c1.isr.read().nackf().bit() {
                        i2c1.icr.write(|w| w.nackcf().set_bit());
                        return None;
                    }
                    timeout -= 1;
                }
                if timeout == 0 {
                    return None;
                }
                *byte = i2c1.rxdr.read().rxdata().bits();
            }

            // Wait for STOPF and clear flag
            timeout = 10_000;
            while !i2c1.isr.read().stopf().bit() && timeout > 0 {
                timeout -= 1;
            }
            i2c1.icr.write(|w| w.stopcf().set_bit());

            Some(parse_packet(&buf))
        }
    }

    #[cfg(test)]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_gestures() {
        let mut buf = [0u8; 7];

        buf[1] = 0x10; // Swipe UP
        assert_eq!(parse_packet(&buf).gesture, Gesture::SwipeUp);

        buf[1] = 0x14; // Swipe RIGHT
        assert_eq!(parse_packet(&buf).gesture, Gesture::SwipeRight);

        buf[1] = 0x18; // Swipe DOWN
        assert_eq!(parse_packet(&buf).gesture, Gesture::SwipeDown);

        buf[1] = 0x1C; // Swipe LEFT
        assert_eq!(parse_packet(&buf).gesture, Gesture::SwipeLeft);

        buf[1] = 0x00; // None
        assert_eq!(parse_packet(&buf).gesture, Gesture::None);
    }

    #[test]
    fn test_parse_coordinates_and_axis_swap() {
        let mut buf = [0u8; 7];
        buf[2] = 1; // 1 touch point

        // raw_x = 40 (buf[3] = 0, buf[4] = 40)
        // raw_y = 100 (buf[5] = 0, buf[6] = 100)
        buf[3] = 0x00; // PressDown (bits 7:6 = 00), xhi = 0
        buf[4] = 40;
        buf[5] = 0x00;
        buf[6] = 100;

        let sample = parse_packet(&buf);
        let pt = sample.point.expect("Expected point");

        assert_eq!(pt.raw_x, 40);
        assert_eq!(pt.raw_y, 100);
        assert_eq!(pt.y, 20); // 40 >> 1
        assert_eq!(pt.x, 128 - 50); // 128 - (100 >> 1) = 78
        assert_eq!(pt.event, TouchEventKind::PressDown);
    }

    #[test]
    fn test_coordinate_clamping() {
        let mut buf = [0u8; 7];
        buf[2] = 1;
        buf[3] = 0x8F; // Contact (bits 7:6 = 10), xhi = 0x0F
        buf[4] = 0xFF; // raw_x = 4095
        buf[5] = 0x0F;
        buf[6] = 0xFF; // raw_y = 4095

        let sample = parse_packet(&buf);
        let pt = sample.point.expect("Expected point");
        assert_eq!(pt.y, 63); // Clamped to LCD height
        assert_eq!(pt.x, 0);  // Saturated to 0
        assert_eq!(pt.event, TouchEventKind::Contact);
    }
}
