//! Continuous 11-channel ADC1 scanning via autonomous DMA1 Channel 1.
//!
//! Samples:
//! - CH0..CH3: PA0..PA3 (Gimbals: RV, RH, LV, LH)
//! - CH4, CH5: PA4, PA5 (Switches SA, SB)
//! - CH6:      PA6      (Potentiometer VR1 / VRA)
//! - CH7:      PA7      (Switch SC)
//! - CH8:      PB0      (Potentiometer VR2 / VRB)
//! - CH9:      PB1      (Switch SD)
//! - CH10:     PC0      (Battery Voltage sense)

#![allow(dead_code)]

#[cfg(not(test))]
use core::ptr;

#[cfg(not(test))]
use stm32f0xx_hal::pac;

pub const NUM_CHANNELS: usize = 11;

/// Minimum 12-bit ADC conversion count (0V).
pub const ADC_MIN: u16 = 0;
/// Theoretical midpoint of 12-bit ADC (VDD / 2).
pub const ADC_CENTER: u16 = 2048;
/// Maximum 12-bit ADC conversion count (VDD).
pub const ADC_MAX: u16 = 4095;

use core::cell::UnsafeCell;

struct AdcDmaBuffer(UnsafeCell<[u16; NUM_CHANNELS]>);
unsafe impl Sync for AdcDmaBuffer {}

/// Statically allocated buffer written directly by DMA1 hardware.
static ADC_RAW_BUFFER: AdcDmaBuffer = AdcDmaBuffer(UnsafeCell::new([0; NUM_CHANNELS]));
/// Stable snapshot of latest completed conversion sequence.
static LATEST_ADC_SNAPSHOT: AdcDmaBuffer = AdcDmaBuffer(UnsafeCell::new([2048; NUM_CHANNELS]));

// DMA CCR configuration for single scan:
// Bit 7:  MINC (memory increment)
// Bit 8:  PSIZE = 16-bit halfword (01)
// Bit 10: MSIZE = 16-bit halfword (01)
// Bits [13:12]: PL = High (10)
// Bit 0:  EN
const DMA_CCR_VAL: u32 = (1 << 7) | (1 << 8) | (1 << 10) | (2 << 12) | (1 << 0);

/// Wait for at least one complete 11-channel DMA conversion cycle and start the next.
pub fn wait_first_conversion() {
    #[cfg(not(test))]
    unsafe {
        let dma = &*pac::DMA1::ptr();
        let adc = &*pac::ADC::ptr();

        let mut timeout = 200_000u32;
        while dma.isr.read().tcif1().bit_is_clear() && timeout > 0 {
            timeout -= 1;
        }

        let raw_ptr = ADC_RAW_BUFFER.0.get();
        let snap_ptr = LATEST_ADC_SNAPSHOT.0.get();

        // Snapshot initial readings
        for i in 0..NUM_CHANNELS {
            (*snap_ptr)[i] = ptr::read_volatile(&(*raw_ptr)[i]);
        }

        // Clear DMA flags (TCIF1, HTIF1, TEIF1, GIF1)
        dma.ifcr.write(|w| w.bits(0x0F));
        // Clear ADC EOSEQ / OVR
        adc.isr.write(|w| w.bits((1 << 4) | (1 << 3)));

        // Re-arm DMA Channel 1
        dma.ch1.cr.write(|w| w.bits(0));
        dma.ch1.ndtr.write(|w| w.ndt().bits(NUM_CHANNELS as u16));
        dma.ch1.cr.write(|w| w.bits(DMA_CCR_VAL));

        // Start next conversion sequence
        adc.cr.modify(|_, w| w.adstart().set_bit());
    }
}

/// Initialize GPIO analog pins, ADC1, and DMA1 Channel 1 for single-sequence scan.
pub fn init() {
    #[cfg(not(test))]
    unsafe {
        let rcc = &*pac::RCC::ptr();
        let gpioa = &*pac::GPIOA::ptr();
        let gpiob = &*pac::GPIOB::ptr();
        let gpioc = &*pac::GPIOC::ptr();
        let adc = &*pac::ADC::ptr();
        let dma = &*pac::DMA1::ptr();

        // 1. Enable GPIOA, GPIOB, GPIOC, DMA1 in RCC_AHBENR
        // Bit 0: DMA1EN, Bit 17: GPIOA, Bit 18: GPIOB, Bit 19: GPIOC
        rcc.ahbenr.modify(|_, w| {
            w.dmaen().set_bit()
             .iopaen().set_bit()
             .iopben().set_bit()
             .iopcen().set_bit()
        });

        // 2. Enable ADC1 clock in RCC_APB2ENR (Bit 9 / adcen)
        rcc.apb2enr.modify(|_, w| w.adcen().set_bit());

        // 3. Configure Pins as Analog Mode (MODER = 11, PUPDR = 00):
        // PA0..PA7: (only touch bits [15:0] so USB, UART, Buzzer on PA8..PA15 are untouched)
        gpioa.moder.modify(|r, w| w.bits((r.bits() & !0x0000_FFFF) | 0x0000_FFFF));
        gpioa.pupdr.modify(|r, w| w.bits(r.bits() & !0x0000_FFFF));

        // PB0, PB1: (pins 0 and 1, bits [3:0])
        gpiob.moder.modify(|r, w| w.bits((r.bits() & !0x0000_000F) | 0x0000_000F));
        gpiob.pupdr.modify(|r, w| w.bits(r.bits() & !0x0000_000F));

        // PC0: (pin 0, bits [1:0])
        gpioc.moder.modify(|r, w| w.bits((r.bits() & !0x0000_0003) | 0x0000_0003));
        gpioc.pupdr.modify(|r, w| w.bits(r.bits() & !0x0000_0003));

        // 4. ADC Clock Selection: Synchronous PCLK/4 (CKMODE = 10 in CFGR2)
        adc.cfgr2.write(|w| w.ckmode().bits(0b10));

        // 5. Enable ADC1 with timeout protection (ADCAL is omitted to match OpenI6X and avoid silicon hang)
        adc.isr.write(|w| w.adrdy().set_bit()); // clear ADRDY by writing 1
        adc.cr.modify(|_, w| w.aden().set_bit()); // ADEN
        let mut timeout = 100_000u32;
        while adc.isr.read().adrdy().bit_is_clear() && timeout > 0 {
            timeout -= 1;
        }

        // 6. Configure Channels (0 through 10)
        // 0x07FF = bits [10:0] set
        adc.chselr.write(|w| w.bits(0x07FF));

        // 7. Sampling Time: 239.5 ADC cycles (SMPR = 0b111) for low-noise sampling
        adc.smpr.write(|w| w.smp().bits(0b111));

        // 8. Configure ADC CFGR1:
        // Bit 0: DMAEN (1: DMA enable)
        // Bit 1: DMACFG = 0 (one-shot DMA mode)
        // Bit 13: CONT = 0 (single sequence conversion)
        adc.cfgr1.write(|w| w.dmaen().set_bit());

        // 9. Configure DMA1 Channel 1:
        let ch1 = &dma.ch1;
        ch1.cr.write(|w| w.bits(0));
        ch1.par.write(|w| w.bits(core::ptr::addr_of!((*pac::ADC::ptr()).dr) as u32));
        ch1.mar.write(|w| w.bits(ADC_RAW_BUFFER.0.get() as u32));
        ch1.ndtr.write(|w| w.ndt().bits(NUM_CHANNELS as u16));
        ch1.cr.write(|w| w.bits(DMA_CCR_VAL));

        // 10. Start first 11-channel conversion sequence
        adc.cr.modify(|_, w| w.adstart().set_bit());
    }
}

/// Read a safe, synchronized snapshot of the latest 11 raw ADC values.
/// Automatically re-arms DMA and restarts conversion sequence when finished.
pub fn read_raw() -> [u16; NUM_CHANNELS] {
    #[cfg(not(test))]
    unsafe {
        let raw_ptr = ADC_RAW_BUFFER.0.get();
        let snap_ptr = LATEST_ADC_SNAPSHOT.0.get();
        let dma = &*pac::DMA1::ptr();
        let adc = &*pac::ADC::ptr();

        // If the current sequence is complete (TCIF1 = bit 1 of DMA1_ISR):
        if dma.isr.read().tcif1().bit_is_set() {
            // Snapshot all 11 channels cleanly
            for i in 0..NUM_CHANNELS {
                (*snap_ptr)[i] = ptr::read_volatile(&(*raw_ptr)[i]);
            }

            // Clear DMA flags for Channel 1
            dma.ifcr.write(|w| w.bits(0x0F));
            // Clear ADC EOSEQ and OVR
            adc.isr.write(|w| w.bits((1 << 4) | (1 << 3)));

            // Re-arm DMA Channel 1
            dma.ch1.cr.write(|w| w.bits(0));
            dma.ch1.ndtr.write(|w| w.ndt().bits(NUM_CHANNELS as u16));
            dma.ch1.cr.write(|w| w.bits(DMA_CCR_VAL));

            // Trigger next conversion sequence
            adc.cr.modify(|_, w| w.adstart().set_bit());
        }

        *snap_ptr
    }
    #[cfg(test)]
    {
        [ADC_CENTER; NUM_CHANNELS]
    }
}
