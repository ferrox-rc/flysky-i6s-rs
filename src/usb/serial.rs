//! USB CDC-ACM Serial communications and telemetry streaming driver.
//!
//! Provides virtual COM port communication with standard JSON-lines telemetry streaming,
//! model inspection, and interactive serial CLI.

use crate::rf::afhds2a::TelemetryData;
use usbd_serial::SerialPort;

/// Helper to write as many bytes as possible without blocking.
pub fn write_all<
    B: usb_device::bus::UsbBus,
    RS: core::borrow::BorrowMut<[u8]>,
    WS: core::borrow::BorrowMut<[u8]>,
>(
    serial: &mut SerialPort<B, RS, WS>,
    data: &[u8],
) {
    let mut written = 0;
    while written < data.len() {
        match serial.write(&data[written..]) {
            Ok(n) if n > 0 => written += n,
            _ => break,
        }
    }
}

pub struct SerialHandler {
    rx_queue: [u8; 128],
    rx_head: usize,
    rx_tail: usize,
    cmd_buf: [u8; 64],
    cmd_len: usize,
    last_was_cr: bool,
    stream_enabled: bool,
}

impl SerialHandler {
    pub const fn new() -> Self {
        Self {
            rx_queue: [0u8; 128],
            rx_head: 0,
            rx_tail: 0,
            cmd_buf: [0u8; 64],
            cmd_len: 0,
            last_was_cr: false,
            stream_enabled: false, // Default OFF: clean interactive CLI on connect
        }
    }

    pub fn is_streaming(&self) -> bool {
        self.stream_enabled
    }

    /// Drain incoming data from the USB serial OUT endpoint into the internal FIFO queue.
    /// MUST be called from the USB interrupt handler (or when polling) so that
    /// the STM32 hardware `CTR_RX` flag on the CDC OUT endpoint is cleared.
    pub fn drain_rx<
        B: usb_device::bus::UsbBus,
        RS: core::borrow::BorrowMut<[u8]>,
        WS: core::borrow::BorrowMut<[u8]>,
    >(
        &mut self,
        serial: &mut SerialPort<B, RS, WS>,
    ) {
        let mut buf = [0u8; 64];
        while let Ok(count) = serial.read(&mut buf) {
            if count == 0 {
                break;
            }
            for &b in &buf[..count] {
                let next = (self.rx_head + 1) % self.rx_queue.len();
                if next != self.rx_tail {
                    self.rx_queue[self.rx_head] = b;
                    self.rx_head = next;
                }
            }
        }
    }

    /// Process queued incoming data from the main loop and write pending telemetry frames.
    pub fn update<
        B: usb_device::bus::UsbBus,
        RS: core::borrow::BorrowMut<[u8]>,
        WS: core::borrow::BorrowMut<[u8]>,
    >(
        &mut self,
        serial: &mut SerialPort<B, RS, WS>,
        rf_chs: &[u16; crate::mixer::NUM_CHANNELS],
        telem: &TelemetryData,
        battery_mv: u16,
    ) {
        // Drain any pending data from the USB hardware endpoint
        self.drain_rx(serial);

        while self.rx_tail != self.rx_head {
            let b = self.rx_queue[self.rx_tail];
            self.rx_tail = (self.rx_tail + 1) % self.rx_queue.len();

            // While actively streaming telemetry, any key press pauses streaming and returns to CLI prompt
            if self.stream_enabled {
                self.stream_enabled = false;
                self.cmd_len = 0;
                self.last_was_cr = false;
                write_all(serial, b"\r\n[Stream paused]\r\ni6x> ");
                continue;
            }

            if b == b'\r' || b == b'\n' {
                let is_crlf_second = b == b'\n' && self.last_was_cr;
                self.last_was_cr = b == b'\r';
                if !is_crlf_second {
                    write_all(serial, b"\r\n");
                    if self.cmd_len > 0 {
                        self.handle_command(serial, rf_chs, telem, battery_mv);
                        self.cmd_len = 0;
                    }
                    if !self.stream_enabled {
                        write_all(serial, b"i6x> ");
                    }
                }
            } else if b == 0x08 || b == 0x7F {
                self.last_was_cr = false;
                // Backspace
                if self.cmd_len > 0 {
                    self.cmd_len -= 1;
                    write_all(serial, b"\x08 \x08");
                }
            } else if self.cmd_len < self.cmd_buf.len() {
                self.last_was_cr = false;
                self.cmd_buf[self.cmd_len] = b;
                self.cmd_len += 1;
                write_all(serial, &[b]); // Local echo
            }
        }
    }

    /// Dispatch interactive CLI command responses.
    fn handle_command<
        B: usb_device::bus::UsbBus,
        RS: core::borrow::BorrowMut<[u8]>,
        WS: core::borrow::BorrowMut<[u8]>,
    >(
        &mut self,
        serial: &mut SerialPort<B, RS, WS>,
        rf_chs: &[u16; crate::mixer::NUM_CHANNELS],
        telem: &TelemetryData,
        battery_mv: u16,
    ) {
        let cmd = core::str::from_utf8(&self.cmd_buf[..self.cmd_len])
            .unwrap_or("")
            .trim();

        if cmd.eq_ignore_ascii_case("help") {
            write_all(
                serial,
                b"Commands:\r\n  help      - Show this help\r\n  status    - Firmware & battery info\r\n  channels  - Dump RF channel values (CH1..CH18)\r\n  telem     - Dump single telemetry frame\r\n  stream    - Start continuous telemetry streaming (any key to stop)\r\n  reboot    - Reboot transmitter\r\n",
            );
        } else if cmd.eq_ignore_ascii_case("status") {
            write_all(
                serial,
                concat!(
                    "FlySky FS-i6X Rust Firmware v",
                    env!("CARGO_PKG_VERSION"),
                    "\r\n"
                )
                .as_bytes(),
            );
            if crate::crsf::is_enabled() {
                write_all(serial, b"Protocol: CRSF / ExpressLRS (PD5 UART active)\r\n");
            } else {
                write_all(serial, b"Protocol: AFHDS 2A (A7105 SPI active)\r\n");
            }
            self.send_telemetry_line(serial, rf_chs, telem, battery_mv);
        } else if cmd.eq_ignore_ascii_case("channels") {
            let mut cbuf = [0u8; 128];
            let mut pos = 0;
            macro_rules! append_ch {
                ($bytes:expr) => {
                    let s = $bytes;
                    if pos + s.len() <= cbuf.len() {
                        cbuf[pos..pos + s.len()].copy_from_slice(s);
                        pos += s.len();
                    }
                };
            }
            append_ch!(b"{\"ch\":[");
            for (i, &ch) in rf_chs.iter().enumerate() {
                if i > 0 {
                    append_ch!(b",");
                }
                let mut num = [b'0'; 4];
                num[0] = b'0' + ((ch / 1000) % 10) as u8;
                num[1] = b'0' + ((ch / 100) % 10) as u8;
                num[2] = b'0' + ((ch / 10) % 10) as u8;
                num[3] = b'0' + (ch % 10) as u8;
                append_ch!(&num);
            }
            append_ch!(b"]}\r\n");
            write_all(serial, &cbuf[..pos]);
        } else if cmd.eq_ignore_ascii_case("telem") {
            self.send_telemetry_line(serial, rf_chs, telem, battery_mv);
        } else if cmd.eq_ignore_ascii_case("stream")
            || cmd.eq_ignore_ascii_case("telem on")
            || cmd.eq_ignore_ascii_case("telem stream")
        {
            self.stream_enabled = true;
            write_all(
                serial,
                b"[Streaming telemetry @ 10Hz. Press any key to stop.]\r\n",
            );
        } else if cmd.eq_ignore_ascii_case("telem off") {
            self.stream_enabled = false;
            write_all(serial, b"Telemetry streaming disabled\r\n");
        } else if cmd.eq_ignore_ascii_case("reboot") {
            write_all(serial, b"Rebooting...\r\n");
            cortex_m::peripheral::SCB::sys_reset();
        } else if !cmd.is_empty() {
            write_all(serial, b"Unknown command. Type 'help'\r\n");
        }
    }

    /// Stream a single formatted JSON telemetry line over USB CDC.
    /// Format: {"vbat":5.18,"rssi":98,"rx_v":5.02,"tx":15820,"rx":15798,"err":22,"crsf":{...},"ch":[1500,...]}\r\n
    pub fn send_telemetry_line<
        B: usb_device::bus::UsbBus,
        RS: core::borrow::BorrowMut<[u8]>,
        WS: core::borrow::BorrowMut<[u8]>,
    >(
        &self,
        serial: &mut SerialPort<B, RS, WS>,
        rf_chs: &[u16; crate::mixer::NUM_CHANNELS],
        telem: &TelemetryData,
        battery_mv: u16,
    ) {
        let mut buf = [0u8; 320];
        let mut pos = 0;

        append_bytes(&mut buf, &mut pos, b"{\"vbat\":");
        append_volt(&mut buf, &mut pos, battery_mv);
        append_bytes(&mut buf, &mut pos, b",\"rssi\":");
        append_u32(&mut buf, &mut pos, telem.rssi as u32);
        append_bytes(&mut buf, &mut pos, b",\"rx_v\":");
        append_volt(&mut buf, &mut pos, telem.rx_voltage_mv);
        append_bytes(&mut buf, &mut pos, b",\"tx\":");
        append_u32(&mut buf, &mut pos, telem.packets_sent);
        append_bytes(&mut buf, &mut pos, b",\"rx\":");
        append_u32(&mut buf, &mut pos, telem.packets_received);
        append_bytes(&mut buf, &mut pos, b",\"err\":");
        append_u32(
            &mut buf,
            &mut pos,
            telem.packets_sent.saturating_sub(telem.packets_received),
        );
        if crate::crsf::is_enabled() {
            let crsf = crate::crsf::get_telemetry();
            append_bytes(&mut buf, &mut pos, b",\"crsf\":{\"conn\":");
            if crsf.connected {
                append_bytes(&mut buf, &mut pos, b"true");
            } else {
                append_bytes(&mut buf, &mut pos, b"false");
            }
            append_bytes(&mut buf, &mut pos, b",\"lq\":");
            append_u32(&mut buf, &mut pos, crsf.uplink_link_quality as u32);
            append_bytes(&mut buf, &mut pos, b",\"rssi_dbm\":");
            append_i32(&mut buf, &mut pos, crsf.uplink_rssi_1 as i32);
            append_bytes(&mut buf, &mut pos, b",\"snr_db\":");
            append_i32(&mut buf, &mut pos, crsf.uplink_snr as i32);
            append_bytes(&mut buf, &mut pos, b",\"ant\":");
            append_u32(&mut buf, &mut pos, crsf.active_antenna as u32);
            append_bytes(&mut buf, &mut pos, b",\"pwr_mw\":");
            append_u32(&mut buf, &mut pos, crsf.tx_power_mw as u32);
            append_bytes(&mut buf, &mut pos, b",\"rf_rate\":\"");
            append_bytes(
                &mut buf,
                &mut pos,
                crate::crsf::protocol::rf_mode_to_str(crsf.rf_mode).as_bytes(),
            );
            append_bytes(&mut buf, &mut pos, b"\",\"rx_vbat_mv\":");
            append_u32(&mut buf, &mut pos, crsf.rx_battery_mv as u32);
            append_bytes(&mut buf, &mut pos, b",\"rx_cap_mah\":");
            append_u32(&mut buf, &mut pos, crsf.rx_capacity_mah);
            append_bytes(&mut buf, &mut pos, b"}");
        }
        append_bytes(&mut buf, &mut pos, b",\"ch\":[");
        for (i, &ch) in rf_chs.iter().enumerate() {
            if i > 0 {
                append_bytes(&mut buf, &mut pos, b",");
            }
            append_u32(&mut buf, &mut pos, ch as u32);
        }
        append_bytes(&mut buf, &mut pos, b"]}\r\n");

        write_all(serial, &buf[..pos]);
    }
}

#[inline]
fn append_bytes(buf: &mut [u8], pos: &mut usize, slice: &[u8]) {
    if *pos + slice.len() <= buf.len() {
        buf[*pos..*pos + slice.len()].copy_from_slice(slice);
        *pos += slice.len();
    }
}

fn append_u32(buf: &mut [u8], pos: &mut usize, mut n: u32) {
    if n == 0 {
        append_bytes(buf, pos, b"0");
    } else {
        let mut num_buf = [0u8; 10];
        let mut idx = 10;
        while n > 0 {
            idx -= 1;
            num_buf[idx] = b'0' + (n % 10) as u8;
            n /= 10;
        }
        append_bytes(buf, pos, &num_buf[idx..]);
    }
}

fn append_i32(buf: &mut [u8], pos: &mut usize, v: i32) {
    if v < 0 {
        append_bytes(buf, pos, b"-");
        append_u32(buf, pos, (-v) as u32);
    } else {
        append_u32(buf, pos, v as u32);
    }
}

fn append_volt(buf: &mut [u8], pos: &mut usize, mv: u16) {
    let v = (mv / 1000) as u32;
    let rem = (mv % 1000) as u32;
    let d1 = (rem / 100) as u8;
    let d2 = ((rem % 100) / 10) as u8;
    if v >= 10 {
        let v_buf = [
            b'0' + ((v / 10) % 10) as u8,
            b'0' + (v % 10) as u8,
            b'.',
            b'0' + d1,
            b'0' + d2,
        ];
        append_bytes(buf, pos, &v_buf);
    } else {
        let v_buf = [b'0' + (v as u8), b'.', b'0' + d1, b'0' + d2];
        append_bytes(buf, pos, &v_buf);
    }
}
