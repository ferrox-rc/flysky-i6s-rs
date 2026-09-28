//! CRSF / ExpressLRS protocol subsystem for FlySky FS-i6X.
//!
//! Handles USART2 hardware setup on PD5/PA15, PC13 module power management,
//! periodic channel packet framing, and incoming telemetry processing.

#![allow(dead_code)]
#![allow(static_mut_refs)]

pub mod protocol;
pub mod uart;

use protocol::{build_channels_frame, parse_telemetry_frame, CrsfTelemetry, CRSF_RC_FRAME_SIZE};

static mut TELEMETRY: CrsfTelemetry = CrsfTelemetry::new();
static mut CRSF_ENABLED: bool = false;
static mut CURRENT_BAUD: u8 = 0xFF;
static mut LAST_TX_MS: u32 = 0;
static mut RX_BUF: [u8; 64] = [0; 64];
static mut RX_LEN: usize = 0;
static mut LAST_RX_BYTE_MS: u32 = 0;

/// Initialize CRSF subsystem hardware pins with configured PC13 power switch polarity.
pub fn init(active_high: bool) {
    uart::init(active_high);
}

/// Set external module power switch polarity (PC13).
pub fn set_power_polarity(active_high: bool) {
    uart::set_power_polarity(active_high);
}

/// Enable or disable CRSF protocol, power external module, and configure baud rate.
pub fn set_enabled(enabled: bool, baud_idx: u8) {
    unsafe {
        if enabled != CRSF_ENABLED || (enabled && baud_idx != CURRENT_BAUD) {
            CRSF_ENABLED = enabled;
            CURRENT_BAUD = baud_idx;

            uart::set_uart_enabled(enabled, baud_idx);
            uart::set_module_power(enabled);

            if !enabled {
                TELEMETRY = CrsfTelemetry::new();
                RX_LEN = 0;
                LAST_RX_BYTE_MS = 0;
            }
        }
    }
}

/// Returns true if CRSF mode is currently active.
pub fn is_enabled() -> bool {
    unsafe { CRSF_ENABLED }
}

/// Transmit RC channels packet to external module if interval elapsed (~100 Hz / 10 ms).
pub fn update_channels(now_ms: u32, channels: &[u16; 14]) {
    unsafe {
        if !CRSF_ENABLED {
            return;
        }

        // Pacing: 10 ms (100 Hz update rate)
        if now_ms.wrapping_sub(LAST_TX_MS) >= 10 {
            LAST_TX_MS = now_ms;
            let mut frame = [0u8; CRSF_RC_FRAME_SIZE];
            build_channels_frame(channels, &mut frame);
            uart::write_bytes(&frame);
        }
    }
}

pub const MAX_PARAMS: usize = 24;

static mut CHUNK_BUF: [u8; 320] = [0; 320];
static mut CHUNK_LEN: usize = 0;
static mut CHUNK_PARAM_ID: u8 = 0;

#[inline]
unsafe fn send_ping() {
    let mut buf = [0u8; 8];
    let len = protocol::build_ping_frame(&mut buf);
    uart::write_bytes(&buf[..len]);
}

#[inline]
unsafe fn send_param_read(target: u8, param_id: u8, chunk: u8) {
    let mut buf = [0u8; 8];
    let len = protocol::build_param_read_frame(target, param_id, chunk, &mut buf);
    uart::write_bytes(&buf[..len]);
}

#[inline]
unsafe fn send_param_write(target: u8, param_id: u8, val: u8) {
    let mut buf = [0u8; 8];
    let len = protocol::build_param_write_frame(target, param_id, val, &mut buf);
    uart::write_bytes(&buf[..len]);
}

/// Extract null-terminated string from `data` starting at `offset` into `dest`,
/// returning (offset_after_null, length_copied).
fn extract_null_string(data: &[u8], offset: usize, dest: &mut [u8]) -> (usize, u8) {
    let mut end = offset;
    while end < data.len() && data[end] != 0 {
        end += 1;
    }
    let full_len = end.saturating_sub(offset);
    let copy_len = full_len.min(dest.len());
    dest[..copy_len].copy_from_slice(&data[offset..offset + copy_len]);
    let next_offset = if end < data.len() { end + 1 } else { end };
    (next_offset, copy_len as u8)
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ActiveCommandState {
    Idle,
    Starting {
        param_id: u8,
        timeout_ms: u32,
    },
    WaitingConfirm {
        param_id: u8,
    },
    Running {
        param_id: u8,
        poll_timer_ms: u32,
        timeout_ms: u32,
    },
}

#[derive(Copy, Clone, Debug)]
pub struct Parameter {
    pub id: u8,
    pub parent: u8,
    pub param_type: u8,
    pub name: [u8; 16],
    pub name_len: u8,
    pub value: u8,
    pub max_value: u8,
    pub options: [u8; 160],
    pub options_len: u8,
    pub status: u8,
}

impl Parameter {
    pub const fn empty() -> Self {
        Self {
            id: 0,
            parent: 0,
            param_type: 0,
            name: [0; 16],
            name_len: 0,
            value: 0,
            max_value: 0,
            options: [0; 160],
            options_len: 0,
            status: 0,
        }
    }

    /// Extract option string for any `val` index into buffer.
    pub fn option_str_for_val<'a>(&'a self, val: u8, buf: &'a mut [u8; 16]) -> &'a str {
        if self.options_len == 0 {
            return "";
        }
        let opts = &self.options[..self.options_len as usize];
        let mut idx = 0u8;
        let mut start = 0usize;

        for (i, &b) in opts.iter().enumerate() {
            if b == b';' || b == 0 {
                if idx == val {
                    let chunk = &opts[start..i];
                    let copy_len = chunk.len().min(15);
                    buf[..copy_len].copy_from_slice(&chunk[..copy_len]);
                    return core::str::from_utf8(&buf[..copy_len]).unwrap_or("");
                }
                idx += 1;
                start = i + 1;
            }
        }
        if idx == val && start < opts.len() {
            let chunk = &opts[start..];
            let copy_len = chunk.len().min(15);
            buf[..copy_len].copy_from_slice(&chunk[..copy_len]);
            return core::str::from_utf8(&buf[..copy_len]).unwrap_or("");
        }
        ""
    }

    /// Extract option string for current `value` index into buffer.
    pub fn current_option_str<'a>(&'a self, buf: &'a mut [u8; 16]) -> &'a str {
        self.option_str_for_val(self.value, buf)
    }
}

pub const MAX_DISCOVERED_DEVICES: usize = 4;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct DiscoveredDevice {
    pub address: u8,
    pub name: [u8; 16],
    pub name_len: u8,
    pub param_count: u8,
}

impl DiscoveredDevice {
    pub const fn empty() -> Self {
        Self {
            address: 0,
            name: [0; 16],
            name_len: 0,
            param_count: 0,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ElrsConfigState {
    Idle,
    Discovering,      // Sending broadcast ping 0x28, collecting devices
    Connected,        // Received device info 0x29
    LoadingParam(u8), // Requesting param 1..param_count for chosen device
    Ready,            // All parameters cached and interactive
}

pub struct ElrsConfigEngine {
    pub state: ElrsConfigState,
    pub active_cmd: ActiveCommandState,
    pub device_id: u8,
    pub device_name: [u8; 20],
    pub device_name_len: u8,
    pub param_count: u8,
    pub devices: [DiscoveredDevice; MAX_DISCOVERED_DEVICES],
    pub devices_len: usize,
    pub selected_device_idx: usize,
    pub params: [Parameter; MAX_PARAMS],
    pub params_len: usize,
    pub last_req_ms: u32,
    pub next_req_ms: u32,
    pub current_chunk: u8,
    pub expect_chunks_remain: u8,
    pub retry_count: u8,
}

impl ElrsConfigEngine {
    pub const fn new() -> Self {
        Self {
            state: ElrsConfigState::Idle,
            active_cmd: ActiveCommandState::Idle,
            device_id: 0, // Zeroed by default to sit in .bss
            device_name: [0; 20],
            device_name_len: 0,
            param_count: 0,
            devices: [DiscoveredDevice::empty(); MAX_DISCOVERED_DEVICES],
            devices_len: 0,
            selected_device_idx: 0,
            params: [Parameter::empty(); MAX_PARAMS],
            params_len: 0,
            last_req_ms: 0,
            next_req_ms: 0,
            current_chunk: 0,
            expect_chunks_remain: 0xFF,
            retry_count: 0,
        }
    }
}

static mut CONFIG_ENGINE: ElrsConfigEngine = ElrsConfigEngine::new();

/// Poll USART2 for incoming telemetry bytes from external module.
pub fn poll_telemetry(now_ms: u32) {
    unsafe {
        if !CRSF_ENABLED {
            return;
        }

        // Inter-byte silence timeout: if bytes stalled mid-frame for >=3ms, reset parser to recover
        if RX_LEN > 0 && now_ms.wrapping_sub(LAST_RX_BYTE_MS) >= 3 {
            RX_LEN = 0;
        }

        // Drain available bytes from USART2 RX
        let mut count = 0;
        while let Some(b) = uart::read_byte() {
            LAST_RX_BYTE_MS = now_ms;
            if RX_LEN == 0 {
                // Look for frame start: CRSF Sync Byte (0xC8) or valid destination addresses (0xEE, 0xEA, 0xEC)
                // Note: CRSF_SYNC_BYTE and CRSF_ADDRESS_FLIGHT_CONTROLLER both equal 0xC8.
                // At byte index 0 of an incoming frame, 0xC8 is the wire sync byte delimiter.
                // Within extended frames (0x28, 0x29, 0x2B, 0x2C, 0x2D), device addresses (such as
                // CRSF_ADDRESS_FLIGHT_CONTROLLER 0xC8) are indexed at frame[3] (dest) and frame[4] (orig).
                if b == protocol::CRSF_SYNC_BYTE
                    || b == protocol::CRSF_ADDRESS_RADIO_TRANSMITTER
                    || b == protocol::CRSF_ADDRESS_CRSF_TRANSMITTER
                    || b == protocol::CRSF_ADDRESS_CRSF_RECEIVER
                {
                    RX_BUF[0] = b;
                    RX_LEN = 1;
                }
            } else if RX_LEN == 1 {
                // Length byte: frame size is length + 2 (addr + len)
                let frame_len = b as usize;
                if frame_len >= 2 && (frame_len + 2) <= RX_BUF.len() {
                    RX_BUF[1] = b;
                    RX_LEN = 2;
                } else {
                    RX_LEN = 0; // Invalid length, reset parser
                }
            } else {
                let total_expected = (RX_BUF[1] as usize) + 2;
                if RX_LEN < total_expected {
                    RX_BUF[RX_LEN] = b;
                    RX_LEN += 1;

                    if RX_LEN == total_expected {
                        // Full frame received! Verify CRC8
                        let frame = &RX_BUF[..RX_LEN];
                        let expected_crc = frame[RX_LEN - 1];
                        let calc = protocol::crc8(&frame[2..RX_LEN - 1]);
                        if expected_crc == calc {
                            let frame_type = frame[2];
                            match frame_type {
                                protocol::CRSF_FRAMETYPE_LINK_STATISTICS
                                | protocol::CRSF_FRAMETYPE_BATTERY_SENSOR => {
                                    let _ = parse_telemetry_frame(frame, &mut TELEMETRY, now_ms);
                                }
                                protocol::CRSF_FRAMETYPE_ELRS_STATUS => {
                                    TELEMETRY.connected = true;
                                    TELEMETRY.last_telemetry_ms = now_ms;
                                }
                                protocol::CRSF_FRAMETYPE_DEVICE_INFO => {
                                    handle_device_info_frame(&frame[3..RX_LEN - 1], now_ms);
                                }
                                protocol::CRSF_FRAMETYPE_PARAMETER_SETTINGS_ENTRY => {
                                    handle_param_entry_frame(&frame[3..RX_LEN - 1], now_ms);
                                }
                                _ => {}
                            }
                        }
                        RX_LEN = 0;
                    }
                } else {
                    RX_LEN = 0;
                }
            }

            count += 1;
            if count >= 128 {
                break;
            }
        }

        // Connection timeout: if no telemetry received for 1000ms, mark disconnected
        if TELEMETRY.connected && now_ms.wrapping_sub(TELEMETRY.last_telemetry_ms) > 1000 {
            TELEMETRY.connected = false;
        }

        // Service parameter configurator background retries
        elrs_tick(now_ms);
    }
}

unsafe fn handle_device_info_frame(payload: &[u8], _now_ms: u32) {
    if payload.len() < 3 {
        return;
    }
    let orig = payload[1];

    let mut name_buf = [0u8; 16];
    let (next_offset, name_len) = extract_null_string(payload, 2, &mut name_buf);

    // Skip past serial (4B), hw (4B), fw (4B) to read param_count
    let param_count_offset = next_offset + 4 + 4 + 4;
    let param_count = if param_count_offset < payload.len() {
        payload[param_count_offset]
    } else {
        10
    };

    // If in Discovering state, collect responding devices into device list
    if CONFIG_ENGINE.state == ElrsConfigState::Discovering {
        let mut found = false;
        for d in &mut CONFIG_ENGINE.devices[..CONFIG_ENGINE.devices_len] {
            if d.address == orig {
                d.name = name_buf;
                d.name_len = name_len;
                d.param_count = param_count;
                found = true;
                break;
            }
        }
        if !found && CONFIG_ENGINE.devices_len < MAX_DISCOVERED_DEVICES {
            CONFIG_ENGINE.devices[CONFIG_ENGINE.devices_len] = DiscoveredDevice {
                address: orig,
                name: name_buf,
                name_len,
                param_count,
            };
            CONFIG_ENGINE.devices_len += 1;
        }
        return;
    }

    // Only accept device info updates from the active device being configured.
    // Discard broadcasts from other devices while loading/ready.
    if orig != CONFIG_ENGINE.device_id {
        return;
    }

    let dev_name_len = name_len.min(20);
    CONFIG_ENGINE.device_name[..dev_name_len as usize]
        .copy_from_slice(&name_buf[..dev_name_len as usize]);
    CONFIG_ENGINE.device_name_len = dev_name_len;
    CONFIG_ENGINE.param_count = param_count;
}

unsafe fn parse_and_store_parameter(chunk: &[u8], param_id: u8, now_ms: u32) {
    if chunk.len() < 3 {
        return;
    }
    let parent = chunk[0];
    let p_type = chunk[1] & 0x7F;

    let mut name_buf = [0u8; 16];
    let (rest_start, name_len) = extract_null_string(chunk, 2, &mut name_buf);

    let mut opt_buf = [0u8; 160];
    let mut opt_len = 0u8;
    let mut val = 0u8;
    let mut max_val = 0u8;
    let mut status = 0u8;

    if rest_start < chunk.len() {
        if p_type == protocol::CRSF_TYPE_SELECT {
            let mut opt_end = rest_start;
            let mut opt_count = 1u8;
            while opt_end < chunk.len() && chunk[opt_end] != 0 {
                if chunk[opt_end] == b';' {
                    opt_count += 1;
                }
                opt_end += 1;
            }
            let copy_len = (opt_end - rest_start).min(160);
            opt_buf[..copy_len].copy_from_slice(&chunk[rest_start..rest_start + copy_len]);
            opt_len = copy_len as u8;
            max_val = opt_count.saturating_sub(1);

            let val_pos = opt_end + 1;
            if val_pos < chunk.len() {
                val = chunk[val_pos];
            }
        } else if p_type == protocol::CRSF_TYPE_COMMAND {
            status = chunk[rest_start];
            val = status;

            // Drive active command state transitions based on module response
            match CONFIG_ENGINE.active_cmd {
                ActiveCommandState::Starting {
                    param_id: cmd_id, ..
                }
                | ActiveCommandState::Running {
                    param_id: cmd_id, ..
                } if cmd_id == param_id => match status {
                    protocol::STATUS_CONFIRMATION_NEEDED => {
                        CONFIG_ENGINE.active_cmd =
                            ActiveCommandState::WaitingConfirm { param_id };
                    }
                    protocol::STATUS_PROGRESS => {
                        CONFIG_ENGINE.active_cmd = ActiveCommandState::Running {
                            param_id,
                            poll_timer_ms: now_ms.wrapping_add(250),
                            timeout_ms: now_ms.wrapping_add(8000),
                        };
                    }
                    protocol::STATUS_READY => {
                        CONFIG_ENGINE.active_cmd = ActiveCommandState::Idle;
                    }
                    _ => {}
                },
                _ => {}
            }
        }
    }

    let mut found = false;
    for p in &mut CONFIG_ENGINE.params[..CONFIG_ENGINE.params_len] {
        if p.id == param_id {
            p.value = val;
            p.status = status;
            p.options = opt_buf;
            p.options_len = opt_len;
            p.max_value = max_val;
            found = true;
            break;
        }
    }
    if !found && CONFIG_ENGINE.params_len < MAX_PARAMS {
        CONFIG_ENGINE.params[CONFIG_ENGINE.params_len] = Parameter {
            id: param_id,
            parent,
            param_type: p_type,
            name: name_buf,
            name_len,
            value: val,
            max_value: max_val,
            options: opt_buf,
            options_len: opt_len,
            status,
        };
        CONFIG_ENGINE.params_len += 1;
    }
}

unsafe fn handle_param_entry_frame(payload: &[u8], now_ms: u32) {
    if payload.len() < 5 {
        return;
    }
    let orig = payload[1];
    if orig != CONFIG_ENGINE.device_id {
        return;
    }

    let param_id = payload[2];
    let chunks_remain = payload[3];
    let chunk_slice = &payload[4..];

    // If we're loading parameters, verify this frame matches the parameter being loaded
    if let ElrsConfigState::LoadingParam(loading_id) = CONFIG_ENGINE.state {
        if param_id != loading_id {
            // Stale or unsolicited param frame while loading another param, discard
            return;
        }
    }

    // Sequence check: if expecting specific chunks_remain, discard duplicates / out-of-order
    if CONFIG_ENGINE.expect_chunks_remain != 0xFF
        && chunks_remain != CONFIG_ENGINE.expect_chunks_remain
    {
        return;
    }

    // If starting a new parameter or chunk index 0, reset accumulator
    if CONFIG_ENGINE.current_chunk == 0 || CHUNK_PARAM_ID != param_id {
        CHUNK_LEN = 0;
        CHUNK_PARAM_ID = param_id;
    }

    // Append chunk payload to accumulator
    let avail = CHUNK_BUF.len().saturating_sub(CHUNK_LEN);
    let to_copy = chunk_slice.len().min(avail);
    CHUNK_BUF[CHUNK_LEN..CHUNK_LEN + to_copy].copy_from_slice(&chunk_slice[..to_copy]);
    CHUNK_LEN += to_copy;

    // Multi-frame parameter: schedule request for next chunk after 40ms pacing delay
    if chunks_remain > 0 {
        CONFIG_ENGINE.current_chunk += 1;
        CONFIG_ENGINE.expect_chunks_remain = chunks_remain - 1;
        CONFIG_ENGINE.retry_count = 0;
        CONFIG_ENGINE.next_req_ms = now_ms.wrapping_add(40);
        return;
    }

    // Parameter stream complete! Parse reassembled payload
    let chunk = &CHUNK_BUF[..CHUNK_LEN];
    CONFIG_ENGINE.current_chunk = 0;
    CONFIG_ENGINE.expect_chunks_remain = 0xFF;
    CONFIG_ENGINE.retry_count = 0;

    parse_and_store_parameter(chunk, param_id, now_ms);

    // Only advance loading sequence if engine was actively in initial loading phase
    if let ElrsConfigState::LoadingParam(loading_id) = CONFIG_ENGINE.state {
        if param_id == loading_id {
            if param_id < CONFIG_ENGINE.param_count && CONFIG_ENGINE.params_len < MAX_PARAMS {
                let next_id = param_id + 1;
                CONFIG_ENGINE.state = ElrsConfigState::LoadingParam(next_id);
                CONFIG_ENGINE.current_chunk = 0;
                CONFIG_ENGINE.expect_chunks_remain = 0xFF;
                CONFIG_ENGINE.retry_count = 0;
                CHUNK_LEN = 0;
                CHUNK_PARAM_ID = 0;
                CONFIG_ENGINE.next_req_ms = now_ms.wrapping_add(40);
            } else {
                CONFIG_ENGINE.state = ElrsConfigState::Ready;
            }
        }
    }
}

unsafe fn elrs_tick(now_ms: u32) {
    match CONFIG_ENGINE.state {
        ElrsConfigState::Discovering => {
            if now_ms.wrapping_sub(CONFIG_ENGINE.last_req_ms) >= 1000 {
                CONFIG_ENGINE.last_req_ms = now_ms;
                send_ping();
            }
        }
        ElrsConfigState::LoadingParam(id) => {
            if now_ms.wrapping_sub(CONFIG_ENGINE.next_req_ms) < 0x8000_0000 {
                let timeout: u32 = if CONFIG_ENGINE.device_id == protocol::CRSF_ADDRESS_CRSF_RECEIVER {
                    1000
                } else {
                    500
                };

                if CONFIG_ENGINE.retry_count < 4 {
                    CONFIG_ENGINE.retry_count += 1;
                    CONFIG_ENGINE.last_req_ms = now_ms;
                    CONFIG_ENGINE.next_req_ms = now_ms.wrapping_add(timeout);
                    send_param_read(
                        CONFIG_ENGINE.device_id,
                        id,
                        CONFIG_ENGINE.current_chunk,
                    );
                } else {
                    // Retries exhausted for this parameter (packet lost over the air).
                    // Advance to next parameter to avoid freezing UI permanently.
                    CONFIG_ENGINE.retry_count = 0;
                    CONFIG_ENGINE.current_chunk = 0;
                    CONFIG_ENGINE.expect_chunks_remain = 0xFF;
                    CHUNK_LEN = 0;
                    CHUNK_PARAM_ID = 0;
                    if id < CONFIG_ENGINE.param_count && CONFIG_ENGINE.params_len < MAX_PARAMS {
                        CONFIG_ENGINE.state = ElrsConfigState::LoadingParam(id + 1);
                        CONFIG_ENGINE.next_req_ms = now_ms.wrapping_add(40);
                    } else {
                        CONFIG_ENGINE.state = ElrsConfigState::Ready;
                    }
                }
            }
        }
        _ => {}
    }

    // Process active command polling and timeouts
    match CONFIG_ENGINE.active_cmd {
        ActiveCommandState::Starting { timeout_ms, .. } => {
            if now_ms.wrapping_sub(timeout_ms) < 0x8000_0000 {
                CONFIG_ENGINE.active_cmd = ActiveCommandState::Idle;
            }
        }
        ActiveCommandState::Running {
            param_id,
            poll_timer_ms,
            timeout_ms,
        } => {
            if now_ms.wrapping_sub(timeout_ms) < 0x8000_0000 {
                CONFIG_ENGINE.active_cmd = ActiveCommandState::Idle;
            } else if now_ms.wrapping_sub(poll_timer_ms) < 0x8000_0000 {
                send_param_write(CONFIG_ENGINE.device_id, param_id, protocol::STATUS_POLL);
                CONFIG_ENGINE.active_cmd = ActiveCommandState::Running {
                    param_id,
                    poll_timer_ms: now_ms.wrapping_add(250),
                    timeout_ms,
                };
            }
        }
        _ => {}
    }
}

/// Start or refresh the native CRSF multi-device discovery handshake (TBS-Agent style).
pub fn start_config() {
    unsafe {
        let now = crate::time::millis();
        CONFIG_ENGINE.state = ElrsConfigState::Discovering;
        CONFIG_ENGINE.device_id = 0;
        CONFIG_ENGINE.devices_len = 0;
        CONFIG_ENGINE.selected_device_idx = 0;
        CONFIG_ENGINE.params_len = 0;
        CONFIG_ENGINE.last_req_ms = now;
        CONFIG_ENGINE.next_req_ms = now.wrapping_add(1000);
        CONFIG_ENGINE.current_chunk = 0;
        CONFIG_ENGINE.expect_chunks_remain = 0xFF;
        CONFIG_ENGINE.retry_count = 0;
        CHUNK_LEN = 0;
        CHUNK_PARAM_ID = 0;
        CONFIG_ENGINE.active_cmd = ActiveCommandState::Idle;
        send_ping();
    }
}

/// Select a discovered device by index (0..devices_len) to load and configure its parameters.
pub fn select_device(idx: usize) -> bool {
    unsafe {
        if idx >= CONFIG_ENGINE.devices_len {
            return false;
        }
        let dev = CONFIG_ENGINE.devices[idx];
        let now = crate::time::millis();
        CONFIG_ENGINE.selected_device_idx = idx;
        CONFIG_ENGINE.device_id = dev.address;
        CONFIG_ENGINE.device_name = [0; 20];
        let n_len = (dev.name_len as usize).min(20);
        CONFIG_ENGINE.device_name[..n_len].copy_from_slice(&dev.name[..n_len]);
        CONFIG_ENGINE.device_name_len = n_len as u8;
        CONFIG_ENGINE.param_count = dev.param_count;
        CONFIG_ENGINE.params_len = 0;
        CONFIG_ENGINE.current_chunk = 0;
        CONFIG_ENGINE.expect_chunks_remain = 0xFF;
        CONFIG_ENGINE.retry_count = 0;
        CHUNK_LEN = 0;
        CHUNK_PARAM_ID = 0;
        CONFIG_ENGINE.last_req_ms = now;

        if dev.param_count > 0 {
            CONFIG_ENGINE.state = ElrsConfigState::LoadingParam(1);
            CONFIG_ENGINE.next_req_ms = now.wrapping_add(40);
        } else {
            CONFIG_ENGINE.state = ElrsConfigState::Ready;
        }
        true
    }
}

/// Select a discovered device by its physical address (e.g. 0xEE for TX, 0xEC for RX).
pub fn select_device_by_addr(addr: u8) -> bool {
    unsafe {
        for i in 0..CONFIG_ENGINE.devices_len {
            if CONFIG_ENGINE.devices[i].address == addr {
                return select_device(i);
            }
        }
        false
    }
}

/// Return from parameter view back to the Device Selection screen.
pub fn return_to_device_list() {
    unsafe {
        let now = crate::time::millis();
        CONFIG_ENGINE.state = ElrsConfigState::Discovering;
        CONFIG_ENGINE.params_len = 0;
        CONFIG_ENGINE.current_chunk = 0;
        CONFIG_ENGINE.expect_chunks_remain = 0xFF;
        CHUNK_LEN = 0;
        CHUNK_PARAM_ID = 0;
        CONFIG_ENGINE.active_cmd = ActiveCommandState::Idle;
        CONFIG_ENGINE.last_req_ms = now;
        CONFIG_ENGINE.next_req_ms = now.wrapping_add(1000);
        send_ping();
    }
}

/// Set a specific value for a select parameter index and transmit write frame to module.
pub fn set_param_value(param_idx: usize, value: u8) {
    unsafe {
        if param_idx < CONFIG_ENGINE.params_len {
            let p = &mut CONFIG_ENGINE.params[param_idx];
            if p.param_type == protocol::CRSF_TYPE_SELECT {
                p.value = value.min(p.max_value);
                send_param_write(CONFIG_ENGINE.device_id, p.id, p.value);
            }
        }
    }
}

/// Query parameter indices belonging to a given folder ID (0 = root).
/// Returns count of matching parameters.
pub fn get_folder_params(folder_id: u8, out_indices: &mut [usize; MAX_PARAMS]) -> usize {
    unsafe {
        let mut count = 0;
        for (idx, p) in CONFIG_ENGINE.params[..CONFIG_ENGINE.params_len].iter().enumerate() {
            if p.parent == folder_id {
                out_indices[count] = idx;
                count += 1;
            }
        }
        count
    }
}

/// Find parent folder ID of a given folder ID. Returns 0 if root or not found.
pub fn get_parent_folder(folder_id: u8) -> u8 {
    unsafe {
        for p in &CONFIG_ENGINE.params[..CONFIG_ENGINE.params_len] {
            if p.id == folder_id {
                return p.parent;
            }
        }
        0
    }
}

/// Find display name of a folder by ID.
pub fn get_folder_name<'a>(folder_id: u8, buf: &'a mut [u8; 16]) -> &'a str {
    unsafe {
        for p in &CONFIG_ENGINE.params[..CONFIG_ENGINE.params_len] {
            if p.id == folder_id {
                let len = (p.name_len as usize).min(16);
                buf[..len].copy_from_slice(&p.name[..len]);
                return core::str::from_utf8(&buf[..len]).unwrap_or("Folder");
            }
        }
        "Folder"
    }
}

/// Cycle option for a select parameter index and transmit write frame to module.
pub fn cycle_param(param_idx: usize) {
    unsafe {
        if param_idx < CONFIG_ENGINE.params_len {
            let p = &mut CONFIG_ENGINE.params[param_idx];
            if p.param_type == protocol::CRSF_TYPE_SELECT && p.max_value > 0 {
                p.value = (p.value + 1) % (p.max_value + 1);
                send_param_write(CONFIG_ENGINE.device_id, p.id, p.value);
            }
        }
    }
}

/// Trigger an action command on module: starts the command handshake with STATUS_START (1).
pub fn trigger_command(param_idx: usize) {
    unsafe {
        if param_idx < CONFIG_ENGINE.params_len {
            let p = &mut CONFIG_ENGINE.params[param_idx];
            if p.param_type == protocol::CRSF_TYPE_COMMAND
                && CONFIG_ENGINE.active_cmd == ActiveCommandState::Idle
            {
                let now = crate::time::millis();
                send_param_write(CONFIG_ENGINE.device_id, p.id, protocol::STATUS_START);
                CONFIG_ENGINE.active_cmd = ActiveCommandState::Starting {
                    param_id: p.id,
                    timeout_ms: now.wrapping_add(1500),
                };
            }
        }
    }
}

/// Confirm or cancel a command requiring pilot confirmation.
pub fn confirm_command(accept: bool) {
    unsafe {
        if let ActiveCommandState::WaitingConfirm { param_id } = CONFIG_ENGINE.active_cmd {
            let now = crate::time::millis();
            if accept {
                send_param_write(CONFIG_ENGINE.device_id, param_id, protocol::STATUS_CONFIRM);
                CONFIG_ENGINE.active_cmd = ActiveCommandState::Running {
                    param_id,
                    poll_timer_ms: now.wrapping_add(250),
                    timeout_ms: now.wrapping_add(8000),
                };
            } else {
                send_param_write(CONFIG_ENGINE.device_id, param_id, protocol::STATUS_CANCEL);
                CONFIG_ENGINE.active_cmd = ActiveCommandState::Idle;
            }
        }
    }
}

/// Get current configurator engine state and cached parameters.
pub fn get_config_engine() -> &'static ElrsConfigEngine {
    unsafe { &CONFIG_ENGINE }
}

/// Get latest decoded CRSF telemetry data.
pub fn get_telemetry() -> CrsfTelemetry {
    unsafe { TELEMETRY }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crsf::protocol::{
        crc8, CRSF_ADDRESS_CRSF_TRANSMITTER,
        CRSF_ADDRESS_RADIO_TRANSMITTER, CRSF_FRAMETYPE_DEVICE_INFO,
        CRSF_FRAMETYPE_LINK_STATISTICS, CRSF_FRAMETYPE_PARAMETER_SETTINGS_ENTRY,
        CRSF_TYPE_COMMAND, CRSF_TYPE_SELECT, STATUS_CONFIRMATION_NEEDED, STATUS_PROGRESS,
        STATUS_READY, STATUS_START,
    };
    use crate::time::set_millis;

    fn reset_state() {
        unsafe {
            CRSF_ENABLED = true;
            CURRENT_BAUD = 0;
            LAST_TX_MS = 0;
            RX_BUF = [0; 64];
            RX_LEN = 0;
            LAST_RX_BYTE_MS = 0;
            CHUNK_BUF = [0; 320];
            CHUNK_LEN = 0;
            CHUNK_PARAM_ID = 0;
            CONFIG_ENGINE = ElrsConfigEngine::new();
            TELEMETRY = CrsfTelemetry::new();
            uart::mock::clear();
        }
    }

    #[test]
    fn test_framing_garbage_rejection_and_invalid_length() {
        reset_state();

        // 1. Noise bytes before valid address must be discarded
        uart::mock::push_rx_bytes(&[0x00, 0x11, 0x22, 0x33, 0x55, 0xAA]);
        poll_telemetry(100);
        unsafe {
            assert_eq!(RX_LEN, 0, "Noise bytes should not initiate frame capture");
        }

        // 2. Sync byte followed by invalid length (< 2) must reset parser
        uart::mock::push_rx_bytes(&[CRSF_ADDRESS_RADIO_TRANSMITTER, 1, 0x14, 0x00]);
        poll_telemetry(100);
        unsafe {
            assert_eq!(RX_LEN, 0, "Frame length < 2 must be rejected");
        }

        // 3. Sync byte followed by oversized length (> 62) must reset parser
        uart::mock::push_rx_bytes(&[CRSF_ADDRESS_RADIO_TRANSMITTER, 63, 0x14]);
        poll_telemetry(100);
        unsafe {
            assert_eq!(RX_LEN, 0, "Frame length > 62 must be rejected");
        }
    }

    #[test]
    fn test_crc_failure_and_resynchronization() {
        reset_state();

        // Feed Link Statistics frame with corrupt CRC
        let bad_frame = [
            CRSF_ADDRESS_RADIO_TRANSMITTER,
            12,
            CRSF_FRAMETYPE_LINK_STATISTICS,
            80, 85, 99, 10, 0, 2, 3, 0, 0, 0,
            0x00, // Bad CRC
        ];
        uart::mock::push_rx_bytes(&bad_frame);
        poll_telemetry(1000);

        let telem = get_telemetry();
        assert!(!telem.connected, "Corrupt CRC frame must be dropped");

        // Immediately follow with valid Link Statistics frame
        let mut good_frame = bad_frame;
        good_frame[13] = crc8(&good_frame[2..13]);
        uart::mock::push_rx_bytes(&good_frame);
        poll_telemetry(1000);

        let telem_ok = get_telemetry();
        assert!(telem_ok.connected, "Parser must resync and accept subsequent valid frame");
        assert_eq!(telem_ok.uplink_link_quality, 99);
        assert_eq!(telem_ok.tx_power_mw, 100);
    }

    #[test]
    fn test_discovery_handshake_and_device_info() {
        reset_state();
        set_millis(1000);

        // 1. Start config handshake
        start_config();
        let engine = get_config_engine();
        assert_eq!(engine.state, ElrsConfigState::Discovering);

        // Verify Ping packet transmitted
        let tx = uart::mock::take_tx();
        assert_eq!(tx.len(), 1, "start_config must transmit ping packet");
        assert_eq!(tx[0][0], CRSF_ADDRESS_CRSF_TRANSMITTER);
        assert_eq!(tx[0][2], protocol::CRSF_FRAMETYPE_DEVICE_PING);

        // 2. Feed Device Info response frame (0x29)
        // Frame: [addr=0xEA, len, type=0x29, dest=0xEA, orig=0xEE, "ELRS 2.4G\0", serial(4), hw(4), fw(4), count=3, ver=1, crc]
        let mut frame = [0u8; 32];
        frame[0] = CRSF_ADDRESS_RADIO_TRANSMITTER; // 0xEA
        frame[2] = CRSF_FRAMETYPE_DEVICE_INFO;     // 0x29
        frame[3] = CRSF_ADDRESS_RADIO_TRANSMITTER; // 0xEA
        frame[4] = CRSF_ADDRESS_CRSF_TRANSMITTER;  // 0xEE (device_id)

        // Device name: "ELRS 2.4G\0" (10 bytes)
        let name = b"ELRS 2.4G\0";
        frame[5..5 + name.len()].copy_from_slice(name);
        let serial_pos = 5 + name.len();
        // Serial (4), HW (4), FW (4) -> 12 bytes
        let count_pos = serial_pos + 12;
        frame[count_pos] = 3; // 3 parameters to load
        frame[count_pos + 1] = 1; // version

        let total_size = count_pos + 3;
        frame[1] = (total_size - 2) as u8;
        frame[total_size - 1] = crc8(&frame[2..total_size - 1]);

        uart::mock::push_rx_bytes(&frame[..total_size]);
        poll_telemetry(1100);

        let engine = get_config_engine();
        assert_eq!(engine.devices_len, 1);
        assert_eq!(engine.devices[0].address, CRSF_ADDRESS_CRSF_TRANSMITTER);
        assert_eq!(&engine.devices[0].name[..9], b"ELRS 2.4G");
        assert_eq!(engine.devices[0].param_count, 3);

        // Pilot selects device 0 from TBS-Agent device list
        assert!(select_device(0));

        let engine = get_config_engine();
        assert_eq!(engine.device_id, CRSF_ADDRESS_CRSF_TRANSMITTER);
        assert_eq!(&engine.device_name[..9], b"ELRS 2.4G");
        assert_eq!(engine.param_count, 3);
        assert_eq!(engine.state, ElrsConfigState::LoadingParam(1));

        // Pacing guard time: no outbound transmission immediately (< 40ms)
        assert_eq!(uart::mock::take_tx().len(), 0, "No request before 40ms pacing interval");

        // Advance to 1140ms (40ms pacing delay satisfied)
        poll_telemetry(1140);

        // Handset must have transmitted Parameter Read for Param 1, Chunk 0
        let tx = uart::mock::take_tx();
        assert_eq!(tx.len(), 1, "Must request Param 1 Chunk 0 after Device Info");
        assert_eq!(tx[0][2], protocol::CRSF_FRAMETYPE_PARAMETER_READ);
        assert_eq!(tx[0][5], 1, "Requested param_id must be 1");
        assert_eq!(tx[0][6], 0, "Requested chunk must be 0");
    }

    #[test]
    fn test_device_info_zero_params_transitions_to_ready() {
        reset_state();
        start_config();
        uart::mock::clear();

        // Feed Device Info frame with param_count = 0
        let mut frame = [0u8; 24];
        frame[0] = CRSF_ADDRESS_RADIO_TRANSMITTER;
        frame[1] = 22;
        frame[2] = CRSF_FRAMETYPE_DEVICE_INFO;
        frame[3] = CRSF_ADDRESS_RADIO_TRANSMITTER;
        frame[4] = CRSF_ADDRESS_CRSF_TRANSMITTER;
        frame[5..10].copy_from_slice(b"None\0");
        // Count pos: 10 + 12 = 22
        frame[22] = 0; // 0 params
        frame[23] = crc8(&frame[2..23]);

        uart::mock::push_rx_bytes(&frame);
        poll_telemetry(1200);

        let engine = get_config_engine();
        assert_eq!(engine.devices_len, 1);
        assert!(select_device(0));

        let engine = get_config_engine();
        assert_eq!(engine.state, ElrsConfigState::Ready, "0 parameters must transition directly to Ready");
    }

    #[test]
    fn test_param_select_parsing_and_sequential_loading() {
        reset_state();
        // Setup engine expecting Param 1 of 2
        unsafe {
            CONFIG_ENGINE.device_id = CRSF_ADDRESS_CRSF_TRANSMITTER;
            CONFIG_ENGINE.param_count = 2;
            CONFIG_ENGINE.state = ElrsConfigState::LoadingParam(1);
        }

        // Construct Param 1 Entry: SELECT type, Name="Pkt Rate\0", Options="50Hz;150Hz;250Hz;500Hz\0", Value=2
        let mut frame = [0u8; 64];
        frame[0] = CRSF_ADDRESS_RADIO_TRANSMITTER;
        frame[2] = CRSF_FRAMETYPE_PARAMETER_SETTINGS_ENTRY;
        frame[3] = CRSF_ADDRESS_RADIO_TRANSMITTER;
        frame[4] = CRSF_ADDRESS_CRSF_TRANSMITTER;
        frame[5] = 1; // param_id
        frame[6] = 0; // chunks_remain
        frame[7] = 0; // parent
        frame[8] = CRSF_TYPE_SELECT; // 9

        let mut pos = 9;
        let name = b"Pkt Rate\0";
        frame[pos..pos + name.len()].copy_from_slice(name);
        pos += name.len();

        let opts = b"50Hz;150Hz;250Hz;500Hz\0";
        frame[pos..pos + opts.len()].copy_from_slice(opts);
        pos += opts.len();

        frame[pos] = 2; // Value = 2 (250Hz)
        pos += 1;

        let frame_len = pos - 1;
        frame[1] = frame_len as u8;
        frame[pos] = crc8(&frame[2..pos]);
        let total = pos + 1;

        uart::mock::push_rx_bytes(&frame[..total]);
        poll_telemetry(2000);

        let engine = get_config_engine();
        assert_eq!(engine.params_len, 1);
        let p = &engine.params[0];
        assert_eq!(p.id, 1);
        assert_eq!(p.param_type, CRSF_TYPE_SELECT);
        assert_eq!(&p.name[..8], b"Pkt Rate");
        assert_eq!(p.value, 2);
        assert_eq!(p.max_value, 3); // 4 options -> max_value = 3
        let mut buf = [0u8; 16];
        assert_eq!(p.current_option_str(&mut buf), "250Hz");

        // Advanced to loading Param 2
        assert_eq!(engine.state, ElrsConfigState::LoadingParam(2));
        assert_eq!(uart::mock::take_tx().len(), 0, "No request immediately inside RX callback");

        // Advance by 40ms pacing interval
        poll_telemetry(2040);
        let tx = uart::mock::take_tx();
        assert_eq!(tx.len(), 1, "Must request Param 2 Chunk 0");
        assert_eq!(tx[0][5], 2);
    }

    #[test]
    fn test_param_multi_frame_chunk_reassembly() {
        reset_state();
        unsafe {
            CONFIG_ENGINE.device_id = CRSF_ADDRESS_CRSF_TRANSMITTER;
            CONFIG_ENGINE.param_count = 1;
            CONFIG_ENGINE.state = ElrsConfigState::LoadingParam(1);
        }

        // Chunk 0: chunks_remain = 1
        let mut chunk0 = [0u8; 32];
        chunk0[0] = CRSF_ADDRESS_RADIO_TRANSMITTER;
        chunk0[2] = CRSF_FRAMETYPE_PARAMETER_SETTINGS_ENTRY;
        chunk0[3] = CRSF_ADDRESS_RADIO_TRANSMITTER;
        chunk0[4] = CRSF_ADDRESS_CRSF_TRANSMITTER;
        chunk0[5] = 1; // param_id
        chunk0[6] = 1; // chunks_remain = 1!
        chunk0[7] = 0; // parent
        chunk0[8] = CRSF_TYPE_SELECT; // type
        chunk0[9..13].copy_from_slice(b"Pwr\0");
        chunk0[13..23].copy_from_slice(b"10mW;25mW;"); // First half of options
        let total0 = 24;
        chunk0[1] = (total0 - 2) as u8;
        chunk0[total0 - 1] = crc8(&chunk0[2..total0 - 1]);

        uart::mock::push_rx_bytes(&chunk0[..total0]);
        poll_telemetry(3000);

        let engine = get_config_engine();
        assert_eq!(engine.current_chunk, 1, "Must advance to chunk 1");
        assert_eq!(engine.params_len, 0, "Parameter not parsed until final chunk");
        assert_eq!(uart::mock::take_tx().len(), 0, "No request immediately inside RX callback");

        // Advance 40ms to trigger paced request
        poll_telemetry(3040);

        // Verify request for Chunk 1 sent
        let tx = uart::mock::take_tx();
        assert_eq!(tx.len(), 1);
        assert_eq!(tx[0][5], 1); // param_id 1
        assert_eq!(tx[0][6], 1); // chunk 1

        // Chunk 1: chunks_remain = 0 (final chunk)
        let mut chunk1 = [0u8; 32];
        chunk1[0] = CRSF_ADDRESS_RADIO_TRANSMITTER;
        chunk1[2] = CRSF_FRAMETYPE_PARAMETER_SETTINGS_ENTRY;
        chunk1[3] = CRSF_ADDRESS_RADIO_TRANSMITTER;
        chunk1[4] = CRSF_ADDRESS_CRSF_TRANSMITTER;
        chunk1[5] = 1;
        chunk1[6] = 0; // chunks_remain = 0!
        chunk1[7..19].copy_from_slice(b"100mW;250mW\0"); // Second half of options
        chunk1[19] = 3; // value = 3 (250mW)
        let total1 = 21;
        chunk1[1] = (total1 - 2) as u8;
        chunk1[total1 - 1] = crc8(&chunk1[2..total1 - 1]);

        uart::mock::push_rx_bytes(&chunk1[..total1]);
        poll_telemetry(3100);

        let engine = get_config_engine();
        assert_eq!(engine.params_len, 1, "Reassembled parameter must be stored");
        let p = &engine.params[0];
        assert_eq!(&p.name[..3], b"Pwr");
        assert_eq!(p.value, 3);
        assert_eq!(p.max_value, 3); // 4 options total: 10mW, 25mW, 100mW, 250mW
        let mut buf = [0u8; 16];
        assert_eq!(p.current_option_str(&mut buf), "250mW");

        // Since param_count was 1, state must transition to Ready
        assert_eq!(engine.state, ElrsConfigState::Ready);
    }

    #[test]
    fn test_command_action_lifecycle_and_confirmations() {
        reset_state();
        set_millis(5000);

        // Setup ready engine with 1 command param
        unsafe {
            CONFIG_ENGINE.device_id = CRSF_ADDRESS_CRSF_TRANSMITTER;
            CONFIG_ENGINE.state = ElrsConfigState::Ready;
            let mut name = [0u8; 16];
            name[..4].copy_from_slice(b"Bind");
            CONFIG_ENGINE.params[0] = Parameter {
                id: 1,
                parent: 0,
                param_type: CRSF_TYPE_COMMAND,
                name,
                name_len: 4,
                value: STATUS_READY,
                max_value: 0,
                options: [0; 160],
                options_len: 0,
                status: STATUS_READY,
            };
            CONFIG_ENGINE.params_len = 1;
        }

        // 1. Trigger command
        trigger_command(0);
        let engine = get_config_engine();
        assert!(matches!(engine.active_cmd, ActiveCommandState::Starting { param_id: 1, .. }));
        let tx = uart::mock::take_tx();
        assert_eq!(tx.len(), 1);
        assert_eq!(tx[0][2], protocol::CRSF_FRAMETYPE_PARAMETER_WRITE);
        assert_eq!(tx[0][6], STATUS_START);

        // 2. Module responds requiring confirmation (STATUS_CONFIRMATION_NEEDED = 3)
        // Payload has: parent(0), type(CRSF_TYPE_COMMAND), "Bind\0", status
        let mut frame = [0u8; 20];
        frame[0] = CRSF_ADDRESS_RADIO_TRANSMITTER;
        frame[2] = CRSF_FRAMETYPE_PARAMETER_SETTINGS_ENTRY;
        frame[3] = CRSF_ADDRESS_RADIO_TRANSMITTER;
        frame[4] = CRSF_ADDRESS_CRSF_TRANSMITTER;
        frame[5] = 1; // param_id
        frame[6] = 0; // chunks_remain
        frame[7] = 0; // parent
        frame[8] = CRSF_TYPE_COMMAND;
        frame[9..14].copy_from_slice(b"Bind\0");
        frame[14] = STATUS_CONFIRMATION_NEEDED; // status
        let total = 16;
        frame[1] = (total - 2) as u8;
        frame[total - 1] = crc8(&frame[2..total - 1]);

        uart::mock::push_rx_bytes(&frame[..total]);
        poll_telemetry(5100);

        let engine = get_config_engine();
        assert_eq!(engine.active_cmd, ActiveCommandState::WaitingConfirm { param_id: 1 });

        // 3. User accepts confirmation
        confirm_command(true);
        let engine = get_config_engine();
        assert!(matches!(engine.active_cmd, ActiveCommandState::Running { param_id: 1, .. }));
        let tx = uart::mock::take_tx();
        assert_eq!(tx.len(), 1);
        assert_eq!(tx[0][6], protocol::STATUS_CONFIRM);

        // 4. Module responds with STATUS_PROGRESS (2)
        frame[14] = STATUS_PROGRESS;
        frame[total - 1] = crc8(&frame[2..total - 1]);
        uart::mock::push_rx_bytes(&frame[..total]);
        poll_telemetry(5200);

        let engine = get_config_engine();
        assert!(matches!(engine.active_cmd, ActiveCommandState::Running { param_id: 1, .. }));

        // 5. Module completes with STATUS_READY (0)
        frame[14] = STATUS_READY;
        frame[total - 1] = crc8(&frame[2..total - 1]);
        uart::mock::push_rx_bytes(&frame[..total]);
        poll_telemetry(5300);

        let engine = get_config_engine();
        assert_eq!(engine.active_cmd, ActiveCommandState::Idle, "Completed command returns to Idle");
    }

    #[test]
    fn test_telemetry_disconnect_timeout() {
        reset_state();

        // Feed link statistics at t = 1000
        let mut frame = [0u8; 14];
        frame[0] = CRSF_ADDRESS_RADIO_TRANSMITTER;
        frame[1] = 12;
        frame[2] = CRSF_FRAMETYPE_LINK_STATISTICS;
        frame[3] = 80;
        frame[4] = 85;
        frame[5] = 99;
        frame[6] = 10;
        frame[7] = 0;
        frame[8] = 2;
        frame[9] = 3;
        frame[13] = crc8(&frame[2..13]);

        uart::mock::push_rx_bytes(&frame);
        poll_telemetry(1000);
        assert!(get_telemetry().connected);

        // Poll at t = 1500 (500 ms later): still connected
        poll_telemetry(1500);
        assert!(get_telemetry().connected);

        // Poll at t = 2050 (>1000 ms timeout): disconnected!
        poll_telemetry(2050);
        assert!(!get_telemetry().connected, "Must mark disconnected after 1000 ms silence");
    }

    #[test]
    fn test_rx_accepts_receiver_address_0xec() {
        reset_state();
        // Feed partial frame starting with CRSF_ADDRESS_CRSF_RECEIVER (0xEC)
        uart::mock::push_rx_bytes(&[protocol::CRSF_ADDRESS_CRSF_RECEIVER, 12, CRSF_FRAMETYPE_LINK_STATISTICS]);
        poll_telemetry(100);
        unsafe {
            assert_eq!(RX_LEN, 3, "Start byte 0xEC must be accepted into RX_BUF");
            assert_eq!(RX_BUF[0], protocol::CRSF_ADDRESS_CRSF_RECEIVER);
        }
    }

    #[test]
    fn test_inter_byte_timeout_resync() {
        reset_state();

        // 1. Feed partial truncated frame (5 bytes of an expected 26-byte frame) at t = 1000
        uart::mock::push_rx_bytes(&[CRSF_ADDRESS_RADIO_TRANSMITTER, 24, 0x16, 0x01, 0x02]);
        poll_telemetry(1000);
        unsafe {
            assert_eq!(RX_LEN, 5, "Parser should have 5 bytes buffered");
        }

        // At t = 1002 (2ms silence), parser should still hold bytes
        poll_telemetry(1002);
        unsafe {
            assert_eq!(RX_LEN, 5, "Parser should keep bytes during <3ms pause");
        }

        // 2. Advance time to t = 1003 (>= 3 ms silence timeout) with no new bytes
        poll_telemetry(1003);
        unsafe {
            assert_eq!(RX_LEN, 0, "Parser must reset RX_LEN after >=3ms bus silence");
        }

        // 3. Immediately feed a valid complete frame at t = 1003
        let mut good_frame = [
            CRSF_ADDRESS_RADIO_TRANSMITTER,
            12,
            CRSF_FRAMETYPE_LINK_STATISTICS,
            80, 85, 99, 10, 0, 2, 3, 0, 0, 0,
            0x00,
        ];
        good_frame[13] = crc8(&good_frame[2..13]);
        uart::mock::push_rx_bytes(&good_frame);
        poll_telemetry(1003);

        let telem = get_telemetry();
        assert!(telem.connected, "Parser must recover and parse the subsequent valid frame");
        assert_eq!(telem.uplink_link_quality, 99);
    }

    #[test]
    fn test_real_world_rp2_device_info_packet() {
        reset_state();
        start_config();
        uart::mock::clear();

        // The exact 27-byte capture from RadioMaster RP2 ExpressLRS receiver:
        // Sync: 0xC8, Len: 0x19 (25), Type: 0x29 (Device Info), Dest: 0xEA, Orig: 0xEE,
        // Name: "RM RP2\0", Serial: "ELRS", HW: 0, FW: 4, Params: 21 (0x15), Ver: 0, CRC: 0x0D
        let rp2_packet: [u8; 27] = [
            0xC8, 0x19, 0x29, 0xEA, 0xEE, 0x52, 0x4D, 0x20, 0x52, 0x50, 0x32, 0x00,
            0x45, 0x4C, 0x52, 0x53, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00,
            0x15, 0x00, 0x0D,
        ];

        uart::mock::push_rx_bytes(&rp2_packet);
        poll_telemetry(1000);

        let engine = get_config_engine();
        assert_eq!(engine.devices_len, 1);
        assert_eq!(engine.devices[0].address, 0xEE);
        assert_eq!(&engine.devices[0].name[..6], b"RM RP2");
        assert_eq!(engine.devices[0].param_count, 21);

        assert!(select_device(0));
        let engine = get_config_engine();
        assert_eq!(engine.device_id, 0xEE);
        assert_eq!(&engine.device_name[..6], b"RM RP2");
        assert_eq!(engine.param_count, 21);
        assert_eq!(engine.state, ElrsConfigState::LoadingParam(1));

        // Pacing guard: 0 outbound packets at t = 1000
        assert_eq!(uart::mock::take_tx().len(), 0, "No packet sent before 40ms pacing guard");

        // Advance to t = 1040:
        poll_telemetry(1040);

        // Handset must reply with Parameter Read for Param 1 Chunk 0 addressed to 0xEE:
        // [0xEE, 0x06, 0x2C, 0xEE, 0xEA, 0x01, 0x00, 0x86]
        let tx = uart::mock::take_tx();
        assert_eq!(tx.len(), 1, "Must emit outbound parameter read for Param 1");
        assert_eq!(tx[0], &[0xEE, 0x06, 0x2C, 0xEE, 0xEA, 0x01, 0x00, 0x86]);
    }

    #[test]
    fn test_param_request_pacing_delay() {
        reset_state();
        set_millis(1000);
        start_config();
        uart::mock::clear();

        // Feed Device Info frame at t = 1000
        let mut frame = [0u8; 32];
        frame[0] = CRSF_ADDRESS_RADIO_TRANSMITTER;
        frame[2] = CRSF_FRAMETYPE_DEVICE_INFO;
        frame[3] = CRSF_ADDRESS_RADIO_TRANSMITTER;
        frame[4] = CRSF_ADDRESS_CRSF_TRANSMITTER;
        let name = b"TX\0";
        frame[5..5 + name.len()].copy_from_slice(name);
        let serial_pos = 5 + name.len();
        let count_pos = serial_pos + 12;
        frame[count_pos] = 2; // param_count = 2
        frame[count_pos + 1] = 1;
        let total = count_pos + 3;
        frame[1] = (total - 2) as u8;
        frame[total - 1] = crc8(&frame[2..total - 1]);

        uart::mock::push_rx_bytes(&frame[..total]);
        poll_telemetry(1000);
        assert!(select_device(0));

        // Guard time: verify NO packet sent during first 39 ms
        assert_eq!(uart::mock::take_tx().len(), 0);
        poll_telemetry(1010);
        assert_eq!(uart::mock::take_tx().len(), 0);
        poll_telemetry(1039);
        assert_eq!(uart::mock::take_tx().len(), 0);

        // At t = 1040 (40ms elapsed), request for Param 1 Chunk 0 is sent
        poll_telemetry(1040);
        let tx = uart::mock::take_tx();
        assert_eq!(tx.len(), 1);
        assert_eq!(tx[0][2], protocol::CRSF_FRAMETYPE_PARAMETER_READ);
        assert_eq!(tx[0][5], 1); // param 1
        assert_eq!(tx[0][6], 0); // chunk 0

        // Now respond with Chunk 0 (chunks_remain = 1) at t = 1060
        let mut chunk0 = [0u8; 24];
        chunk0[0] = CRSF_ADDRESS_RADIO_TRANSMITTER;
        chunk0[2] = CRSF_FRAMETYPE_PARAMETER_SETTINGS_ENTRY;
        chunk0[3] = CRSF_ADDRESS_RADIO_TRANSMITTER;
        chunk0[4] = CRSF_ADDRESS_CRSF_TRANSMITTER;
        chunk0[5] = 1; // param 1
        chunk0[6] = 1; // chunks_remain = 1
        chunk0[7] = 0; // parent
        chunk0[8] = CRSF_TYPE_SELECT;
        chunk0[9..14].copy_from_slice(b"Test\0");
        chunk0[1] = (chunk0.len() - 2) as u8;
        chunk0[chunk0.len() - 1] = crc8(&chunk0[2..chunk0.len() - 1]);

        uart::mock::push_rx_bytes(&chunk0);
        poll_telemetry(1060);

        // Guard time again: verify 0 packets sent between 1060 and 1099
        assert_eq!(uart::mock::take_tx().len(), 0);
        poll_telemetry(1080);
        assert_eq!(uart::mock::take_tx().len(), 0);
        poll_telemetry(1099);
        assert_eq!(uart::mock::take_tx().len(), 0);

        // At t = 1100 (1060 + 40ms), request for Chunk 1 sent
        poll_telemetry(1100);
        let tx2 = uart::mock::take_tx();
        assert_eq!(tx2.len(), 1);
        assert_eq!(tx2[0][5], 1);
        assert_eq!(tx2[0][6], 1);
    }

    #[test]
    fn test_duplicate_chunk_discard() {
        reset_state();
        unsafe {
            CONFIG_ENGINE.device_id = CRSF_ADDRESS_CRSF_TRANSMITTER;
            CONFIG_ENGINE.param_count = 1;
            CONFIG_ENGINE.state = ElrsConfigState::LoadingParam(1);
        }

        // Chunk 0: chunks_remain = 2
        let mut chunk0 = [0u8; 20];
        chunk0[0] = CRSF_ADDRESS_RADIO_TRANSMITTER;
        chunk0[2] = CRSF_FRAMETYPE_PARAMETER_SETTINGS_ENTRY;
        chunk0[3] = CRSF_ADDRESS_RADIO_TRANSMITTER;
        chunk0[4] = CRSF_ADDRESS_CRSF_TRANSMITTER;
        chunk0[5] = 1; // param 1
        chunk0[6] = 2; // chunks_remain = 2
        chunk0[7..12].copy_from_slice(b"Hello");
        chunk0[1] = (chunk0.len() - 2) as u8;
        chunk0[chunk0.len() - 1] = crc8(&chunk0[2..chunk0.len() - 1]);

        uart::mock::push_rx_bytes(&chunk0);
        poll_telemetry(2000);

        let engine = get_config_engine();
        assert_eq!(engine.current_chunk, 1);
        assert_eq!(engine.expect_chunks_remain, 1);
        unsafe {
            assert_eq!(CHUNK_LEN, 12); // 12 payload bytes
        }

        // Duplicate or stale chunk arrived with chunks_remain = 2 (expected 1)
        uart::mock::push_rx_bytes(&chunk0);
        poll_telemetry(2010);

        // Verify accumulator was NOT corrupted by duplicate chunk
        let engine = get_config_engine();
        assert_eq!(engine.current_chunk, 1, "Must not advance chunk on duplicate");
        assert_eq!(engine.expect_chunks_remain, 1);
        unsafe {
            assert_eq!(CHUNK_LEN, 12, "Accumulator length must remain untouched");
        }
    }

    #[test]
    fn test_param_timeout_retry_and_recovery() {
        reset_state();
        unsafe {
            CONFIG_ENGINE.device_id = CRSF_ADDRESS_CRSF_TRANSMITTER; // 500ms timeout
            CONFIG_ENGINE.param_count = 2;
            CONFIG_ENGINE.state = ElrsConfigState::LoadingParam(1);
            CONFIG_ENGINE.next_req_ms = 1000;
        }

        // First attempt at t = 1000
        poll_telemetry(1000);
        let tx = uart::mock::take_tx();
        assert_eq!(tx.len(), 1, "Attempt 1 sent");
        assert_eq!(tx[0][5], 1); // Param 1

        let engine = get_config_engine();
        assert_eq!(engine.retry_count, 1);

        // Advance to t = 1499: no retry yet (< 500ms timeout)
        poll_telemetry(1499);
        assert_eq!(uart::mock::take_tx().len(), 0);

        // t = 1500: Retry 1
        poll_telemetry(1500);
        let tx = uart::mock::take_tx();
        assert_eq!(tx.len(), 1, "Retry 1 sent");
        let engine = get_config_engine();
        assert_eq!(engine.retry_count, 2);

        // t = 2000: Retry 2
        poll_telemetry(2000);
        assert_eq!(uart::mock::take_tx().len(), 1, "Retry 2 sent");
        let engine = get_config_engine();
        assert_eq!(engine.retry_count, 3);

        // t = 2500: Retry 3
        poll_telemetry(2500);
        assert_eq!(uart::mock::take_tx().len(), 1, "Retry 3 sent");
        let engine = get_config_engine();
        assert_eq!(engine.retry_count, 4);

        // t = 3000: Retries exhausted! Must safely advance to Param 2
        poll_telemetry(3000);
        assert_eq!(uart::mock::take_tx().len(), 0, "No request sent immediately when skipping");
        let engine = get_config_engine();
        assert_eq!(engine.state, ElrsConfigState::LoadingParam(2), "Advanced to Param 2");
        assert_eq!(engine.retry_count, 0, "Retry count reset for next param");

        // t = 3040: Request for Param 2 Chunk 0 sent!
        poll_telemetry(3040);
        let tx = uart::mock::take_tx();
        assert_eq!(tx.len(), 1);
        assert_eq!(tx[0][5], 2, "Requested Param 2");
    }

    #[test]
    fn test_discovery_ping_interval_1000ms() {
        reset_state();
        set_millis(1000);
        start_config();

        // Initial ping sent by start_config()
        let tx = uart::mock::take_tx();
        assert_eq!(tx.len(), 1, "Initial ping sent");

        // Polling before 1000ms elapsed must NOT send additional pings
        poll_telemetry(1100);
        poll_telemetry(1500);
        poll_telemetry(1999);
        assert_eq!(uart::mock::take_tx().len(), 0, "No ping before 1000ms interval");

        // At t = 2000 (>= 1000ms), second ping is sent
        poll_telemetry(2000);
        let tx2 = uart::mock::take_tx();
        assert_eq!(tx2.len(), 1, "Second ping sent at t = 2000");
    }

    #[test]
    fn test_remote_receiver_device_info_ignored_and_param_chunk_progression() {
        reset_state();
        set_millis(1000);
        start_config();
        uart::mock::clear();

        // 1. Module (RM RP2, 0xEE, 21 params) replies to ping with Device Info
        let rp2_info: [u8; 27] = [
            0xC8, 0x19, 0x29, 0xEA, 0xEE, 0x52, 0x4D, 0x20, 0x52, 0x50, 0x32, 0x00,
            0x45, 0x4C, 0x52, 0x53, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00,
            0x15, 0x00, 0x0D,
        ];
        uart::mock::push_rx_bytes(&rp2_info);
        poll_telemetry(1000);
        assert!(select_device(0));

        let engine = get_config_engine();
        assert_eq!(engine.device_id, 0xEE);
        assert_eq!(&engine.device_name[..6], b"RM RP2");
        assert_eq!(engine.param_count, 21);
        assert_eq!(engine.state, ElrsConfigState::LoadingParam(1));

        // 2. Remote receiver (RM RP4TD-M, 0xEC, 11 params) broadcasts Device Info over the air
        let rp4td_info: [u8; 36] = [
            0xC8, 0x22, 0x29, 0xEA, 0xEC, 0x52, 0x4D, 0x20, 0x52, 0x50, 0x34, 0x54,
            0x44, 0x2D, 0x4D, 0x20, 0x32, 0x34, 0x30, 0x30, 0x00, 0x45, 0x4C, 0x52,
            0x53, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x80, 0x0B, 0x00, 0x76,
        ];
        uart::mock::push_rx_bytes(&rp4td_info);
        poll_telemetry(1020);

        // Handset must NOT be hijacked by remote receiver 0xEC:
        let engine = get_config_engine();
        assert_eq!(engine.device_id, 0xEE, "device_id must remain 0xEE");
        assert_eq!(&engine.device_name[..6], b"RM RP2", "device_name must remain RM RP2");
        assert_eq!(engine.param_count, 21, "param_count must remain 21, not overwritten to 11");

        // 3. At t = 1040 (40ms pacing delay), request Param 1 Chunk 0 is sent to 0xEE
        poll_telemetry(1040);
        let tx = uart::mock::take_tx();
        assert_eq!(tx.len(), 1);
        assert_eq!(tx[0][0], 0xEE, "Dest wire byte must be 0xEE");
        assert_eq!(tx[0][3], 0xEE, "Dest payload byte must be 0xEE");
        assert_eq!(tx[0][5], 1, "Param 1");
        assert_eq!(tx[0][6], 0, "Chunk 0");

        // 4. Feed real capture Chunk 0 of Param 1 ("Packet Rate") from 0xEE (chunks_remain = 3)
        let rp2_param1_chunk0: [u8; 64] = [
            0xC8, 0x3E, 0x2B, 0xEA, 0xEE, 0x01, 0x03, 0x00, 0x09, 0x50, 0x61, 0x63,
            0x6B, 0x65, 0x74, 0x20, 0x52, 0x61, 0x74, 0x65, 0x00, 0x35, 0x30, 0x48,
            0x7A, 0x28, 0x2D, 0x31, 0x31, 0x35, 0x64, 0x42, 0x6D, 0x29, 0x3B, 0x31,
            0x30, 0x30, 0x48, 0x7A, 0x20, 0x46, 0x75, 0x6C, 0x6C, 0x28, 0x2D, 0x31,
            0x31, 0x32, 0x64, 0x42, 0x6D, 0x29, 0x3B, 0x31, 0x35, 0x30, 0x48, 0x7A,
            0x28, 0x2D, 0x31, 0x8A,
        ];
        uart::mock::push_rx_bytes(&rp2_param1_chunk0);
        poll_telemetry(1050);

        let engine = get_config_engine();
        assert_eq!(engine.current_chunk, 1, "Must advance to Chunk 1");
        assert_eq!(engine.expect_chunks_remain, 2, "Must expect chunks_remain = 2 next");
        unsafe {
            assert_eq!(CHUNK_LEN, 56, "56 bytes accumulated from Chunk 0");
        }

        // 5. At t = 1090 (1050 + 40ms pacing delay), request Chunk 1 addressed to 0xEE!
        poll_telemetry(1090);
        let tx2 = uart::mock::take_tx();
        assert_eq!(tx2.len(), 1);
        assert_eq!(tx2[0][0], 0xEE, "Dest wire byte must be 0xEE");
        assert_eq!(tx2[0][3], 0xEE, "Dest payload byte must be 0xEE");
        assert_eq!(tx2[0][5], 1, "Param 1");
        assert_eq!(tx2[0][6], 1, "Chunk 1");
    }

    #[test]
    fn test_multi_device_discovery_and_selection() {
        reset_state();
        set_millis(1000);
        start_config();
        uart::mock::clear();

        // 1. Device 1 responds: TX module (RM RP2, 0xEE, 21 params)
        let rp2_info: [u8; 27] = [
            0xC8, 0x19, 0x29, 0xEA, 0xEE, 0x52, 0x4D, 0x20, 0x52, 0x50, 0x32, 0x00,
            0x45, 0x4C, 0x52, 0x53, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00,
            0x15, 0x00, 0x0D,
        ];
        uart::mock::push_rx_bytes(&rp2_info);
        poll_telemetry(1010);

        // 2. Device 2 responds: RX module (RM RP4TD-M 2400, 0xEC, 11 params)
        let rp4td_info: [u8; 36] = [
            0xC8, 0x22, 0x29, 0xEA, 0xEC, 0x52, 0x4D, 0x20, 0x52, 0x50, 0x34, 0x54,
            0x44, 0x2D, 0x4D, 0x20, 0x32, 0x34, 0x30, 0x30, 0x00, 0x45, 0x4C, 0x52,
            0x53, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x80, 0x0B, 0x00, 0x76,
        ];
        uart::mock::push_rx_bytes(&rp4td_info);
        poll_telemetry(1020);

        // 3. Repeat ping response from TX (deduplication check)
        uart::mock::push_rx_bytes(&rp2_info);
        poll_telemetry(1030);

        let engine = get_config_engine();
        assert_eq!(engine.state, ElrsConfigState::Discovering);
        assert_eq!(engine.devices_len, 2, "Must deduplicate and register exactly 2 devices");
        assert_eq!(engine.devices[0].address, 0xEE);
        assert_eq!(&engine.devices[0].name[..6], b"RM RP2");
        assert_eq!(engine.devices[0].param_count, 21);
        assert_eq!(engine.devices[1].address, 0xEC);
        assert_eq!(&engine.devices[1].name[..10], b"RM RP4TD-M");
        assert_eq!(engine.devices[1].param_count, 11);

        // 4. Select the over-the-air Receiver (Device 1)
        assert!(select_device(1));

        let engine = get_config_engine();
        assert_eq!(engine.device_id, 0xEC, "Device ID must switch to 0xEC for receiver");
        assert_eq!(engine.param_count, 11);
        assert_eq!(engine.state, ElrsConfigState::LoadingParam(1));

        // Pacing delay: verify request for receiver param 1 chunk 0 sent after 40ms
        poll_telemetry(1070);
        let tx = uart::mock::take_tx();
        assert_eq!(tx.len(), 1);
        assert_eq!(tx[0][0], 0xEC, "Outbound wire byte must be targeted to 0xEC");
        assert_eq!(tx[0][3], 0xEC, "Outbound payload dest must be 0xEC");
        assert_eq!(tx[0][5], 1); // Param 1
        assert_eq!(tx[0][6], 0); // Chunk 0

        // 5. Test return to device list
        return_to_device_list();
        let engine = get_config_engine();
        assert_eq!(engine.state, ElrsConfigState::Discovering);
        assert_eq!(engine.devices_len, 2, "Discovered devices must persist when returning to list");
    }

    #[test]
    fn test_folder_hierarchy_and_filtering() {
        reset_state();
        unsafe {
            CONFIG_ENGINE.params_len = 4;
            // Param 1: Packet Rate (root)
            CONFIG_ENGINE.params[0] = Parameter {
                id: 1,
                parent: 0,
                param_type: protocol::CRSF_TYPE_SELECT,
                name: *b"Packet Rate\0\0\0\0\0",
                name_len: 11,
                value: 0,
                max_value: 3,
                options: [0; 160],
                options_len: 0,
                status: 0,
            };
            // Param 2: VTX Admin folder (id 2, parent 0)
            CONFIG_ENGINE.params[1] = Parameter {
                id: 2,
                parent: 0,
                param_type: protocol::CRSF_TYPE_FOLDER,
                name: *b"VTX Admin\0\0\0\0\0\0\0",
                name_len: 9,
                value: 0,
                max_value: 0,
                options: [0; 160],
                options_len: 0,
                status: 0,
            };
            // Param 3: Band (parent 2)
            CONFIG_ENGINE.params[2] = Parameter {
                id: 3,
                parent: 2,
                param_type: protocol::CRSF_TYPE_SELECT,
                name: *b"Band\0\0\0\0\0\0\0\0\0\0\0\0",
                name_len: 4,
                value: 1,
                max_value: 4,
                options: [0; 160],
                options_len: 0,
                status: 0,
            };
            // Param 4: Channel (parent 2)
            CONFIG_ENGINE.params[3] = Parameter {
                id: 4,
                parent: 2,
                param_type: protocol::CRSF_TYPE_SELECT,
                name: *b"Channel\0\0\0\0\0\0\0\0\0",
                name_len: 7,
                value: 2,
                max_value: 7,
                options: [0; 160],
                options_len: 0,
                status: 0,
            };
        }

        let mut indices = [0usize; MAX_PARAMS];

        // Root folder (0): must contain Param 1 and Param 2 (count = 2)
        let root_count = get_folder_params(0, &mut indices);
        assert_eq!(root_count, 2);
        assert_eq!(indices[0], 0); // Param 1 index
        assert_eq!(indices[1], 1); // Param 2 index

        // VTX Admin folder (2): must contain Param 3 and Param 4 (count = 2)
        let vtx_count = get_folder_params(2, &mut indices);
        assert_eq!(vtx_count, 2);
        assert_eq!(indices[0], 2); // Param 3 index
        assert_eq!(indices[1], 3); // Param 4 index

        // Parent lookup: parent of VTX Admin (2) must be 0
        assert_eq!(get_parent_folder(2), 0);
        // Parent of Band (3) must be 2
        assert_eq!(get_parent_folder(3), 2);

        // Name lookup
        let mut name_buf = [0u8; 16];
        let fname = get_folder_name(2, &mut name_buf);
        assert_eq!(fname, "VTX Admin");
    }

    #[test]
    fn test_in_place_editing_and_set_param_value() {
        reset_state();
        unsafe {
            CONFIG_ENGINE.device_id = CRSF_ADDRESS_CRSF_TRANSMITTER;
            CONFIG_ENGINE.params_len = 1;
            let mut opts = [0u8; 160];
            let opt_str = b"10mW;25mW;100mW;250mW";
            opts[..opt_str.len()].copy_from_slice(opt_str);
            CONFIG_ENGINE.params[0] = Parameter {
                id: 1,
                parent: 0,
                param_type: protocol::CRSF_TYPE_SELECT,
                name: *b"Power\0\0\0\0\0\0\0\0\0\0\0",
                name_len: 5,
                value: 0, // initially 10mW
                max_value: 3,
                options: opts,
                options_len: opt_str.len() as u8,
                status: 0,
            };
        }

        let p = &get_config_engine().params[0];
        let mut buf = [0u8; 16];
        assert_eq!(p.current_option_str(&mut buf), "10mW");
        assert_eq!(p.option_str_for_val(1, &mut buf), "25mW");
        assert_eq!(p.option_str_for_val(2, &mut buf), "100mW");
        assert_eq!(p.option_str_for_val(3, &mut buf), "250mW");

        // Simulate committing tentative edit value = 2 (100mW)
        uart::mock::clear();
        set_param_value(0, 2);

        // Param value updated locally
        let p_updated = &get_config_engine().params[0];
        assert_eq!(p_updated.value, 2);
        assert_eq!(p_updated.current_option_str(&mut buf), "100mW");

        // Param Write frame (0x2D) transmitted
        let tx = uart::mock::take_tx();
        assert_eq!(tx.len(), 1, "Must transmit 0x2D Param Write");
        assert_eq!(tx[0][0], CRSF_ADDRESS_CRSF_TRANSMITTER);
        assert_eq!(tx[0][2], protocol::CRSF_FRAMETYPE_PARAMETER_WRITE);
        assert_eq!(tx[0][5], 1, "Param ID 1");
        assert_eq!(tx[0][6], 2, "New value = 2");
    }

    #[test]
    fn test_device_role_strings() {
        assert_eq!(protocol::device_role_str(CRSF_ADDRESS_CRSF_TRANSMITTER), "TX");
        assert_eq!(protocol::device_role_str(protocol::CRSF_ADDRESS_CRSF_RECEIVER), "RX");
        assert_eq!(protocol::device_role_str(protocol::CRSF_ADDRESS_FLIGHT_CONTROLLER), "FC");
        assert_eq!(protocol::device_role_str(0x55), "DEV");
    }
}

