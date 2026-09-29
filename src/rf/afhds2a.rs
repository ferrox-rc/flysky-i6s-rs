//! AFHDS 2A (Automatic Frequency Hopping Digital System 2nd Gen) Protocol.
//!
//! Implements:
//! - Deterministic 16-channel spread-spectrum frequency hopping from TX ID
//! - 14-channel 38-byte control frame generation (988..2012 µs pulse widths)
//! - 4-phase bidirectional bind sequence
//! - 37-byte telemetry frame parsing (RSSI, RX battery voltage)

#![allow(dead_code)]

use super::{a7105, spi};
use crate::mixer::{CHANNEL_CENTER_US, CHANNEL_MAX_US, CHANNEL_MIN_US};

pub const NUM_CHANNELS: usize = 18;
pub const NUM_FREQ: usize = 16;
pub const TX_PACKET_SIZE: usize = 38;
pub const RX_PACKET_SIZE: usize = 37;
pub const FAILSAFE_THROTTLE_US: u16 = CHANNEL_MIN_US; // 988 µs motor cutoff

// Packet command bytes
pub const PACKET_STICKS: u8 = 0x58;
pub const PACKET_FAILSAFE: u8 = 0x56;
pub const PACKET_SETTINGS: u8 = 0xAA;
pub const PACKET_BIND1: u8 = 0xBB;
pub const PACKET_BIND2: u8 = 0xBC;

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum RadioMode {
    Normal,
    Binding,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum PacketType {
    Sticks,
    Settings,
    Failsafe,
}

#[derive(Copy, Clone, Debug, PartialEq)]
enum SubState {
    WaitingTxDone,
    ListeningRx,
    Idle,
}

#[derive(Copy, Clone, Debug)]
pub struct TelemetryData {
    pub connected: bool,
    pub rssi: u8,           // 0..100%
    pub rx_voltage_mv: u16, // in millivolts (e.g. 5000 for 5.00V)
    pub packets_sent: u32,
    pub packets_received: u32,
}

impl TelemetryData {
    pub const fn new() -> Self {
        Self {
            connected: false,
            rssi: 0,
            rx_voltage_mv: 0,
            packets_sent: 0,
            packets_received: 0,
        }
    }
}

/// Read persisted receiver ID from Flash if previously bound.
pub fn load_saved_rx_id() -> Option<u32> {
    #[cfg(test)]
    return None;

    #[cfg(not(test))]
    crate::storage::load_saved_rx_id()
}

pub struct Afhds2a {
    pub tx_id: u32,
    pub rx_id: u32,
    pub hopping_table: [u8; NUM_FREQ],
    pub hopping_idx: u8,
    pub mode: RadioMode,
    sub_state: SubState,
    bind_phase: u8,
    pub bind_confirm_count: u8,
    pub bind_done: bool,
    pub rx_id_needs_save: bool,
    pub next_packet_type: PacketType,
    pub channels: [u16; NUM_CHANNELS],
    pub telemetry: TelemetryData,
    loss_counter: u16,
}

impl Afhds2a {
    pub fn new(tx_id: u32) -> Self {
        let hopping_table = calculate_hopping_table(tx_id);
        let rx_id = load_saved_rx_id().unwrap_or(0xFFFF_FFFF);
        let is_initially_bound = rx_id != 0xFFFF_FFFF && rx_id != 0;
        Self {
            tx_id,
            rx_id,
            hopping_table,
            hopping_idx: 0,
            mode: RadioMode::Normal,
            sub_state: SubState::Idle,
            bind_phase: 0,
            bind_confirm_count: 0,
            bind_done: is_initially_bound,
            rx_id_needs_save: false,
            next_packet_type: PacketType::Sticks,
            channels: [CHANNEL_CENTER_US; NUM_CHANNELS], // All centered CHANNEL_CENTER_US µs
            telemetry: TelemetryData::new(),
            loss_counter: 0,
        }
    }

    /// Update 18 RC channel pulse widths (988..2012 µs).
    pub fn set_channels(&mut self, chs: &[u16; NUM_CHANNELS]) {
        self.channels.copy_from_slice(chs);
    }

    /// Enter or exit binding mode.
    pub fn set_bind_mode(&mut self, enable: bool) {
        if enable {
            self.mode = RadioMode::Binding;
            self.bind_phase = 1;
            self.bind_confirm_count = 0;
            self.bind_done = false;
            a7105::set_power(a7105::BIND_POWER);
        } else {
            self.mode = RadioMode::Normal;
            self.bind_phase = 0;
            self.bind_confirm_count = 0;
            self.bind_done = true;
            self.next_packet_type = PacketType::Sticks;
            a7105::set_power(a7105::HIGH_POWER);

            // If an RX ID was not captured (one-way receiver like FS-A8S, Fli14, etc.),
            // assign a deterministic non-zero ID derived from TX ID and mark for save!
            if self.rx_id == 0 || self.rx_id == 0xFFFF_FFFF {
                self.rx_id = 0x0001_0000 | (self.tx_id & 0xFFFF);
            }
            self.rx_id_needs_save = true;
        }
    }

    /// Dynamically update receiver ID (called when switching active model profile).
    pub fn set_rx_id(&mut self, rx_id: u32) {
        self.rx_id = if rx_id != 0 { rx_id } else { 0xFFFF_FFFF };
        self.bind_done = rx_id != 0 && rx_id != 0xFFFF_FFFF;
        self.loss_counter = 0;
        self.telemetry.connected = false;
    }

    /// Periodic timer callback (called every 3.85 ms from TIM16 ISR).
    pub fn on_timer_tick(&mut self) {
        // Prevent race conditions with GIO2 during packet generation and SPI setup
        spi::disable_gio2_irq();

        self.telemetry.packets_sent = self.telemetry.packets_sent.wrapping_add(1);

        // Connection loss detection (no RX packets received for > 200 ms ~ 52 packets)
        self.loss_counter = self.loss_counter.saturating_add(1);
        if self.loss_counter > 52 {
            self.telemetry.connected = false;
            self.telemetry.rssi = 0;
        }

        let mut tx_buf = [0u8; TX_PACKET_SIZE];
        let channel: u8;

        match self.mode {
            RadioMode::Binding => {
                // Alternate bind channels between 0x0D (13) and 0x8C (140)
                channel = if (self.telemetry.packets_sent & 1) != 0 {
                    0x0D
                } else {
                    0x8C
                };
                self.build_bind_packet(&mut tx_buf);

                // In BIND4: send 4 confirmation packets before hopping
                if self.bind_phase == 4 {
                    self.bind_confirm_count = self.bind_confirm_count.saturating_add(1);
                    if self.bind_confirm_count >= 4 {
                        self.mode = RadioMode::Normal;
                        self.bind_phase = 0;
                        self.bind_confirm_count = 0;
                        self.bind_done = true;
                        self.rx_id_needs_save = true;
                        self.hopping_idx = 1;
                        self.next_packet_type = PacketType::Sticks;
                        a7105::set_power(a7105::HIGH_POWER);
                    }
                }
            }
            RadioMode::Normal => {
                channel = self.hopping_table[(self.hopping_idx as usize) & 0x0F];
                self.hopping_idx = (self.hopping_idx + 1) & 0x0F;

                // Alternate antenna on each frequency hop for spatial diversity
                if self.hopping_idx != 0 {
                    spi::switch_antenna();
                }

                // Autonomous periodic failsafe broadcast: every 1,569 packets (~6.0s at 260 Hz),
                // refresh receiver failsafe register memory even if downlink frames were dropped.
                if self.next_packet_type == PacketType::Sticks
                    && self.telemetry.packets_sent.is_multiple_of(1569)
                {
                    self.next_packet_type = PacketType::Failsafe;
                }

                match self.next_packet_type {
                    PacketType::Settings => {
                        self.build_settings_packet(&mut tx_buf);
                        self.next_packet_type = PacketType::Sticks;
                    }
                    PacketType::Failsafe => {
                        self.build_failsafe_packet(&mut tx_buf);
                        self.next_packet_type = PacketType::Sticks;
                    }
                    PacketType::Sticks => {
                        self.build_stick_packet(&mut tx_buf);
                    }
                }
            }
        }

        // Transmit packet on selected RF channel
        a7105::write_fifo(&tx_buf, channel);

        // Wait for GIO2 to signal TX completion
        self.sub_state = SubState::WaitingTxDone;
        spi::enable_gio2_irq();
    }

    /// Event handler for A7105 GIO2 pin falling edge (from EXTI2_3 ISR).
    pub fn on_gio2_event(&mut self) {
        spi::disable_gio2_irq();

        match self.sub_state {
            SubState::WaitingTxDone => {
                // TX completed! Switch to RX mode to listen for downlink telemetry
                self.sub_state = SubState::ListeningRx;

                if self.mode == RadioMode::Binding {
                    // Turn LNA off during bind RX to prevent front-end swamping at near range (< 20 cm)
                    spi::set_tx_rx_mode(spi::RF_MODE_OFF);
                    // Cycle bind searching phases 1 -> 2 -> 3 -> 1
                    if self.bind_phase < 4 {
                        self.bind_phase = (self.bind_phase % 3) + 1;
                    }
                } else {
                    // Normal mode: enable RX LNA for maximum downlink sensitivity
                    spi::set_tx_rx_mode(spi::RF_MODE_RX_EN);
                }

                a7105::strobe(a7105::STROBE_RX);
                spi::enable_gio2_irq();
            }
            SubState::ListeningRx => {
                // Packet received in RX FIFO!
                self.sub_state = SubState::Idle;

                if a7105::is_crc_ok() {
                    let mut rx_buf = [0u8; RX_PACKET_SIZE];
                    a7105::read_fifo(&mut rx_buf);
                    self.process_rx_packet(&rx_buf);
                }

                // Cleanly return to standby; GIO2 IRQ remains disabled until next on_timer_tick
                a7105::strobe(a7105::STROBE_STANDBY);
            }
            SubState::Idle => {}
        }
    }

    fn build_stick_packet(&self, out: &mut [u8; TX_PACKET_SIZE]) {
        out[0] = PACKET_STICKS;
        out[1..5].copy_from_slice(&self.tx_id.to_le_bytes());
        out[5..9].copy_from_slice(&self.rx_id.to_le_bytes());

        // 1. Pack base 14 channels (lower 12 bits) into 14 slots (bytes 9..36)
        for ch in 0..14 {
            let val = self.channels[ch].clamp(CHANNEL_MIN_US, CHANNEL_MAX_US);
            out[9 + ch * 2] = (val & 0xFF) as u8;
            out[10 + ch * 2] = ((val >> 8) & 0x0F) as u8;
        }

        // 2. Interleave channels 15..18 (indices 14..17) across the upper nibbles of slots 0..11
        // Matches Betaflight / iNav / Cleanflight rx/ibus.c updateChannelData unpacking:
        // for (i = IBUS_MAX_SLOTS, offset = ibusChannelOffset + 1; i < IBUS_MAX_CHANNEL; i++, offset += 6) {
        //     ibusChannelData[i] = ((ibus[offset] & 0xF0) >> 4) | (ibus[offset + 2] & 0xF0) | ((ibus[offset + 4] & 0xF0) << 4);
        // }
        for ext_ch in 0..4 {
            let val = self.channels[14 + ext_ch].clamp(CHANNEL_MIN_US, CHANNEL_MAX_US);
            let base_slot = ext_ch * 3;

            // Bits 0..3 -> High nibble of slot (base_slot + 0)
            out[10 + base_slot * 2] |= ((val & 0x000F) << 4) as u8;
            // Bits 4..7 -> High nibble of slot (base_slot + 1)
            out[10 + (base_slot + 1) * 2] |= (val & 0x00F0) as u8;
            // Bits 8..11 -> High nibble of slot (base_slot + 2)
            out[10 + (base_slot + 2) * 2] |= ((val >> 4) & 0x00F0) as u8;
        }

        out[37] = 0x00;
    }

    fn build_settings_packet(&self, out: &mut [u8; TX_PACKET_SIZE]) {
        out[0] = PACKET_SETTINGS; // 0xAA
        out[1..5].copy_from_slice(&self.tx_id.to_le_bytes());
        out[5..9].copy_from_slice(&self.rx_id.to_le_bytes());
        out[9] = 0xFD;
        out[10] = 0xFF;
        out[11] = 0x90; // 400 Hz servo refresh rate (low byte: 400 = 0x0190)
        out[12] = 0x01; // high byte
        out[13] = 0x00; // PWM output enabled (0x00 = PWM, 0x01 = PPM)
        out[14] = 0x00;
        out[15..37].fill(0xFF);
        let center_bytes = CHANNEL_CENTER_US.to_be_bytes();
        out[18] = center_bytes[0];
        out[19] = center_bytes[1]; // 1500 µs center pulse
        out[20] = 0x05;
        out[21] = 0xDE; // i-BUS serial output enabled (0xDE = i-BUS, 0xDD = SBUS)
        out[37] = 0x00;
    }

    fn build_failsafe_packet(&self, out: &mut [u8; TX_PACKET_SIZE]) {
        out[0] = PACKET_FAILSAFE; // 0x56
        out[1..5].copy_from_slice(&self.tx_id.to_le_bytes());
        out[5..9].copy_from_slice(&self.rx_id.to_le_bytes());

        // AFHDS 2A failsafe packets carry 14 channels (each 16 bits = 28 bytes filling bytes 9..36)
        for ch in 0..14 {
            if ch == 2 {
                // CH3 (Throttle): failsafe cutoff to FAILSAFE_THROTTLE_US (motor stop)
                let fs_bytes = FAILSAFE_THROTTLE_US.to_le_bytes();
                out[9 + ch * 2] = fs_bytes[0];
                out[10 + ch * 2] = fs_bytes[1];
            } else {
                // All other channels: Hold last position (0xFFFF)
                out[9 + ch * 2] = 0xFF;
                out[10 + ch * 2] = 0xFF;
            }
        }

        out[37] = 0x00;
    }

    fn build_bind_packet(&self, out: &mut [u8; TX_PACKET_SIZE]) {
        out[1..5].copy_from_slice(&self.tx_id.to_le_bytes());
        out[10] = 0x00;

        match self.bind_phase {
            1 => {
                out[0] = PACKET_BIND1; // 0xBB
                out[5..9].fill(0xFF);
                out[9] = 0x01;
                out[11..27].copy_from_slice(&self.hopping_table);
                out[27..37].fill(0xFF);
            }
            2 => {
                out[0] = PACKET_BIND2; // 0xBC
                out[5..9].fill(0xFF);
                out[9] = 0x00;
                out[11..27].copy_from_slice(&self.hopping_table);
                out[27] = 0x01;
                out[28] = 0x80;
                out[29..37].fill(0xFF);
            }
            3 => {
                out[0] = PACKET_BIND2; // 0xBC
                out[5..9].fill(0xFF);
                out[9] = 0x01;
                out[11..27].copy_from_slice(&self.hopping_table);
                out[27] = 0x01;
                out[28] = 0x80;
                out[29..37].fill(0xFF);
            }
            _ => {
                // BIND4 confirmation packet
                out[0] = PACKET_BIND2; // 0xBC
                out[5..9].copy_from_slice(&self.rx_id.to_le_bytes());
                out[9] = 0x02;
                out[11..27].fill(0xFF);
                out[27] = 0x01;
                out[28] = 0x80;
                out[29..37].fill(0xFF);
            }
        }

        out[37] = 0x00;
    }

    fn process_rx_packet(&mut self, rx: &[u8; RX_PACKET_SIZE]) {
        if self.mode == RadioMode::Binding {
            // Receiver responding to bind request
            if rx[0] == PACKET_BIND2 && rx[9] == 0x01 {
                let mut rid_bytes = [0u8; 4];
                rid_bytes.copy_from_slice(&rx[5..9]);
                self.rx_id = u32::from_le_bytes(rid_bytes);
                self.bind_phase = 4;
                self.bind_confirm_count = 0;
            }
            return;
        }

        // Standard downlink frames (0xAA or 0xAC)
        if rx[0] == 0xAA || rx[0] == 0xAC {
            // Verify TX ID
            let mut tx_bytes = [0u8; 4];
            tx_bytes.copy_from_slice(&rx[1..5]);
            if u32::from_le_bytes(tx_bytes) != self.tx_id {
                return;
            }

            // Verify or capture RX ID
            let mut rx_bytes = [0u8; 4];
            rx_bytes.copy_from_slice(&rx[5..9]);
            let rid = u32::from_le_bytes(rx_bytes);
            let is_placeholder = self.rx_id == 0xFFFF_FFFF
                || self.rx_id == 0
                || (self.rx_id & 0xFFFF_0000) == 0x0001_0000;
            if is_placeholder && rid != 0xFFFF_FFFF && rid != 0 {
                self.rx_id = rid;
                self.rx_id_needs_save = true;
                self.bind_done = true;
            } else if rid != self.rx_id && !is_placeholder {
                return;
            } else {
                self.bind_done = true;
            }

            // Downlink requests from receiver
            if rx[0] == 0xAA && rx[9] == 0xFC {
                // Receiver is actively requesting configuration settings!
                self.next_packet_type = PacketType::Settings;
                return;
            } else if rx[0] == 0xAA && rx[9] == 0xFD {
                // Receiver is requesting failsafe channel configuration!
                self.next_packet_type = PacketType::Failsafe;
                return;
            }

            self.telemetry.connected = true;
            self.telemetry.packets_received = self.telemetry.packets_received.wrapping_add(1);
            self.loss_counter = 0;

            // Compute signal strength (RSSI) from A7105 RSSI threshold register
            let raw_rssi = a7105::read_raw_rssi();
            let rssi_val = 256i32 - ((raw_rssi as i32 * 8) / 5);
            self.telemetry.rssi = (rssi_val.clamp(0, 100)) as u8;

            // Parse IBUS sensors (starting at byte 10, 4 bytes per sensor: [id, inst, val_lo, val_hi])
            let mut offset = 10;
            while offset + 4 <= RX_PACKET_SIZE {
                let sensor_id = rx[offset];
                let val = (rx[offset + 2] as u16) | ((rx[offset + 3] as u16) << 8);

                match sensor_id {
                    0x00 => {
                        // Internal receiver voltage in 0.01V units -> convert to mV
                        self.telemetry.rx_voltage_mv = val * 10;
                    }
                    0x03 => {
                        // External battery voltage
                        self.telemetry.rx_voltage_mv = val * 10;
                    }
                    0xFE => {
                        // Link Quality indicator (100 - error_rate)
                        let lqi = (100u16).saturating_sub(val);
                        if lqi <= 100 {
                            self.telemetry.rssi = lqi as u8;
                        }
                    }
                    _ => {}
                }

                offset += 4;
            }
        }
    }
}

/// Generate 16 spread-spectrum hopping channels from unique 32-bit TX ID.
pub fn calculate_hopping_table(tx_id: u32) -> [u8; NUM_FREQ] {
    let mut hopping = [0u8; NUM_FREQ];
    let mut idx = 0;
    let mut rnd = tx_id;
    let tx_byte3 = ((tx_id >> 24) & 0xFF) as u8;
    let mut attempts = 0u32;

    while idx < NUM_FREQ {
        attempts += 1;
        let band_no = (((idx << 1) | ((idx >> 1) & 0x01)) as u8 + tx_byte3) & 0x03;
        rnd = rnd.wrapping_mul(0x0019_660D).wrapping_add(0x3C6E_F35F);

        let next_ch = band_no * 41 + 1 + (((rnd >> idx) % 41) as u8);

        // Relax channel separation if excessive collisions occur on pathological UIDs
        let min_sep = if attempts > 500 {
            1
        } else if attempts > 200 {
            3
        } else {
            5
        };

        let mut valid = true;
        for &h in hopping.iter().take(idx) {
            if next_ch.abs_diff(h) < min_sep {
                valid = false;
                break;
            }
        }

        if valid {
            hopping[idx] = next_ch;
            idx += 1;
        }
    }

    hopping
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_stick_packet_18ch_roundtrip() {
        let mut radio = Afhds2a::new(0x1234_5678);
        radio.rx_id = 0x9ABC_DEF0;

        let test_channels: [u16; 18] = [
            1500, // CH1
            988,  // CH2 (min)
            2012, // CH3 (max)
            1123, // CH4
            1456, // CH5
            1789, // CH6
            1357, // CH7
            1642, // CH8
            1890, // CH9
            1050, // CH10
            1555, // CH11
            1999, // CH12
            1001, // CH13
            1620, // CH14
            1234, // CH15 (ext 0)
            1987, // CH16 (ext 1)
            1432, // CH17 (ext 2)
            1876, // CH18 (ext 3)
        ];
        radio.set_channels(&test_channels);

        let mut packet = [0u8; TX_PACKET_SIZE];
        radio.build_stick_packet(&mut packet);

        assert_eq!(packet[0], PACKET_STICKS);
        assert_eq!(&packet[1..5], &0x1234_5678u32.to_le_bytes());
        assert_eq!(&packet[5..9], &0x9ABC_DEF0u32.to_le_bytes());

        // Decode using exact Betaflight updateChannelData logic from rx/ibus.c:
        // In iBus frame:
        // offset 0: 0x20, offset 1: 0x40.
        // channel slots begin at offset 2 (which corresponds to packet[9..37]).
        let slot_bytes = &packet[9..37]; // 28 bytes
        let mut decoded = [0u16; 18];

        // 1. Standard channels 0..13 (slots 0..13)
        for i in 0..14 {
            let offset = i * 2;
            decoded[i] =
                (slot_bytes[offset] as u16) | (((slot_bytes[offset + 1] & 0x0F) as u16) << 8);
        }

        // 2. Extended channels 14..17 (CH15..CH18)
        for i in 0..4 {
            let offset = (i * 3) * 2 + 1;
            let val = ((slot_bytes[offset] & 0xF0) >> 4) as u16
                | ((slot_bytes[offset + 2] & 0xF0) as u16)
                | (((slot_bytes[offset + 4] & 0xF0) as u16) << 4);
            decoded[14 + i] = val;
        }

        for ch in 0..18 {
            assert_eq!(
                decoded[ch],
                test_channels[ch],
                "Channel {} mismatch: decoded {}, expected {}",
                ch + 1,
                decoded[ch],
                test_channels[ch]
            );
        }
    }

    #[test]
    fn test_backward_compatible_14ch_unpack() {
        let mut radio = Afhds2a::new(0x1122_3344);
        radio.rx_id = 0x5566_7788;

        let test_channels: [u16; 18] = [
            1500, 1000, 2000, 1200, 1400, 1600, 1800, 1100, 1300, 1500, 1700, 1900, 1050, 1950,
            2012, 988, 1750, 1250, // Channels 15..18 with diverse bit patterns
        ];
        radio.set_channels(&test_channels);

        let mut packet = [0u8; TX_PACKET_SIZE];
        radio.build_stick_packet(&mut packet);

        // A legacy 14-channel decoder reads 16-bit little-endian and masks 12 bits (& 0x0FFF)
        let slot_bytes = &packet[9..37];
        for ch in 0..14 {
            let offset = ch * 2;
            let raw_16 = (slot_bytes[offset] as u16) | ((slot_bytes[offset + 1] as u16) << 8);
            let legacy_12 = raw_16 & 0x0FFF;
            assert_eq!(
                legacy_12,
                test_channels[ch],
                "Legacy 14-ch decoder on CH{} failed: got {}, expected {}",
                ch + 1,
                legacy_12,
                test_channels[ch]
            );
        }
    }

    #[test]
    fn test_build_failsafe_packet() {
        let radio = Afhds2a::new(0xAABB_CCDD);
        let mut packet = [0u8; TX_PACKET_SIZE];
        radio.build_failsafe_packet(&mut packet);

        assert_eq!(packet[0], PACKET_FAILSAFE); // 0x56
        assert_eq!(&packet[1..5], &0xAABB_CCDDu32.to_le_bytes());

        // CH3 (throttle) failsafe cutoff: FAILSAFE_THROTTLE_US = 988
        let ch3_fs = (packet[9 + 2 * 2] as u16) | ((packet[10 + 2 * 2] as u16) << 8);
        assert_eq!(ch3_fs, FAILSAFE_THROTTLE_US);

        // Other channels 0, 1, 3..13: hold last (0xFFFF)
        for ch in 0..14 {
            if ch != 2 {
                let val = (packet[9 + ch * 2] as u16) | ((packet[10 + ch * 2] as u16) << 8);
                assert_eq!(
                    val,
                    0xFFFF,
                    "Channel {} should be hold-last (0xFFFF)",
                    ch + 1
                );
            }
        }

        assert_eq!(packet[37], 0x00);
    }
}
