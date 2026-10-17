//! Continuous 11-channel / 15-channel ADC1 scanning via autonomous DMA1 Channel 1.
//!
//! Samples:
//! - CH0..CH3: PA0..PA3 (Gimbals: RV, RH, LV, LH)
//! - CH4, CH5: PA4, PA5 (Switches SA, SB)
//! - CH6:      PA6      (Potentiometer VR1 / VRA)
//! - CH7:      PA7      (Switch SC)
//! - CH8:      PB0      (Potentiometer VR2 / VRB)
//! - CH9:      PB1      (Switch SD)
//! - CH10:     PC0      (Battery Voltage sense)
//!
//! ### Additional ADC Channels for Hardware Modding (STM32F072VBT6 LQFP100):
//! The FS-i6S transmitter utilizes an STM32F072VBT6 (100-pin LQFP) MCU.
//! On this package, ADC1 has up to 16 external channels. Channels on PC1..PC4:
//! - `PC1` (ADC1_IN11, Pin 16): Auxiliary Pot / Slider VRC (index 11)
//! - `PC2` (ADC1_IN12, Pin 17): Auxiliary Pot / Slider VRD (index 12)
//! - `PC3` (ADC1_IN13, Pin 18): Auxiliary Pot / 6-pos VRE (index 13)
//! - `PC4` (ADC1_IN14, Pin 33): Auxiliary Pot / Sensor VRF (index 14)

#[cfg(not(test))]
use core::ptr;
use core::sync::atomic::{AtomicBool, Ordering};

pub const NUM_STD_CHANNELS: usize = 11;
pub const NUM_EXT_CHANNELS: usize = 15;
pub const NUM_CHANNELS: usize = NUM_EXT_CHANNELS;

/// Minimum 12-bit ADC conversion count (0V).
pub const ADC_MIN: u16 = 0;
/// Theoretical midpoint of 12-bit ADC (VDD / 2).
pub const ADC_CENTER: u16 = 2048;
/// Maximum 12-bit ADC conversion count (VDD).
pub const ADC_MAX: u16 = 4095;

static EXT_ADC_ENABLED: AtomicBool = AtomicBool::new(false);

use core::cell::UnsafeCell;

struct AdcDmaBuffer(UnsafeCell<[u16; NUM_CHANNELS]>);
unsafe impl Sync for AdcDmaBuffer {}

/// Statically allocated buffer written directly by DMA1 hardware.
#[cfg(not(test))]
static ADC_RAW_BUFFER: AdcDmaBuffer = AdcDmaBuffer(UnsafeCell::new([0; NUM_CHANNELS]));
/// Stable snapshot of latest completed conversion sequence.
static LATEST_ADC_SNAPSHOT: AdcDmaBuffer = AdcDmaBuffer(UnsafeCell::new([2048; NUM_CHANNELS]));

#[cfg(not(test))]
use stm32f0xx_hal::pac;

pub fn is_ext_adc_enabled() -> bool {
    EXT_ADC_ENABLED.load(Ordering::Relaxed)
}

#[inline(always)]
pub fn is_p7_enabled() -> bool {
    is_ext_adc_enabled()
}

#[inline(always)]
pub fn set_p7_enabled(enabled: bool) {
    set_ext_adc_enabled(enabled);
}

// DMA CCR configuration for single scan:
// Bit 7:  MINC (memory increment)
// Bit 8:  PSIZE = 16-bit halfword (01)
// Bit 10: MSIZE = 16-bit halfword (01)
// Bits [13:12]: PL = High (10)
// Bit 0:  EN
#[cfg(not(test))]
const DMA_CCR_VAL: u32 = (1 << 7) | (1 << 8) | (1 << 10) | (2 << 12) | (1 << 0);

/// Wait for at least one complete DMA conversion cycle and start the next.
pub fn wait_first_conversion() {
    #[cfg(not(test))]
    unsafe {
        let ext_enabled = EXT_ADC_ENABLED.load(Ordering::Relaxed);
        let ch_count = if ext_enabled { NUM_EXT_CHANNELS } else { NUM_STD_CHANNELS };
        let dma = &*pac::DMA1::ptr();
        let adc = &*pac::ADC::ptr();

        let mut timeout = 200_000u32;
        while dma.isr.read().tcif1().bit_is_clear() && timeout > 0 {
            timeout -= 1;
        }

        let raw_ptr = ADC_RAW_BUFFER.0.get();
        let snap_ptr = LATEST_ADC_SNAPSHOT.0.get();

        // Snapshot initial readings
        for i in 0..ch_count {
            (*snap_ptr)[i] = ptr::read_volatile(&(*raw_ptr)[i]);
        }
        if !ext_enabled {
            for i in NUM_STD_CHANNELS..NUM_EXT_CHANNELS {
                (*snap_ptr)[i] = ADC_CENTER;
            }
        }

        // Clear DMA flags (TCIF1, HTIF1, TEIF1, GIF1)
        dma.ifcr.write(|w| w.bits(0x0F));
        // Clear ADC EOSEQ / OVR
        adc.isr.write(|w| w.bits((1 << 4) | (1 << 3)));

        // Re-arm DMA Channel 1
        dma.ch1.cr.write(|w| w.bits(0));
        dma.ch1.ndtr.write(|w| w.ndt().bits(ch_count as u16));
        dma.ch1.cr.write(|w| w.bits(DMA_CCR_VAL));

        // Start next conversion sequence
        adc.cr.modify(|_, w| w.adstart().set_bit());
    }
}

/// Dynamically enable or disable extended 15-channel scanning (adding PC1..PC4).
pub fn set_ext_adc_enabled(enabled: bool) {
    EXT_ADC_ENABLED.store(enabled, Ordering::Relaxed);
    #[cfg(not(test))]
    unsafe {
        reconfigure_scan(enabled);
    }
}

#[cfg(not(test))]
unsafe fn reconfigure_scan(enabled: bool) {
    let adc = &*pac::ADC::ptr();
    let dma = &*pac::DMA1::ptr();

    // 1. Disable DMA Channel 1
    dma.ch1.cr.write(|w| w.bits(0));

    // 2. Stop ongoing conversion if ADSTART is set
    if adc.cr.read().adstart().bit_is_set() {
        adc.cr.modify(|_, w| w.adstp().set_bit());
        let mut timeout = 100_000u32;
        while adc.cr.read().adstart().bit_is_set() && timeout > 0 {
            timeout -= 1;
        }
    }

    // 3. Clear DMA and ADC flags
    dma.ifcr.write(|w| w.bits(0x0F));
    adc.isr.write(|w| w.bits((1 << 4) | (1 << 3)));

    // 4. Update CHSELR
    let chselr = if enabled { 0x7FFF } else { 0x07FF };
    adc.chselr.write(|w| w.bits(chselr));

    // 5. Update CNDTR and re-enable DMA
    let ch_count = if enabled { NUM_EXT_CHANNELS } else { NUM_STD_CHANNELS };
    dma.ch1.ndtr.write(|w| w.ndt().bits(ch_count as u16));
    dma.ch1.cr.write(|w| w.bits(DMA_CCR_VAL));

    // 6. Restart conversion
    adc.cr.modify(|_, w| w.adstart().set_bit());
}

/// Initialize GPIO analog pins, ADC1, and DMA1 Channel 1 for continuous scan.
pub fn init(ext_adc_en: bool) {
    EXT_ADC_ENABLED.store(ext_adc_en, Ordering::Relaxed);
    #[cfg(not(test))]
    unsafe {
        let rcc = &*pac::RCC::ptr();
        let gpioa = &*pac::GPIOA::ptr();
        let gpiob = &*pac::GPIOB::ptr();
        let gpioc = &*pac::GPIOC::ptr();
        let adc = &*pac::ADC::ptr();
        let dma = &*pac::DMA1::ptr();

        // 1. Enable GPIOA, GPIOB, GPIOC, DMA1 in RCC_AHBENR
        rcc.ahbenr.modify(|_, w| {
            w.dmaen().set_bit()
             .iopaen().set_bit()
             .iopben().set_bit()
             .iopcen().set_bit()
        });

        // 2. Enable ADC1 clock in RCC_APB2ENR
        rcc.apb2enr.modify(|_, w| w.adcen().set_bit());

        // 3. Configure Pins as Analog Mode (MODER = 11, PUPDR = 00):
        // PA0..PA7: (only touch bits [15:0] so USB, UART, Buzzer on PA8..PA15 are untouched)
        gpioa.moder.modify(|r, w| w.bits((r.bits() & !0x0000_FFFF) | 0x0000_FFFF));
        gpioa.pupdr.modify(|r, w| w.bits(r.bits() & !0x0000_FFFF));

        // PB0, PB1: (pins 0 and 1, bits [3:0])
        gpiob.moder.modify(|r, w| w.bits((r.bits() & !0x0000_000F) | 0x0000_000F));
        gpiob.pupdr.modify(|r, w| w.bits(r.bits() & !0x0000_000F));

        // PC0..PC4: (pins 0 through 4, bits [9:0])
        gpioc.moder.modify(|r, w| w.bits((r.bits() & !0x0000_03FF) | 0x0000_03FF));
        gpioc.pupdr.modify(|r, w| w.bits(r.bits() & !0x0000_03FF));

        // 4. ADC Clock Selection: Synchronous PCLK/4 (CKMODE = 10 in CFGR2)
        adc.cfgr2.write(|w| w.ckmode().bits(0b10));

        // 5. Enable ADC1 with timeout protection (ADCAL is omitted to match OpenI6X and avoid silicon hang)
        adc.isr.write(|w| w.adrdy().set_bit()); // clear ADRDY by writing 1
        adc.cr.modify(|_, w| w.aden().set_bit()); // ADEN
        let mut timeout = 100_000u32;
        while adc.isr.read().adrdy().bit_is_clear() && timeout > 0 {
            timeout -= 1;
        }

        // 6. Configure Channels (0..10 or 0..14)
        // 0x07FF = bits [10:0] set; 0x7FFF = bits [14:0] set
        let chselr = if ext_adc_en { 0x7FFF } else { 0x07FF };
        adc.chselr.write(|w| w.bits(chselr));

        // 7. Sampling Time: 239.5 ADC cycles (SMPR = 0b111) for low-noise sampling
        adc.smpr.write(|w| w.smp().bits(0b111));

        // 8. Configure ADC CFGR1:
        // Bit 0: DMAEN (1: DMA enable)
        // Bit 1: DMACFG = 0 (one-shot DMA mode)
        // Bit 13: CONT = 0 (single sequence conversion)
        adc.cfgr1.write(|w| w.dmaen().set_bit());

        // 9. Configure DMA1 Channel 1:
        let ch_count = if ext_adc_en { NUM_EXT_CHANNELS } else { NUM_STD_CHANNELS };
        let ch1 = &dma.ch1;
        ch1.cr.write(|w| w.bits(0));
        ch1.par.write(|w| w.bits(core::ptr::addr_of!((*pac::ADC::ptr()).dr) as u32));
        ch1.mar.write(|w| w.bits(ADC_RAW_BUFFER.0.get() as u32));
        ch1.ndtr.write(|w| w.ndt().bits(ch_count as u16));
        ch1.cr.write(|w| w.bits(DMA_CCR_VAL));

        // 10. Start first conversion sequence
        adc.cr.modify(|_, w| w.adstart().set_bit());
    }
}

/// Read a safe, synchronized snapshot of the latest raw ADC values.
/// Automatically re-arms DMA and restarts conversion sequence when finished.
pub fn read_raw() -> [u16; NUM_CHANNELS] {
    #[cfg(not(test))]
    unsafe {
        let ext_enabled = EXT_ADC_ENABLED.load(Ordering::Relaxed);
        let ch_count = if ext_enabled { NUM_EXT_CHANNELS } else { NUM_STD_CHANNELS };

        let raw_ptr = ADC_RAW_BUFFER.0.get();
        let snap_ptr = LATEST_ADC_SNAPSHOT.0.get();
        let dma = &*pac::DMA1::ptr();
        let adc = &*pac::ADC::ptr();

        // If the current sequence is complete (TCIF1 = bit 1 of DMA1_ISR):
        if dma.isr.read().tcif1().bit_is_set() {
            // Snapshot active channels cleanly
            for i in 0..ch_count {
                (*snap_ptr)[i] = ptr::read_volatile(&(*raw_ptr)[i]);
            }
            if !ext_enabled {
                for i in NUM_STD_CHANNELS..NUM_EXT_CHANNELS {
                    (*snap_ptr)[i] = ADC_CENTER;
                }
            }

            // Clear DMA flags for Channel 1
            dma.ifcr.write(|w| w.bits(0x0F));
            // Clear ADC EOSEQ and OVR
            adc.isr.write(|w| w.bits((1 << 4) | (1 << 3)));

            // Re-arm DMA Channel 1
            dma.ch1.cr.write(|w| w.bits(0));
            dma.ch1.ndtr.write(|w| w.ndt().bits(ch_count as u16));
            dma.ch1.cr.write(|w| w.bits(DMA_CCR_VAL));

            // Trigger next conversion sequence
            adc.cr.modify(|_, w| w.adstart().set_bit());
        }

        *snap_ptr
    }
    #[cfg(test)]
    unsafe {
        *LATEST_ADC_SNAPSHOT.0.get()
    }
}

#[cfg(test)]
pub fn set_test_adc_snapshot(snapshot: [u16; NUM_CHANNELS]) {
    unsafe {
        *LATEST_ADC_SNAPSHOT.0.get() = snapshot;
    }
}
