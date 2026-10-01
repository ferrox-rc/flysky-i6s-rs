//! Continuous 11-channel / 15-channel ADC1 scanning via autonomous DMA1 Channel 1.
//!
//! Standard Samples (CH0..CH10):
//! - CH0..CH3: PA0..PA3 (Gimbals: RV, RH, LV, LH)
//! - CH4, CH5: PA4, PA5 (Switches SA, SB)
//! - CH6:      PA6      (Potentiometer VR1 / VRA)
//! - CH7:      PA7      (Potentiometer VR2 / VRB)
//! - CH8:      PB0      (Switch SC)
//! - CH9:      PB1      (Switch SD)
//! - CH10:     PC0      (Battery Voltage sense)
//!
//! Header P7 Samples (CH12..CH15, indices 11..14):
//! - CH12:     PC2      (Auxiliary Pot / Slider VRC)
//! - CH13:     PC3      (Auxiliary Pot / Slider VRD)
//! - CH14:     PC4      (Auxiliary Pot / 6-pos VRE)
//! - CH15:     PC5      (Auxiliary Pot / Sensor VRF)

use core::ptr;

pub const NUM_STD_CHANNELS: usize = 11;
pub const NUM_CHANNELS: usize = 15;

/// Minimum 12-bit ADC conversion count (0V).
pub const ADC_MIN: u16 = 0;
/// Theoretical midpoint of 12-bit ADC (VDD / 2).
pub const ADC_CENTER: u16 = 2048;
/// Maximum 12-bit ADC conversion count (VDD).
pub const ADC_MAX: u16 = 4095;

use core::cell::UnsafeCell;

struct AdcDmaBuffer(UnsafeCell<[u16; NUM_CHANNELS]>);
unsafe impl Sync for AdcDmaBuffer {}

struct AdcChannelCount(UnsafeCell<usize>);
unsafe impl Sync for AdcChannelCount {}

/// Number of active channels currently scanned by DMA1 (11 or 15).
static SCAN_CHANNELS: AdcChannelCount = AdcChannelCount(UnsafeCell::new(NUM_STD_CHANNELS));

/// Statically allocated buffer written directly by DMA1 hardware.
static ADC_RAW_BUFFER: AdcDmaBuffer = AdcDmaBuffer(UnsafeCell::new([0; NUM_CHANNELS]));
/// Stable snapshot of latest completed conversion sequence.
static LATEST_ADC_SNAPSHOT: AdcDmaBuffer = AdcDmaBuffer(UnsafeCell::new([2048; NUM_CHANNELS]));

// Base register addresses for STM32F072
const RCC_AHBENR: *mut u32 = 0x4002_1014 as *mut u32;
const RCC_APB2ENR: *mut u32 = 0x4002_1018 as *mut u32;

const GPIOA_MODER: *mut u32 = 0x4800_0000 as *mut u32;
const GPIOA_PUPDR: *mut u32 = 0x4800_000C as *mut u32;

const GPIOB_MODER: *mut u32 = 0x4800_0400 as *mut u32;
const GPIOB_PUPDR: *mut u32 = 0x4800_040C as *mut u32;

const GPIOC_MODER: *mut u32 = 0x4800_0800 as *mut u32;
const GPIOC_PUPDR: *mut u32 = 0x4800_080C as *mut u32;

const ADC1_BASE: usize = 0x4001_2400;
const ADC1_ISR: *mut u32 = ADC1_BASE as *mut u32;
const ADC1_CR: *mut u32 = (ADC1_BASE + 0x08) as *mut u32;
const ADC1_CFGR1: *mut u32 = (ADC1_BASE + 0x0C) as *mut u32;
const ADC1_CFGR2: *mut u32 = (ADC1_BASE + 0x10) as *mut u32;
const ADC1_SMPR: *mut u32 = (ADC1_BASE + 0x14) as *mut u32;
const ADC1_CHSELR: *mut u32 = (ADC1_BASE + 0x28) as *mut u32;
const ADC1_DR: *mut u32 = (ADC1_BASE + 0x40) as *mut u32;

const DMA1_BASE: usize = 0x4002_0000;
const DMA1_ISR: *mut u32 = DMA1_BASE as *mut u32;
const DMA1_IFCR: *mut u32 = (DMA1_BASE + 0x04) as *mut u32;

const DMA1_CH1_BASE: usize = 0x4002_0008;
const DMA1_CH1_CCR: *mut u32 = DMA1_CH1_BASE as *mut u32;
const DMA1_CH1_CNDTR: *mut u32 = (DMA1_CH1_BASE + 0x04) as *mut u32;
const DMA1_CH1_CPAR: *mut u32 = (DMA1_CH1_BASE + 0x08) as *mut u32;
const DMA1_CH1_CMAR: *mut u32 = (DMA1_CH1_BASE + 0x0C) as *mut u32;

// DMA CCR configuration for single scan:
// Bit 7:  MINC (memory increment)
// Bit 8:  PSIZE = 16-bit halfword (01)
// Bit 10: MSIZE = 16-bit halfword (01)
// Bits [13:12]: PL = High (10)
// Bit 0:  EN
const DMA_CCR_VAL: u32 = (1 << 7) | (1 << 8) | (1 << 10) | (2 << 12) | (1 << 0);

/// Wait for at least one complete DMA conversion cycle and start the next.
pub fn wait_first_conversion() {
    unsafe {
        let ch_count = *SCAN_CHANNELS.0.get();
        let mut timeout = 200_000u32;
        while (ptr::read_volatile(DMA1_ISR) & (1 << 1)) == 0 && timeout > 0 {
            timeout -= 1;
        }

        let raw_ptr = ADC_RAW_BUFFER.0.get();
        let snap_ptr = LATEST_ADC_SNAPSHOT.0.get();

        // Snapshot initial readings
        for i in 0..ch_count {
            (*snap_ptr)[i] = ptr::read_volatile(&(*raw_ptr)[i]);
        }
        if ch_count < NUM_CHANNELS {
            for i in ch_count..NUM_CHANNELS {
                (*snap_ptr)[i] = ADC_CENTER;
            }
        }

        // Clear DMA flags (TCIF1, HTIF1, TEIF1, GIF1)
        ptr::write_volatile(DMA1_IFCR, 0x0F);
        // Clear ADC EOSEQ / OVR
        ptr::write_volatile(ADC1_ISR, (1 << 4) | (1 << 3));

        // Re-arm DMA Channel 1
        ptr::write_volatile(DMA1_CH1_CCR, 0);
        ptr::write_volatile(DMA1_CH1_CNDTR, ch_count as u32);
        ptr::write_volatile(DMA1_CH1_CCR, DMA_CCR_VAL);

        // Start next conversion sequence
        ptr::write_volatile(ADC1_CR, ptr::read_volatile(ADC1_CR) | (1 << 2)); // ADSTART
    }
}

/// Initialize GPIO analog pins, ADC1, and DMA1 Channel 1 for single-sequence scan.
pub fn init(p7_enabled: bool) {
    unsafe {
        let ch_count = if p7_enabled { NUM_CHANNELS } else { NUM_STD_CHANNELS };
        let chselr = if p7_enabled { 0xF7FF } else { 0x07FF };
        *SCAN_CHANNELS.0.get() = ch_count;

        // 1. Enable GPIOA, GPIOB, GPIOC, DMA1 in RCC_AHBENR
        // Bit 0: DMA1EN, Bit 17: GPIOA, Bit 18: GPIOB, Bit 19: GPIOC
        let ahb = ptr::read_volatile(RCC_AHBENR);
        ptr::write_volatile(
            RCC_AHBENR,
            ahb | (1 << 0) | (1 << 17) | (1 << 18) | (1 << 19),
        );

        // 2. Enable ADC1 clock in RCC_APB2ENR (Bit 9)
        let apb2 = ptr::read_volatile(RCC_APB2ENR);
        ptr::write_volatile(RCC_APB2ENR, apb2 | (1 << 9));

        // 3. Configure Pins as Analog Mode (MODER = 11, PUPDR = 00):
        // PA0..PA7: (only touch bits [15:0] so USB, UART, Buzzer on PA8..PA15 are untouched)
        let a_moder = ptr::read_volatile(GPIOA_MODER);
        ptr::write_volatile(GPIOA_MODER, (a_moder & !0x0000_FFFF) | 0x0000_FFFF);
        let a_pupdr = ptr::read_volatile(GPIOA_PUPDR);
        ptr::write_volatile(GPIOA_PUPDR, a_pupdr & !0x0000_FFFF);

        // PB0, PB1: (pins 0 and 1, bits [3:0])
        let b_moder = ptr::read_volatile(GPIOB_MODER);
        ptr::write_volatile(GPIOB_MODER, (b_moder & !0x0000_000F) | 0x0000_000F);
        let b_pupdr = ptr::read_volatile(GPIOB_PUPDR);
        ptr::write_volatile(GPIOB_PUPDR, b_pupdr & !0x0000_000F);

        // PC0 (battery) and PC2..PC5 (AD12..AD15 on Header P7): (MODER = 11, PUPDR = 00)
        // PC0: bits [1:0]
        // PC2: bits [5:4]
        // PC3: bits [7:6]
        // PC4: bits [9:8]
        // PC5: bits [11:10]
        // Total mask: 0x0000_0FF3
        let c_moder = ptr::read_volatile(GPIOC_MODER);
        ptr::write_volatile(GPIOC_MODER, (c_moder & !0x0000_0FF3) | 0x0000_0FF3);
        let c_pupdr = ptr::read_volatile(GPIOC_PUPDR);
        ptr::write_volatile(GPIOC_PUPDR, c_pupdr & !0x0000_0FF3);

        // 4. ADC Clock Selection: Synchronous PCLK/4 (CKMODE = 10 in CFGR2)
        ptr::write_volatile(ADC1_CFGR2, 2 << 30);

        // 5. Enable ADC1 with timeout protection (ADCAL is omitted to match OpenI6X and avoid silicon hang)
        ptr::write_volatile(ADC1_ISR, 1 << 0); // clear ADRDY by writing 1
        ptr::write_volatile(ADC1_CR, 1 << 0);  // ADEN
        let mut timeout = 100_000u32;
        while (ptr::read_volatile(ADC1_ISR) & (1 << 0)) == 0 && timeout > 0 {
            timeout -= 1;
        }

        // 6. Configure Channels (0..10 or 0..10 + 12..15)
        ptr::write_volatile(ADC1_CHSELR, chselr);

        // 7. Sampling Time: 239.5 ADC cycles (SMPR = 0b111) for low-noise sampling
        ptr::write_volatile(ADC1_SMPR, 0x07);

        // 8. Configure ADC CFGR1:
        // Bit 0: DMAEN (1: DMA enable)
        // Bit 1: DMACFG = 0 (one-shot DMA mode)
        // Bit 13: CONT = 0 (single sequence conversion)
        ptr::write_volatile(ADC1_CFGR1, 1 << 0);

        // 9. Configure DMA1 Channel 1:
        ptr::write_volatile(DMA1_CH1_CCR, 0);
        ptr::write_volatile(DMA1_CH1_CPAR, ADC1_DR as u32);
        ptr::write_volatile(DMA1_CH1_CMAR, ADC_RAW_BUFFER.0.get() as u32);
        ptr::write_volatile(DMA1_CH1_CNDTR, ch_count as u32);
        ptr::write_volatile(DMA1_CH1_CCR, DMA_CCR_VAL);

        // 10. Start first conversion sequence
        ptr::write_volatile(ADC1_CR, ptr::read_volatile(ADC1_CR) | (1 << 2)); // ADSTART
    }
}

/// Enable or disable 15-channel scanning for P7 header AD12..AD15 at runtime.
pub fn set_p7_enabled(enabled: bool) {
    unsafe {
        let ch_count = if enabled { NUM_CHANNELS } else { NUM_STD_CHANNELS };
        let chselr = if enabled { 0xF7FF } else { 0x07FF };

        // 1. Disable DMA Channel 1
        ptr::write_volatile(DMA1_CH1_CCR, 0);

        // 2. Stop ADC conversion if in progress
        let cr = ptr::read_volatile(ADC1_CR);
        if (cr & (1 << 2)) != 0 {
            ptr::write_volatile(ADC1_CR, cr | (1 << 4)); // ADSTP
            let mut timeout = 10_000u32;
            while (ptr::read_volatile(ADC1_CR) & (1 << 2)) != 0 && timeout > 0 {
                timeout -= 1;
            }
        }

        // 3. Update active scan channel selection and count
        ptr::write_volatile(ADC1_CHSELR, chselr);
        *SCAN_CHANNELS.0.get() = ch_count;

        // 4. Clear DMA & ADC flags
        ptr::write_volatile(DMA1_IFCR, 0x0F);
        ptr::write_volatile(ADC1_ISR, (1 << 4) | (1 << 3));

        // 5. Re-arm DMA Channel 1
        ptr::write_volatile(DMA1_CH1_CNDTR, ch_count as u32);
        ptr::write_volatile(DMA1_CH1_CCR, DMA_CCR_VAL);

        // 6. Restart conversion
        ptr::write_volatile(ADC1_CR, ptr::read_volatile(ADC1_CR) | (1 << 2)); // ADSTART
    }
}

/// Check if P7 header 15-channel scanning is currently active.
pub fn is_p7_enabled() -> bool {
    unsafe { *SCAN_CHANNELS.0.get() == NUM_CHANNELS }
}

/// Read a safe, synchronized snapshot of the latest raw ADC values.
/// Automatically re-arms DMA and restarts conversion sequence when finished.
pub fn read_raw() -> [u16; NUM_CHANNELS] {
    unsafe {
        let ch_count = *SCAN_CHANNELS.0.get();
        let raw_ptr = ADC_RAW_BUFFER.0.get();
        let snap_ptr = LATEST_ADC_SNAPSHOT.0.get();

        // If the current sequence is complete (TCIF1 = bit 1 of DMA1_ISR):
        if (ptr::read_volatile(DMA1_ISR) & (1 << 1)) != 0 {
            // Snapshot active channels cleanly
            for i in 0..ch_count {
                (*snap_ptr)[i] = ptr::read_volatile(&(*raw_ptr)[i]);
            }
            if ch_count < NUM_CHANNELS {
                for i in ch_count..NUM_CHANNELS {
                    (*snap_ptr)[i] = ADC_CENTER;
                }
            }

            // Clear DMA flags for Channel 1
            ptr::write_volatile(DMA1_IFCR, 0x0F);
            // Clear ADC EOSEQ and OVR
            ptr::write_volatile(ADC1_ISR, (1 << 4) | (1 << 3));

            // Re-arm DMA Channel 1
            ptr::write_volatile(DMA1_CH1_CCR, 0);
            ptr::write_volatile(DMA1_CH1_CNDTR, ch_count as u32);
            ptr::write_volatile(DMA1_CH1_CCR, DMA_CCR_VAL);

            // Trigger next conversion sequence
            ptr::write_volatile(ADC1_CR, ptr::read_volatile(ADC1_CR) | (1 << 2)); // ADSTART
        }

        *snap_ptr
    }
}
