//! ST7567 128×64 Monochrome LCD 8-bit Parallel Driver with Embedded-Graphics Support.

use core::convert::Infallible;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{OriginDimensions, Size},
    pixelcolor::BinaryColor,
    Pixel,
};
#[cfg(not(test))]
use stm32f0xx_hal::pac;

pub const WIDTH: usize = 128;
pub const HEIGHT: usize = 64;
pub const BUFFER_SIZE: usize = WIDTH * (HEIGHT / 8); // 1024 bytes

/// ST7567 LCD Driver with 1024-byte internal framebuffer.
pub struct St7567 {
    framebuffer: [u8; BUFFER_SIZE],
}

impl Default for St7567 {
    fn default() -> Self {
        Self::new()
    }
}

impl St7567 {
    /// Initialize GPIO pins and the ST7567 display controller.
    pub fn new() -> Self {
        #[allow(unused_mut)]
        let mut display = Self {
            framebuffer: [0u8; BUFFER_SIZE],
        };
        #[cfg(not(test))]
        {
            display.init_hardware();
            display.init_controller();
            display.init_backlight_pwm();
            display.clear_buffer();
            display.flush();
            display.set_backlight_level(100);
        }
        display
    }

    /// Configure MCU GPIO ports for parallel LCD interface and backlight.
    #[cfg(not(test))]
    fn init_hardware(&self) {
        let rcc = unsafe { &*pac::RCC::ptr() };
        let gpiob = unsafe { &*pac::GPIOB::ptr() };
        let gpiod = unsafe { &*pac::GPIOD::ptr() };
        let gpioe = unsafe { &*pac::GPIOE::ptr() };
        let gpiof = unsafe { &*pac::GPIOF::ptr() };

        unsafe {
            // Enable GPIOB, GPIOC, GPIOD, GPIOE, GPIOF in RCC_AHBENR
            // Bit 18 = GPIOB, 19 = GPIOC, 20 = GPIOD, 21 = GPIOE, 22 = GPIOF
            rcc.ahbenr.modify(|r, w| {
                w.bits(r.bits() | (1 << 18) | (1 << 19) | (1 << 20) | (1 << 21) | (1 << 22))
            });

            // Configure PE0..PE7 as outputs (MODER = 01)
            gpioe.moder.modify(|r, w| {
                w.bits((r.bits() & !0x0000_FFFF) | 0x0000_5555)
            });

            // Configure Control pins as outputs (MODER = 01):
            // PB3 (RS), PB4 (RST), PB5 (RW)
            let b_mask = (3 << 6) | (3 << 8) | (3 << 10);
            let b_val = (1 << 6) | (1 << 8) | (1 << 10);
            gpiob.moder.modify(|r, w| {
                w.bits((r.bits() & !b_mask) | b_val)
            });

            // PD2 (CS), PD7 (RD / Strobe)
            let d_mask = (3 << 4) | (3 << 14);
            let d_val = (1 << 4) | (1 << 14);
            gpiod.moder.modify(|r, w| {
                w.bits((r.bits() & !d_mask) | d_val)
            });

            // Standard Backlight is on PF3 (Active HIGH). Configure PF3 as output.
            gpiof.moder.modify(|r, w| {
                w.bits((r.bits() & !(3 << 6)) | (1 << 6))
            });

            // Default LCD states matching OpenI6X:
            // CS (PD2) = Low (chip enabled)
            // RW (PB5) = Low (write mode enabled)
            // RD (PD7) = High (strobe idle high)
            // RST (PB4) = High
            // RS (PB3) = High
            gpiod.bsrr.write(|w| w.bits((1 << (2 + 16)) | (1 << 7))); // PD2=0 (CS Low), PD7=1 (RD High)
            gpiob.bsrr.write(|w| w.bits((1 << (5 + 16)) | (1 << 4) | (1 << 3))); // PB5=0 (RW Low), PB4=1 (RST High), PB3=1 (RS High)
        }
    }

    /// Initialize TIM3_CH4 PWM on PC9 (AF0) for optional hardware backlight dimming mod.
    #[cfg(not(test))]
    fn init_backlight_pwm(&self) {
        let rcc = unsafe { &*pac::RCC::ptr() };
        let gpioc = unsafe { &*pac::GPIOC::ptr() };
        let tim3 = unsafe { &*pac::TIM3::ptr() };

        unsafe {
            // Enable TIM3 peripheral clock (RCC_APB1ENR bit 1)
            rcc.apb1enr.modify(|r, w| w.bits(r.bits() | (1 << 1)));

            // Configure PC9 as Alternate Function (MODER = 10, AF0 = TIM3_CH4)
            gpioc.moder.modify(|r, w| {
                w.bits((r.bits() & !(3 << 18)) | (2 << 18))
            });

            // AFRH: PC9 is pin 9 -> bits 7:4. AF0 is 0b0000
            gpioc.afrh.modify(|r, w| {
                w.bits(r.bits() & !(0xF << 4))
            });

            // Configure TIM3 for 1 kHz PWM on Channel 4:
            // 48 MHz clock: PSC = 47 -> 1 MHz tick. ARR = 999 -> 1000 ticks = 1 kHz.
            tim3.psc.write(|w| w.bits(47));
            tim3.arr.write(|w| w.bits(999));
            tim3.ccr4.write(|w| w.bits(999)); // Start at 100% duty

            // CCMR2: OC4M = 0b110 (PWM Mode 1), OC4PE = 1 (Preload enable)
            tim3.ccmr2_output().modify(|r, w| {
                w.bits((r.bits() & !(0x7F << 8)) | (6 << 12) | (1 << 11))
            });

            // CCER: CC4E = 1 (Enable CH4 output)
            tim3.ccer.modify(|r, w| w.bits(r.bits() | (1 << 12)));

            // Re-initialize registers (UG = 1)
            tim3.egr.write(|w| w.bits(1));

            // CR1: ARPE = 1, CEN = 1 (Enable counter)
            tim3.cr1.modify(|r, w| w.bits(r.bits() | (1 << 7) | (1 << 0)));
        }
    }

    /// Set Backlight brightness level (0..100%).
    /// Supports both stock factory backlight (PF3 on/off) and modded hardware (PC9 PWM dimming).
    pub fn set_backlight_level(&self, pct: u8) {
        #[cfg(not(test))]
        {
            let gpiof = unsafe { &*pac::GPIOF::ptr() };
            let tim3 = unsafe { &*pac::TIM3::ptr() };

            unsafe {
                if pct == 0 {
                    // Stock backlight OFF
                    gpiof.bsrr.write(|w| w.bits(1 << (3 + 16)));
                    // PC9 PWM duty 0
                    tim3.ccr4.write(|w| w.bits(0));
                } else {
                    // Stock backlight ON
                    gpiof.bsrr.write(|w| w.bits(1 << 3));
                    // PC9 PWM duty (0..999)
                    let duty = (pct.min(100) as u32 * 999) / 100;
                    tim3.ccr4.write(|w| w.bits(duty));
                }
            }
        }
        #[cfg(test)]
        let _ = pct;
    }

    /// Convenience helper for binary ON/OFF control.
    #[allow(dead_code)]
    pub fn set_backlight(&self, on: bool) {
        self.set_backlight_level(if on { 100 } else { 0 });
    }

    #[cfg(not(test))]
    #[inline(always)]
    fn delay_cycles(&self, count: u32) {
        cortex_m::asm::delay(count);
    }

    /// Send byte to ST7567 using 6800-series parallel strobe:
    /// Put byte on PE0..PE7, then pulse RD (PD7) High -> Low.
    #[inline]
    fn write_byte(&self, byte: u8, is_data: bool) {
        #[cfg(not(test))]
        {
            let gpiob = unsafe { &*pac::GPIOB::ptr() };
            let gpiod = unsafe { &*pac::GPIOD::ptr() };
            let gpioe = unsafe { &*pac::GPIOE::ptr() };

            unsafe {
                // Set RS: PB3 (1 for Data, 0 for Command)
                if is_data {
                    gpiob.bsrr.write(|w| w.bits(1 << 3));
                } else {
                    gpiob.bsrr.write(|w| w.bits(1 << (3 + 16)));
                }

                // Put byte on PE0..PE7 (low 8 bits of GPIOE_ODR)
                gpioe.odr.modify(|r, w| {
                    w.bits((r.bits() & !0xFF) | (byte as u32))
                });

                // Strobe RD (PD7): High then Low latches data (OpenI6X timing)
                gpiod.bsrr.write(|w| w.bits(1 << 7)); // RD High
                cortex_m::asm::nop();
                gpiod.bsrr.write(|w| w.bits(1 << (7 + 16))); // RD Low
            }
        }
        #[cfg(test)]
        {
            let _ = (byte, is_data);
        }
    }

    #[inline]
    pub fn write_cmd(&self, cmd: u8) {
        self.write_byte(cmd, false);
    }

    #[allow(dead_code)]
    #[inline]
    pub fn write_data(&self, data: u8) {
        self.write_byte(data, true);
    }

    /// Set ST7567 Electronic Volume (EV) contrast value (0..63).
    pub fn set_contrast(&self, ev: u8) {
        let val = ev.min(63);
        self.write_cmd(0x81); // LCD_CMD_EV (Electronic Volume)
        self.write_cmd(val);
    }

    /// Hardware reset and send OpenI6X-matched ST7567 initialization sequence.
    #[cfg(not(test))]
    fn init_controller(&self) {
        let gpiob = unsafe { &*pac::GPIOB::ptr() };

        unsafe {
            // Hardware reset: PB4 Low for ~50us, then High (OpenI6X delay_us(20))
            gpiob.bsrr.write(|w| w.bits(1 << (4 + 16))); // RST Low
            self.delay_cycles(500);
            gpiob.bsrr.write(|w| w.bits(1 << 4));         // RST High
            self.delay_cycles(500);
        }

        // Exact OpenI6X initialization sequence:
        self.write_cmd(0xE2); // LCD_CMD_RESET
        self.delay_cycles(10_000);

        self.write_cmd(0xAE); // LCD_CMD_DISPLAY_OFF
        self.write_cmd(0xA4); // LCD_CMD_MODE_RAM
        self.write_cmd(0xA3); // LCD_CMD_BIAS_1_7 (1/7 bias)
        self.write_cmd(0xC0); // LCD_CMD_COM_NORMAL (COM0 -> COM63)
        self.write_cmd(0xA1); // LCD_CMD_SEG_INVERSE (SEG131 -> SEG0) - Fixes upside-down!
        self.write_cmd(0x2F); // LCD_CMD_POWERCTRL_ALL_ON
        self.write_cmd(0x23); // LCD_CMD_REG_RATIO_011
        self.write_cmd(0x81); // LCD_CMD_EV (Electronic Volume)
        self.write_cmd(0x25); // Default contrast
        self.write_cmd(0x40); // LCD_CMD_SET_STARTLINE (0)
        self.write_cmd(0xB0); // LCD_CMD_SET_PAGESTART (0)
        self.write_cmd(0x04); // LCD_CMD_SET_COL_LO (start at line 4: 132 lines controller vs 128 LCD)
        self.write_cmd(0x10); // LCD_CMD_SET_COL_HI (0)
        self.write_cmd(0xAF); // LCD_CMD_DISPLAY_ON
        self.delay_cycles(10_000);
    }

    /// Flush the entire 1024-byte framebuffer to the ST7567 LCD.
    /// Uses OpenI6X direct 8-bit parallel bus strobing: RS set once per page,
    /// direct 8-bit STRB to GPIOE_ODR, and single-cycle RD pulses.
    pub fn flush(&self) {
        #[cfg(not(test))]
        {
            let gpiob = unsafe { &*pac::GPIOB::ptr() };
            let gpiod = unsafe { &*pac::GPIOD::ptr() };

            unsafe {
                // Low byte of GPIOE ODR register (offset 0x14 from GPIOE base)
                let data_odr_u8 = (pac::GPIOE::ptr() as *mut u8).add(0x14);

                for page in 0..8 {
                    // Command mode: PB3 Low
                    gpiob.bsrr.write(|w| w.bits(1 << (3 + 16)));

                    // Page selection (0xB0..0xB7)
                    core::ptr::write_volatile(data_odr_u8, 0xB0 | page as u8);
                    gpiod.bsrr.write(|w| w.bits(1 << 7));
                    gpiod.bsrr.write(|w| w.bits(1 << (7 + 16)));

                    // Column low nibble = 4 (controller is 132 col, LCD is 128)
                    core::ptr::write_volatile(data_odr_u8, 0x04);
                    gpiod.bsrr.write(|w| w.bits(1 << 7));
                    gpiod.bsrr.write(|w| w.bits(1 << (7 + 16)));

                    // Column high nibble = 0 (0x10)
                    core::ptr::write_volatile(data_odr_u8, 0x10);
                    gpiod.bsrr.write(|w| w.bits(1 << 7));
                    gpiod.bsrr.write(|w| w.bits(1 << (7 + 16)));

                    // Data mode: PB3 High (set ONCE for the entire 128-byte page!)
                    gpiob.bsrr.write(|w| w.bits(1 << 3));

                    let start = page * WIDTH;
                    let end = start + WIDTH;
                    for &byte in &self.framebuffer[start..end] {
                        core::ptr::write_volatile(data_odr_u8, byte);
                        gpiod.bsrr.write(|w| w.bits(1 << 7));
                        gpiod.bsrr.write(|w| w.bits(1 << (7 + 16)));
                    }
                }
            }
        }
    }

    /// Clear the framebuffer (all pixels off).
    #[allow(dead_code)]
    pub fn clear_buffer(&mut self) {
        self.framebuffer.fill(0);
    }

    /// Set a single pixel at (x, y) to on or off with bounds checking.
    #[inline]
    pub fn set_pixel(&mut self, x: i32, y: i32, on: bool) {
        if x >= 0 && (x as usize) < WIDTH && y >= 0 && (y as usize) < HEIGHT {
            let page = (y as usize) / 8;
            let bit = (y as usize) % 8;
            let index = page * WIDTH + (x as usize);
            if on {
                self.framebuffer[index] |= 1 << bit;
            } else {
                self.framebuffer[index] &= !(1 << bit);
            }
        }
    }

    /// Fast horizontal line from (x, y) with length `w`.
    pub fn draw_hline(&mut self, x: i32, y: i32, w: u32, on: bool) {
        if y < 0 || (y as usize) >= HEIGHT || w == 0 {
            return;
        }
        let x_start = x.max(0) as usize;
        let x_end = ((x + w as i32).max(0) as usize).min(WIDTH);
        if x_start >= x_end {
            return;
        }
        let page = (y as usize) / 8;
        let bit = (y as usize) % 8;
        let mask = 1u8 << bit;
        let start_idx = page * WIDTH + x_start;
        let end_idx = page * WIDTH + x_end;

        if on {
            for b in &mut self.framebuffer[start_idx..end_idx] {
                *b |= mask;
            }
        } else {
            let inv_mask = !mask;
            for b in &mut self.framebuffer[start_idx..end_idx] {
                *b &= inv_mask;
            }
        }
    }

    /// Fast vertical line from (x, y) with length `h`.
    pub fn draw_vline(&mut self, x: i32, y: i32, h: u32, on: bool) {
        if x < 0 || (x as usize) >= WIDTH || h == 0 {
            return;
        }
        let y_start = y.max(0) as usize;
        let y_end = ((y + h as i32).max(0) as usize).min(HEIGHT);
        if y_start >= y_end {
            return;
        }
        let start_page = y_start / 8;
        let end_page = (y_end - 1) / 8;
        let col = x as usize;

        for page in start_page..=end_page {
            let page_y_start = page * 8;
            let page_y_end = page_y_start + 7;
            let bit_start = y_start.saturating_sub(page_y_start);
            let bit_end = if y_end - 1 < page_y_end { (y_end - 1) - page_y_start } else { 7 };

            let mut mask = 0u8;
            for b in bit_start..=bit_end {
                mask |= 1 << b;
            }

            let idx = page * WIDTH + col;
            if on {
                self.framebuffer[idx] |= mask;
            } else {
                self.framebuffer[idx] &= !mask;
            }
        }
    }

    /// Fast filled rectangle from (x, y) with size (w, h).
    pub fn fill_rect(&mut self, x: i32, y: i32, w: u32, h: u32, on: bool) {
        if w == 0 || h == 0 {
            return;
        }
        let x_start = x.max(0) as usize;
        let x_end = ((x + w as i32).max(0) as usize).min(WIDTH);
        if x_start >= x_end {
            return;
        }
        let y_start = y.max(0) as usize;
        let y_end = ((y + h as i32).max(0) as usize).min(HEIGHT);
        if y_start >= y_end {
            return;
        }
        let start_page = y_start / 8;
        let end_page = (y_end - 1) / 8;

        for page in start_page..=end_page {
            let page_y_start = page * 8;
            let page_y_end = page_y_start + 7;
            let bit_start = y_start.saturating_sub(page_y_start);
            let bit_end = if y_end - 1 < page_y_end { (y_end - 1) - page_y_start } else { 7 };

            let mask = if bit_start == 0 && bit_end == 7 {
                0xFFu8
            } else {
                let mut m = 0u8;
                for b in bit_start..=bit_end {
                    m |= 1 << b;
                }
                m
            };

            let row_start = page * WIDTH + x_start;
            let row_end = page * WIDTH + x_end;

            if on {
                if mask == 0xFF {
                    self.framebuffer[row_start..row_end].fill(0xFF);
                } else {
                    for b in &mut self.framebuffer[row_start..row_end] {
                        *b |= mask;
                    }
                }
            } else {
                if mask == 0xFF {
                    self.framebuffer[row_start..row_end].fill(0x00);
                } else {
                    let inv_mask = !mask;
                    for b in &mut self.framebuffer[row_start..row_end] {
                        *b &= inv_mask;
                    }
                }
            }
        }
    }

    /// Fast outline rectangle from (x, y) with size (w, h).
    pub fn draw_rect(&mut self, x: i32, y: i32, w: u32, h: u32, on: bool) {
        if w == 0 || h == 0 {
            return;
        }
        self.draw_hline(x, y, w, on);
        if h > 1 {
            self.draw_hline(x, y + h as i32 - 1, w, on);
            if h > 2 {
                self.draw_vline(x, y + 1, h - 2, on);
                if w > 1 {
                    self.draw_vline(x + w as i32 - 1, y + 1, h - 2, on);
                }
            }
        }
    }

    #[cfg(test)]
    pub fn framebuffer(&self) -> &[u8; BUFFER_SIZE] {
        &self.framebuffer
    }
}

impl OriginDimensions for St7567 {
    fn size(&self) -> Size {
        Size::new(WIDTH as u32, HEIGHT as u32)
    }
}

impl DrawTarget for St7567 {
    type Color = BinaryColor;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(coord, color) in pixels.into_iter() {
            if coord.x >= 0 && coord.x < WIDTH as i32 && coord.y >= 0 && coord.y < HEIGHT as i32 {
                let x = coord.x as usize;
                let y = coord.y as usize;

                let page = y / 8;
                let bit = y % 8;
                let index = page * WIDTH + x;

                match color {
                    BinaryColor::On => {
                        self.framebuffer[index] |= 1 << bit;
                    }
                    BinaryColor::Off => {
                        self.framebuffer[index] &= !(1 << bit);
                    }
                }
            }
        }
        Ok(())
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        let fill = match color {
            BinaryColor::On => 0xFF,
            BinaryColor::Off => 0x00,
        };
        self.framebuffer.fill(fill);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_graphics::prelude::*;
    use embedded_graphics::primitives::{Line, PrimitiveStyle, Rectangle};

    #[test]
    fn test_draw_hline_matches_embedded_graphics() {
        let mut d1 = St7567::new();
        let mut d2 = St7567::new();

        // Direct primitive
        d1.draw_hline(10, 15, 50, true);

        // embedded_graphics
        Line::new(Point::new(10, 15), Point::new(59, 15))
            .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
            .draw(&mut d2)
            .unwrap();

        assert_eq!(d1.framebuffer(), d2.framebuffer());
    }

    #[test]
    fn test_draw_vline_matches_embedded_graphics() {
        let mut d1 = St7567::new();
        let mut d2 = St7567::new();

        // Direct primitive spanning multiple pages
        d1.draw_vline(25, 6, 20, true);

        // embedded_graphics
        Line::new(Point::new(25, 6), Point::new(25, 25))
            .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
            .draw(&mut d2)
            .unwrap();

        assert_eq!(d1.framebuffer(), d2.framebuffer());
    }

    #[test]
    fn test_fill_rect_matches_embedded_graphics() {
        let mut d1 = St7567::new();
        let mut d2 = St7567::new();

        // List item highlight shape (e.g. 2, 14, 124, 9)
        d1.fill_rect(2, 14, 124, 9, true);

        Rectangle::new(Point::new(2, 14), Size::new(124, 9))
            .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
            .draw(&mut d2)
            .unwrap();

        assert_eq!(d1.framebuffer(), d2.framebuffer());
    }

    #[test]
    fn test_draw_rect_matches_embedded_graphics() {
        let mut d1 = St7567::new();
        let mut d2 = St7567::new();

        d1.draw_rect(10, 10, 40, 20, true);

        Rectangle::new(Point::new(10, 10), Size::new(40, 20))
            .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
            .draw(&mut d2)
            .unwrap();

        assert_eq!(d1.framebuffer(), d2.framebuffer());
    }

    #[test]
    fn test_primitives_out_of_bounds_no_panic() {
        let mut d = St7567::new();
        // Negative coords, zero width/height, exceeding width/height
        d.draw_hline(-10, 20, 5, true);
        d.draw_hline(100, 20, 50, true);
        d.draw_hline(10, -5, 50, true);
        d.draw_hline(10, 100, 50, true);
        d.draw_vline(-10, 20, 5, true);
        d.draw_vline(20, -10, 5, true);
        d.draw_vline(20, 50, 30, true);
        d.fill_rect(-20, -20, 200, 200, true);
        d.draw_rect(-20, -20, 200, 200, true);
    }
}

