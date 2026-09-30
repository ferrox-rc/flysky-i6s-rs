//! Driver for the Amiccom A7105 2.4 GHz FSK/GFSK Transceiver.

#![allow(dead_code)]

use super::spi;

// A7105 Command Strobes
pub const STROBE_SLEEP: u8 = 0x80;
pub const STROBE_IDLE: u8 = 0x90;
pub const STROBE_STANDBY: u8 = 0xA0;
pub const STROBE_PLL: u8 = 0xB0;
pub const STROBE_RX: u8 = 0xC0;
pub const STROBE_TX: u8 = 0xD0;
pub const STROBE_RST_WRPTR: u8 = 0xE0;
pub const STROBE_RST_RDPTR: u8 = 0xF0;

// A7105 Registers
pub const REG_MODE: u8 = 0x00;
pub const REG_MODE_CONTROL: u8 = 0x01;
pub const REG_CALC: u8 = 0x02;
pub const REG_FIFO_I: u8 = 0x03;
pub const REG_FIFO_II: u8 = 0x04;
pub const REG_FIFO_DATA: u8 = 0x05;
pub const REG_ID_DATA: u8 = 0x06;
pub const REG_GPIO1: u8 = 0x0B;
pub const REG_GPIO2: u8 = 0x0C;
pub const REG_CLOCK: u8 = 0x0D;
pub const REG_DATA_RATE: u8 = 0x0E;
pub const REG_PLL_I: u8 = 0x0F; // Channel register
pub const REG_PLL_II: u8 = 0x10;
pub const REG_PLL_III: u8 = 0x11;
pub const REG_PLL_IV: u8 = 0x12;
pub const REG_PLL_V: u8 = 0x13;
pub const REG_TX_I: u8 = 0x14;
pub const REG_TX_II: u8 = 0x15;
pub const REG_DELAY_I: u8 = 0x16;
pub const REG_DELAY_II: u8 = 0x17;
pub const REG_RX: u8 = 0x18;
pub const REG_RX_GAIN_I: u8 = 0x19;
pub const REG_RX_GAIN_II: u8 = 0x1A;
pub const REG_RX_GAIN_III: u8 = 0x1B;
pub const REG_RX_GAIN_IV: u8 = 0x1C;
pub const REG_RSSI_THOLD: u8 = 0x1D;
pub const REG_CODE_I: u8 = 0x1F;
pub const REG_CODE_II: u8 = 0x20;
pub const REG_VCO_CURCAL: u8 = 0x24;
pub const REG_VCO_SBCAL_I: u8 = 0x25;
pub const REG_VCO_SBCAL_II: u8 = 0x26;
pub const REG_TX_TEST: u8 = 0x28;

// Power levels (PAC << 3 | TBG)
pub const POWER_100UW: u8 = 0x00; // -23 dBm
pub const POWER_1MW: u8 = 0x02;   // -16 dBm
pub const POWER_10MW: u8 = 0x0D;  //  -6 dBm
pub const POWER_100MW: u8 = 0x1F; //  +1 dBm into PA (Stock High Power)

pub const BIND_POWER: u8 = POWER_100UW;
pub const HIGH_POWER: u8 = POWER_100MW;

/// A7105 Register Initialization Table for AFHDS 2A (500 kbps GFSK)
/// Entries matching 0xFF are skipped (unmodified).
static AFHDS2A_A7105_REGS: [u8; 50] = [
    0xFF,
    0xC2 | (1 << 5), // 01: Mode Control (Enable FIFO mode + FCRC)
    0x00,            // 02: Calc
    0x25,            // 03: FIFO I (FIFO margin)
    0x00,            // 04: FIFO II
    0xFF,            // 05: FIFO Data
    0xFF,            // 06: ID Data
    0x00,            // 07: RC OSC I
    0x00,            // 08: RC OSC II
    0x00,            // 09: RC OSC III
    0x00,            // 0A: CKO Pin
    0x19,            // 0B: GPIO1 (SDO output)
    0x01,            // 0C: GPIO2 (WTR output: Low = Tx/Rx done)
    0x05,            // 0D: Clock
    0x00,            // 0E: Data Rate (500 kbps)
    0x50,            // 0F: PLL I (Initial Channel = 80)
    0x9E,            // 10: PLL II
    0x4B,            // 11: PLL III (2400 MHz base frequency)
    0x00,            // 12: PLL IV
    0x02,            // 13: PLL V
    0x16,            // 14: TX I
    0x2B,            // 15: TX II
    0x12,            // 16: Delay I
    0x4F,            // 17: Delay II
    0x62,            // 18: RX
    0x80,            // 19: RX Gain I
    0xFF,            // 1A: RX Gain II
    0xFF,            // 1B: RX Gain III
    0x2A,            // 1C: RX Gain IV
    0x32,            // 1D: RSSI Threshold
    0xC3,            // 1E: ADC
    0x1E,            // 1F: Code I
    0x1E,            // 20: Code II
    0xFF,            // 21: Code III
    0x00,            // 22: IF Calib I
    0xFF,            // 23: IF Calib II
    0x00,            // 24: VCO Curcal
    0x00,            // 25: VCO SBCAL I
    0x3B,            // 26: VCO SBCAL II
    0x00,            // 27: Battery Det
    0x17,            // 28: TX Test (Power)
    0x47,            // 29: RX Dem Test I
    0x80,            // 2A: RX Dem Test II
    0x03,            // 2B: CPC
    0x01,            // 2C: XTAL Test
    0x45,            // 2D: PLL Test
    0x18,            // 2E: VCO Test I
    0x00,            // 2F: VCO Test II
    0x01,            // 30: IFAT
    0x0F,            // 31: RSCALE
];

/// Send a command strobe to the A7105.
#[inline(always)]
pub fn strobe(cmd: u8) {
    spi::csn_low();
    spi::write_byte(cmd);
    spi::csn_high();
}

/// Write a single byte to an A7105 register.
pub fn write_reg(addr: u8, data: u8) {
    spi::csn_low();
    spi::write_byte(addr & 0x3F); // Address bits [5:0], bit 6 = 0 for write
    spi::write_byte(data);
    spi::csn_high();
}

/// Read a single byte from an A7105 register.
pub fn read_reg(addr: u8) -> u8 {
    spi::csn_low();
    spi::write_byte((addr & 0x3F) | 0x40); // bit 6 = 1 for read
    let val = spi::read_byte();
    spi::csn_high();
    val
}

use core::sync::atomic::{AtomicU8, Ordering};

pub static LAST_CHIP_ID: AtomicU8 = AtomicU8::new(0);

/// Perform a hardware reset and verify communications.
/// Returns true if the A7105 responds with its factory reset signature (PLL_II = 0x9E).
pub fn reset() -> bool {
    for _ in 0..5 {
        strobe(STROBE_STANDBY);
        for _ in 0..2000 { cortex_m::asm::nop(); }

        // Reset digital core
        write_reg(REG_MODE, 0x00);

        // Delay ~2 ms for internal analog circuits to reset
        for _ in 0..16000 {
            cortex_m::asm::nop();
        }

        // CRITICAL: Configure GIO1 as 4-wire SPI SDO output (0x19)
        // By default on reset, GIO1 is High-Z and MISO reads 0x00!
        write_reg(REG_GPIO1, 0x19);
        write_reg(REG_GPIO2, 0x01); // GIO2 = WTR

        spi::set_tx_rx_mode(spi::RF_MODE_OFF);

        let sig = read_reg(REG_PLL_II);
        LAST_CHIP_ID.store(sig, Ordering::Relaxed);

        strobe(STROBE_STANDBY);

        if sig == 0x9E {
            return true;
        }
    }

    false
}

/// Write the 32-bit radio preamble/sync ID to the A7105.
pub fn write_id(id: u32) {
    spi::csn_low();
    spi::write_byte(REG_ID_DATA);
    spi::write_byte(((id >> 24) & 0xFF) as u8);
    spi::write_byte(((id >> 16) & 0xFF) as u8);
    spi::write_byte(((id >> 8) & 0xFF) as u8);
    spi::write_byte((id & 0xFF) as u8);
    spi::csn_high();
}

/// Initialize the A7105 with AFHDS 2A register values and perform internal calibrations.
pub fn init() {
    // 1. Write FlySky standard sync ID (0x5475C52A)
    write_id(0x5475_C52A);

    // 2. Load register table
    for (addr, &val) in AFHDS2A_A7105_REGS.iter().enumerate() {
        if val != 0xFF {
            write_reg(addr as u8, val);
        }
    }

    strobe(STROBE_STANDBY);

    // 3. Calibrations recommended by A7105 datasheet:
    // IF Filter Bank Calibration
    write_reg(REG_CALC, 1);
    let mut timeout = 10_000u32;
    while (read_reg(REG_CALC) & 1) != 0 && timeout > 0 {
        timeout -= 1;
    }

    // VCO Current Calibration
    write_reg(REG_VCO_CURCAL, 0x13);

    // VCO Bank Calibration
    write_reg(REG_VCO_SBCAL_II, 0x3B);

    // Calibrate channel 0
    write_reg(REG_PLL_I, 0);
    write_reg(REG_CALC, 2);
    timeout = 10_000;
    while (read_reg(REG_CALC) & 2) != 0 && timeout > 0 {
        timeout -= 1;
    }

    // Calibrate channel 0xA0 (160)
    write_reg(REG_PLL_I, 0xA0);
    write_reg(REG_CALC, 2);
    timeout = 10_000;
    while (read_reg(REG_CALC) & 2) != 0 && timeout > 0 {
        timeout -= 1;
    }

    // Reset VCO Band Calibration to default center
    write_reg(REG_VCO_SBCAL_I, 0x0A);

    // 4. Default to high power and standby
    spi::set_tx_rx_mode(spi::RF_MODE_TX_EN);
    set_power(HIGH_POWER);
    strobe(STROBE_STANDBY);
}

/// Set output power level.
pub fn set_power(power: u8) {
    write_reg(REG_TX_TEST, power);
}

/// Write packet payload into TX FIFO, set RF channel, and strobe transmission.
/// Forces STANDBY mode before writing FIFO to prevent pointer corruption during RX.
pub fn write_fifo(data: &[u8], channel: u8) {
    // 1. Force standby mode and switch front-end to TX before touching FIFO,
    // aborting any active RX demodulation to prevent FIFO pointer corruption.
    strobe(STROBE_STANDBY);
    spi::set_tx_rx_mode(spi::RF_MODE_TX_EN);

    // 2. Reset write pointer and load packet payload into TX FIFO
    spi::csn_low();
    spi::write_byte(STROBE_RST_WRPTR);
    spi::write_byte(REG_FIFO_DATA);
    for &b in data {
        spi::write_byte(b);
    }
    spi::csn_high();

    // 3. Set PLL channel and strobe transmission
    write_reg(REG_PLL_I, channel);
    strobe(STROBE_TX);
}

/// Read packet payload from RX FIFO.
pub fn read_fifo(buf: &mut [u8]) {
    strobe(STROBE_RST_RDPTR);
    spi::csn_low();
    spi::write_byte(0x40 | REG_FIFO_DATA);
    for b in buf.iter_mut() {
        *b = spi::read_byte();
    }
    spi::csn_high();
}

/// Check if the received packet passed hardware CRC verification.
/// In A7105, bit 5 of REG_MODE is CRCF (0 = CRC OK, 1 = CRC Error).
#[inline(always)]
pub fn is_crc_ok() -> bool {
    (read_reg(REG_MODE) & (1 << 5)) == 0
}

/// Read current receiver signal strength indicator (RSSI).
/// Returns raw RSSI register value (lower is stronger signal).
#[inline(always)]
pub fn read_raw_rssi() -> u8 {
    read_reg(REG_RSSI_THOLD)
}
