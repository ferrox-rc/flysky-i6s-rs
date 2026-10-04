# Changelog

All notable changes to the `flysky-i6s-rs` project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.1] - 2026-10-03

### Fixed
- **Fast Panic Reset & Boot-Loop Protection ([`src/main.rs`](src/main.rs), [`src/chip/mod.rs`](src/chip/mod.rs))**:
  - Replaced infinite `panic_halt` loop with immediate NVIC system reset.
  - Implemented uninitialized SRAM boot-loop guard (`BOOT_CRASH_COUNTER` at `0x2000_3FE0`). Rapid crashes (>3 successive faults within 5 seconds) automatically fall back into STM32 ROM DFU recovery mode.
- **RF Transmission Gating & Bounded Staleness ([`src/main.rs`](src/main.rs), [`src/mixer.rs`](src/mixer.rs), [`src/rf/mod.rs`](src/rf/mod.rs))**:
  - Gated RF frame broadcast until mixer publishes its first valid channel packet, preventing transmission of uninitialized zero-frames on cold boot.
  - Added transmission staleness timeout bounding maximum AFHDS 2A telemetry/link hold to ~115 ms.
- **Telemetry Triple Buffer Preemption Race Decoupling ([`src/rf/mod.rs`](src/rf/mod.rs))**:
  - Decoupled telemetry consumer index from shared atomics, preventing preemption races between high-frequency RF ISRs and main-loop telemetry readers.
- **AFHDS 2A RX ID Flash Persistence on Disarm ([`src/main.rs`](src/main.rs))**:
  - Persists newly bound receiver IDs to non-volatile flash storage upon throttle disarm.
  - Guarded flash write calls against redundant re-flashing and reentrancy.
- **Debounced Model Trim Auto-Save ([`src/main.rs`](src/main.rs))**:
  - Implemented a 3-second debounced background timer that flushes modified model trims to flash outside of ISR / flight-critical code paths when disarmed or idle.

### Added
- **Dynamic Channel Monitor Labels ([`src/ui/menu/screens/channels.rs`](src/ui/menu/screens/channels.rs))**:
  - Channel Monitor screen now dynamically resolves channel names based on active model wing/tail mixers (Elevon, V-Tail) and auxiliary channel mappings (CH5–CH18) instead of generic channel indices.
- **PA15 Touch Reset Electrical Dynamics & CRSF Wiring Guide ([`docs/HARDWARE_REFERENCE.md`](docs/HARDWARE_REFERENCE.md), [`docs/CRSF_ELRS_GUIDE.md`](docs/CRSF_ELRS_GUIDE.md))**:
  - Documented PCB test pads `TX` (`PD5`) and `RX` (`PA15`) adjacent to internal connector `J15`.
  - Added electrical timing and baud rate sensitivity analysis for the FT6236 $\ge 1.0\text{ ms}$ active-low reset threshold versus UART framing bit times across 115.2k, 416.6k, 420k, and 921.6k baud.
  - Documented passive RC low-pass filter ($\tau = 1.0\text{ ms}$), VDD tie-high, and single-wire half-duplex decoupling strategies.

## [0.1.0] - 2026-10-01

### Added
- **Initial FlySky FS-i6S Standalone Firmware Bring-Up**:
  - Dedicated, standalone open-source firmware for the FlySky FS-i6S transmitter with touchscreen, forked from `flysky-i6x-rs` v0.19.1.
  - Initialized crate naming `flysky-i6s-rs` and initial semantic version `v0.1.0`.
- **Electronic Power Latching & Soft Shutdown Subsystem ([`src/power.rs`](src/power.rs), [`src/main.rs`](src/main.rs))**:
  - Hardware electronic latch on `PB15` driven HIGH as the first instruction in `main()` to maintain the transmitter power rail.
  - Push-button shutdown monitoring on `PB14` with boot-release debounce protection and hold threshold (>= 1.5s).
  - Safe Flash storage compaction and flush before de-asserting `PB15` to power down.
  - Graphical shutdown confirmation modal with real-time countdown progress bar.
  - Dual power button blue status LEDs on `PD10` and `PD11`.
- **FocalTech FT6236 Capacitive Touchscreen Driver ([`src/touch/ft6236.rs`](src/touch/ft6236.rs), [`src/touch/mod.rs`](src/touch/mod.rs))**:
  - Hardware I2C1 master driver on `PB8` (SCL) and `PB9` (SDA) operating at 400 kHz Fast Mode.
  - Active-low hardware reset on `PA15` and interrupt line monitoring on `PC12`.
  - Non-blocking 7-byte register packet reader parsing touch points, event kinds, and gesture directions.
  - Coordinate mapping and 90° axis swap converting raw 320x320 touch coordinates to 128x64 LCD display geometry.
- **Touch Navigation & Virtual / Modifier Digital Trims ([`src/touch/nav.rs`](src/touch/nav.rs))**:
  - Touch-to-nav translator converting Swipe gestures (Up/Down/Left/Right) and Direct Tap zones (Top/Bottom/Left/Right/Center) into UI navigation events.
  - Rear tactile push-buttons: Rear Left (`PA9`) mapped to Cancel/Back and Rear Right (`PA10`) mapped to OK/Select.
  - Dual digital trim methods replacing mechanical rockers:
    1. **Stick Modifier Mode**: Hold Rear Left (`PA9`) + Left Stick for Yaw and Throttle trims; hold Rear Right (`PA10`) + Right Stick for Roll and Pitch trims.
    2. **Virtual Touch Targets**: Perimeter taps on the flight dashboard nudge trim axes directly.
- **Zero-Disassembly DFU Bootloader Manager ([`src/boot.rs`](src/boot.rs), [`src/chip/mod.rs`](src/chip/mod.rs), [`src/usb/serial.rs`](src/usb/serial.rs), [`src/ui/menu/screens/diag.rs`](src/ui/menu/screens/diag.rs))**:
  - Cold-boot combo: Hold both Rear Buttons (`PA9` + `PA10`) during power-on to jump directly into the factory ROM DFU bootloader without opening the transmitter case.
  - Soft reboot to DFU: Magic flag `0xDEADBEEF` at SRAM `0x2000_3FF0` checked on reset.
  - Touch Menu option: `Diagnostics -> [OK] Reboot DFU`.
  - USB CDC CLI commands: `dfu`, `reboot bootloader`, and `reboot-bootloader`.
  - Power latch preservation: `rcc_deinit()` preserves `GPIOB` clock (`0x0004_0014`) and re-latches `PB15` HIGH before and after RCC de-initialization so the device remains powered in ROM DFU.
- **FS-i6S Battery Voltage Sensing Scaling ([`src/input.rs`](src/input.rs))**:
  - Updated battery ADC divider formula for the FS-i6S $10\text{ k}\Omega / 5.1\text{ k}\Omega$ resistive divider ($V_{bat} = V_{adc} \times 2.9608$, integer ratio `(raw * 977064) / 409500`).
- **Complete Documentation Overhaul**:
  - Updated `README.md`, `CHANGELOG.md`, `CONTRIBUTING.md`, and all guides in `docs/` to reflect the FS-i6S hardware architecture, pinout, touchscreen navigation, and electronic power management.

## [flysky-i6x-rs Heritage]

### Added
- **Per-Model AFHDS 2A / i-BUS Receiver Settings in Protocol Setup ([`src/storage.rs`](src/storage.rs), [`src/main.rs`](src/main.rs), [`src/ui/menu/screens/setup.rs`](src/ui/menu/screens/setup.rs), [`src/ui/menu/screens/model.rs`](src/ui/menu/screens/model.rs))**:
  - Relocated receiver refresh rate (`servo_rate_hz`), output mode (`rx_out_mode`), and serial protocol (`rx_serial_proto`) from global **Radio Setup** into model-specific **Protocol Setup** under `Proto: AFHDS 2A`.
  - Stored receiver configuration per-model in `ModelConfig` (`rx_out_mode: u8`, `servo_rate_hz: u16`, `rx_serial_proto: u8`, `_reserved: [u8; 1]`) with natural 2-byte alignment, preserving strict 128-byte `ModelConfig` and 2,688-byte `RadioStorage` guarantees without padding.
  - Dynamically updates active hardware receiver settings on model change and boot-up.
  - Streamlined **Radio Setup** from 12 items to 9 pure radio/system configuration items.
- **CRSF Protocol Universal Naming**:
  - Renamed `CRSF/ELRS` to **`CRSF`** throughout the UI and documentation, reflecting universal compatibility across Crossfire, ExpressLRS, and third-party CRSF systems.

## [0.19.0] - 2026-09-30

### Added
- **CRSF Multi-Device Discovery Expansion & Scrollable Device Picker ([`src/crsf/mod.rs`](src/crsf/mod.rs), [`src/ui/menu/screens/elrs.rs`](src/ui/menu/screens/elrs.rs), [`docs/CRSF_ELRS_GUIDE.md`](docs/CRSF_ELRS_GUIDE.md))**:
  - Expanded `MAX_DISCOVERED_DEVICES` from 4 to 16 devices (+288 bytes `.bss`), enabling full auto-discovery for complex multi-node setups (TX module, RX, FC, VTX, 4x ESCs, PDB, telemetry sensors, lighting controllers, and sound modules common on giant-scale aircraft and scale model boats).
  - Added right-edge vertical scrollbar indicator (`widgets::draw_scrollbar`) on the `CRSF DEVICES` selection screen when more than 4 devices are discovered.
  - Implemented dynamic row highlight width (121px vs 124px) and right-aligned role tag offset adjustment to prevent visual collisions with the scrollbar track.
  - Added boundary defensive scroll offset clamping on device list count changes.
  - Added unit test `test_multi_device_discovery_capacity_and_scroll` verifying 16-device discovery, deduplication, full index scroll selection, and safe rejection of capacity overflows.
- **Ferrox-RC Power-On Splash Screen & Synchronized Fanfare ([`src/ui/splash.rs`](src/ui/splash.rs), [`src/ui/glyphs.rs`](src/ui/glyphs.rs), [`src/main.rs`](src/main.rs))**:
  - Kinetic 28x27 Ferrox-RC delta logo emblem rendered directly on LCD initialization with firmware name (`flysky-i6x-rs`) and version display.
  - 1200 ms non-blocking splash screen hold while ADC, DMA, and RF peripherals initialize in parallel.
  - Concurrently triggered 4-note ascending fanfare chime ($C_6 \to E_6 \to G_6 \to C_7$, 660 ms) under `ToneStyle::Rich` (or tactile click under `Simple`).
  - Monotonic SysTick delta-time updates (`now.wrapping_sub(buzzer_last_ms)`) maintaining consistent musical cadence independent of MCU load or flash reads.
  - DO-178C safety principle: warm watchdog recovery completely bypasses splash hold and melody to restore active RF control in $<2$ ms.
- **12x12 Material Design Glyph Icon System ([`src/ui/glyphs.rs`](src/ui/glyphs.rs))**:
  - Dedicated 12x12 vector glyphs for all model types: Airplane, Helicopter, Multirotor/Quad, Glider, and General/Boat.
  - System and telemetry glyphs for RF signal, battery level, timers, gimbals, channels, switches, settings, curves, and matrix mixer.
- **3-Slot Main Settings Icon Menu & Proportional Scrollbar ([`src/ui/menu/mod.rs`](src/ui/menu/mod.rs), [`src/ui/menu/screens/main_menu.rs`](src/ui/menu/screens/main_menu.rs), [`src/ui/widgets.rs`](src/ui/widgets.rs))**:
  - Replaced legacy text list with a modern 3-slot graphical list viewport (14px row height) featuring 12x12 MDI glyphs and high-contrast selection highlights.
  - Right-edge proportional scrollbar widget displaying relative list position (`1/13`).
  - Menu return preservation: exiting any sub-menu preserves the exact cursor and scroll slot rather than resetting to slot 0.
- **5-Page Flight Dashboard & 18-Channel Split Monitor ([`src/ui/dashboard/mod.rs`](src/ui/dashboard/mod.rs), [`src/ui/dashboard/pages/channels.rs`](src/ui/dashboard/pages/channels.rs))**:
  - Split 18-channel live monitor into Page 2/5 (Primary Channels 1..10) and Page 3/5 (Auxiliary Channels 11..18), providing dedicated horizontal bar graphs and exact microsecond readouts (988..2012 µs).
  - Mode 2 natural stick layout: right-aligned throttle bar on Page 1/5.
  - Dual split horizontal potentiometer bar on Page 1/5: top bar displays VRa and bottom bar displays VRb, each with center tick marks.
  - High-visibility switch position arrow glyphs (`^` UP, `-` MID, `v` DOWN).
- **Procedural Continuous Battery Gauge & Status Bar Layout Alignment ([`src/ui/glyphs.rs`](src/ui/glyphs.rs), [`src/ui/dashboard/status_bar.rs`](src/ui/dashboard/status_bar.rs), [`docs/USER_GUIDE.md`](docs/USER_GUIDE.md))**:
  - Implemented continuous procedural min-max subtraction algorithm (`((val_mv.saturating_sub(min_mv) * 8) / span).min(8)`) rendering an 8-pixel solid fill cavity without interior gap columns.
  - Linear right-to-left fill orientation: fills from base ($x+9$) towards tip ($x+2$) as battery charges, emptying tip-to-base as cells deplete.
  - Zero `.rodata` tables: compiles down to ~32 bytes of Thumb-1 instructions, saving flash over bitmap tables and multi-branch match statements.
  - Right-aligned power indicator cluster ($x=93..126$): voltage readout text (`X.YYV`, $x=93$) positioned left of the right-edge battery gauge ($x=116..126$), equalizing margins with the center RF/link status block ($x=64..83$) and left-aligned model name ($x=2..61$).
- **`ModelType::General` Support ([`src/storage.rs`](src/storage.rs), [`src/main.rs`](src/main.rs), [`src/ui/menu/screens/model_setup.rs`](src/ui/menu/screens/model_setup.rs))**:
  - Added `General` model type for surface models (boats, rovers, cars, robotics) with dedicated ship icon glyph.
  - Pre-flight throttle safety interlocks adapted to bypass raised throttle warnings for spring-centered surface throttles.
- **DFU Bootloader Troubleshooting Documentation ([`docs/USER_GUIDE.md`](docs/USER_GUIDE.md), [`README.md`](README.md))**:
  - Documented recovery from `dfuERROR` / corrupt firmware state via `dfu-util -a 0 -e` clear status command.

## [0.18.1] - 2026-09-30

### Fixed
- **AFHDS 2A Servo Refresh Rate Safety ([`src/rf/afhds2a.rs`](src/rf/afhds2a.rs), [`src/storage.rs`](src/storage.rs), [`src/ui/menu/screens/setup.rs`](src/ui/menu/screens/setup.rs))**:
  - Replaced hardcoded 400 Hz servo refresh rate in `build_settings_packet` with a safe default of **50 Hz** (20 ms period) to prevent jitter, overheating, and burnout of standard analog servos.
  - Added configurable servo rate in **Radio Setup** (`Servo Hz:`) cycling safely through `50 Hz`, `60 Hz`, `100 Hz`, `150 Hz`, `200 Hz`, `250 Hz`, `300 Hz`, `350 Hz`, and `400 Hz`.
  - Added hardware bounds checking in `storage.sanitize()` clamping values strictly to `50..=400 Hz` (defaulting to 50 Hz on uninitialized flash).
- **Configurable AFHDS 2A Output Modes**:
  - Added receiver output mode selection in **Radio Setup** (`RX Out:`): toggle between **PWM** (`0x00`) and **PPM** (`0x01`).
  - Added receiver serial telemetry protocol selection in **Radio Setup** (`Serial:`): toggle between **i-BUS** (`0xDE`) and **S.BUS** (`0xDD`).
  - Implemented dynamic over-the-air synchronization via `PacketType::Settings` to reconfigure receiver hardware on the fly without power cycling.
  - Preserved strict 128-byte `RadioConfig` flash layout by allocating 4 bytes from `_reserved`.

### Added
- **CRSF / ELRS Integer Parameter Support ([`src/crsf/mod.rs`](src/crsf/mod.rs), [`src/crsf/protocol.rs`](src/crsf/protocol.rs), [`src/ui/menu/screens/elrs.rs`](src/ui/menu/screens/elrs.rs))**:
  - Full decoding and parsing for CRSF integer parameters: `CRSF_TYPE_UINT8` (0), `CRSF_TYPE_INT8` (1), `CRSF_TYPE_UINT16` (2), and `CRSF_TYPE_INT16` (3).
  - Captures value, min, max, default bounds, and unit strings (e.g., `ch`, `%`, `mW`, `us`).
  - Added in-place modal editing (`[OK]` to edit, `[UP]`/`[DOWN]` clamped to `[min, max]`, `[OK]` to commit write, `[ESC]` to cancel).
  - Multi-byte parameter write frame builder (`build_param_write_frame_multi`) supporting 1-byte and 2-byte big-endian writes.
  - Implemented `i32_to_dec` formatting helper supporting negative, zero, and positive decimal numbers without 64-bit division.
- **Configurator Buffer Unification & Expansion**:
  - Unified parameter capacity to `MAX_PARAMS = 48` per folder (removing the obsolete 26-param limit).
  - Expanded device-wide parent mapping to `MAX_PARAM_MAP = 96`.
  - Unified string pool to `STRING_POOL_SIZE = 1280` bytes (1.25 KB) for parameter names, options, and units.
  - Replaced magic numbers with `MAX_FOLDER_NAME_LEN = 32` and `MAX_FOLDER_DEPTH = 6`.

### Fixed
- **Nested Subfolder Navigation Header Title ([`src/crsf/mod.rs`](src/crsf/mod.rs))**:
  - Fixed issue where ascending from a sub-subfolder to a subfolder reverted the top header title to `"Folder"`.
  - Introduced `FolderStackItem` storing both folder ID and folder name buffer on the navigation stack, restoring parent titles accurately across up to 6 levels of nesting.

## [0.18.0] - 2026-09-29

### Summary
Major feature release delivering 18-channel i-BUS over-the-air encoding and mixer expansion, `ModelConfig` v5 with backward-compatible v4 flash migration, CRSF Parameter 0 root-folder querying with selective child-ID discovery, dynamic CRSF command polling with live info text feedback, clamped info and string parameter rendering, expanded TBS device role identifiers, system info git commit hash display, and Phase 19 EdgeTX flight countdown/stopwatch timers, unipolar throttle compensation mixing, wing/tail differential mixing, physical switch auto-detection, and model duplication.

### Added
- **18-Channel i-BUS & AFHDS 2A Subsystem ([`src/rf/afhds2a.rs`](src/rf/afhds2a.rs), [`src/mixer.rs`](src/mixer.rs), [`src/storage.rs`](src/storage.rs))**:
  - **Over-the-Air 18-Channel Encoding**: Implemented interleaved payload encoding matching Betaflight/iNav `rx/ibus.c` `updateChannelData`, transmitting all 18 channels reliably over standard AFHDS 2A links.
  - **`ModelConfig` v5 Architecture**: Expanded `channel_reverse` to a 32-bit bitmask and increased auxiliary channel mappings to 14 channels (`aux_channels: [u8; 14]`), preserving the 128-byte alignment invariant with automatic v4-to-v5 sequential storage migration on first boot.
  - **18-Channel Mixer Engine**: Expanded mixer outputs and matrix mixer sources to support `CH1..CH18` and `Thr+` unipolar throttle, with dual-column channel monitor display on flight dashboard Page 2/4.
  - **CRSF Alignment**: Mapped channels 1..16 directly to the 16-channel CRSF stream with zero channel clipping or protocol conflicts.
  - **Failsafe Packet Clamping**: Clamped over-the-air failsafe frames to the standard 14-channel frame bounds to avoid RF receiver sync faults.
- **CRSF Parameter 0 Root-Folder Querying & Selective Child Discovery ([`src/crsf/mod.rs`](src/crsf/mod.rs))**:
  - Automatically queries Parameter 0 (Root Folder descriptor) upon device connection per the TBS CRSF specification.
  - Parses `0x0B FOLDER` child ID lists (`List_of_children`) to discover and query only root items, eliminating full 59-parameter sequential scans and speeding up device initial connection by >80%.
  - Maintains automatic retry fallback (up to 2 retries) for legacy devices lacking Parameter 0 support.
- **Dynamic CRSF Command Polling & Live Feedback ([`src/crsf/mod.rs`](src/crsf/mod.rs), [`src/ui/menu/screens/elrs.rs`](src/ui/menu/screens/elrs.rs))**:
  - Obeyed `Timeout` byte (in units of 100 ms) in `0x0D COMMAND` replies to dynamically pace `0x2D STATUS_POLL` frames (with a safe 250 ms fallback when `timeout == 0`, avoiding tight loops per ExpressLRS Lua Issue #16).
  - Displays live device `Info` text feedback during execution (e.g. `[Binding]`, `[Erasing]`, `[45%]`) instead of static `[Executing...]`.
  - Custom confirmation modal questions (e.g. `"Erase model?"`) derived directly from device-supplied `Info` strings.
  - Dedicated `ActiveCommandState::Completed` state displaying final status strings (e.g. `[OK]`, `[Done]`, `[Failed]`) for ~2.5s or dismissed immediately on `[OK]` / `[ESC]` (per ExpressLRS Lua Issue #17).
- **Clamped Info & String Parameter Rendering ([`src/ui/menu/screens/elrs.rs`](src/ui/menu/screens/elrs.rs))**:
  - Parsed `0x0C INFO` and `0x0A STRING` parameter values into the unified string pool.
  - Right-aligned info values at $x = 124$ with dynamic character clamping and automatic fallback to `FONT_4X6` for long strings, strictly preventing right-edge display overflow.
  - Integrated split footer display (`draw_footer_split`) showing complete, un-truncated info strings when highlighted.
- **Expanded CRSF Device Role Tags ([`src/crsf/protocol.rs`](src/crsf/protocol.rs), [`src/ui/menu/screens/elrs.rs`](src/ui/menu/screens/elrs.rs))**:
  - Expanded role tag strings from the TBS CRSF specification (`[TX]`, `[RX]`, `[FC]`, `[VTX]`, `[WIFI]`, `[ESC1..4]`, `[OSD]`, `[GIMB]`, `[BLU]`).
  - Added hex fallback (`[0x..]`) and `FONT_4X6` rendering to prevent display overflow on unknown device IDs.
- **System Info Git Commit Hash Display ([`src/ui/menu/screens/diag.rs`](src/ui/menu/screens/diag.rs), [`build.rs`](build.rs))**:
  - Captured current shortened git commit hash during build and rendered `v0.18.0 (<hash>)` in small font on the System Info screen.
- **EdgeTX-Parity Flight Timer Subsystem ([`src/main.rs`](src/main.rs), [`src/mixer.rs`](src/mixer.rs), [`src/storage.rs`](src/storage.rs))**:
  - **Multi-Trigger Engine (`is_timer_active`)**: Supports `OFF`, `THs (RUN)` (active while throttle $> 5\%$), `THt (LTCH)` (latched running once throttle $> 5\%$), `ALWAYS ON` (continuous stopwatch), and switch conditions `SA^` through `SDv`.
  - **Arm Switch Integration**: Disarming unlatches and pauses the timer (freezing final flight time for post-landing review). Re-arming automatically resets the timer back to its configured duration and clears elapsed seconds for the next flight pack.
  - **Visual Hold-to-Reset HUD Progress Modal**: Holding `[CANCEL]` for $\ge 1.0\text{ s}$ on any flight dashboard displays a centered modal with a 0..100% animated progress bar, confirmation tone, and `"TIMER RESET!"` toast.
  - **Acoustic Countdown & Alarms**: 1-minute warning chimes, 10s..1s countdown tick beeps, zero-second elapsed alarm, and inverted/blinking negative overdue time display.
  - **Dashboard Navigation**: Added `[UP]` / `[DOWN]` keys to step between flight dashboard pages 1–4.
- **Unipolar Throttle Mixer Source (`Thr+`) ([`src/mixer.rs`](src/mixer.rs))**:
  - Implemented `MixSource::ThrUnipolar` (ID 26) scaling linearly from 0 at idle to +1000 at full throttle, preventing negative pitch-down compensation at idle throttle on aircraft mixes.
- **Physical Switch Auto-Detection ([`src/ui/menu/screens/model.rs`](src/ui/menu/screens/model.rs), [`src/ui/menu/screens/mixer.rs`](src/ui/menu/screens/mixer.rs))**:
  - Automatically selects the flipped physical switch and its active position condition when editing `Arm Sw`, `T-Trig`, or mixer line activation switches.
- **Model Copy / Duplicate Utility ([`src/ui/menu/screens/model.rs`](src/ui/menu/screens/model.rs))**:
  - Added Field 5 (`Copy -> Mxx`) to `MODEL SETUP` allowing instant cloning of model configurations to other slots.
- **Potentiometer Center Acoustic Detent ([`src/input.rs`](src/input.rs), [`src/buzzer.rs`](src/buzzer.rs))**:
  - Emits a crisp non-blocking click tone (`2200 Hz`, 10 ms) whenever `VRA` or `VRB` crosses through neutral center ($0$).
- **Wing/Tail Differential Mixing ([`src/mixer.rs`](src/mixer.rs))**:
  - Enabled active differential throw scaling using the `template_diff` parameter.

## [0.17.0] - 2026-09-28

### Summary
Major protocol and architecture release delivering full TBS Crossfire Protocol Rev 08 and ExpressLRS parameter synchronization specification adherence, FlySky standard channel conversion (988–2012 µs), dynamic physical wire routing with `0xC8` wire sync framing, full TBS-Agent style multi-device discovery with dynamic disconnect pruning, Unified 255-Parameter Pool with 3.5 KB static string pool, hidden parameter suppression, LCD text overlap and clipping fixes, subfolder hierarchy navigation, modal in-place parameter editing, immediate high-speed query dispatch, hardware interrupt-driven USART2 RX with a 128-byte lock-free ring buffer, framing inter-byte resynchronization timeouts, and a dual-target host test harness (`cargo test-host`) with 48 automated unit tests.

### Added
- **Unified 255-Parameter Pool ([`src/crsf/mod.rs`](src/crsf/mod.rs))**:
  - Expanded parameter capacity from 24 to the full protocol ceiling of **255 parameters** using an ultra-compact 12-byte descriptor table and a centralized 3.5 KB static string pool (`STRING_POOL`). Fully accommodates complex flight controllers (80+ parameters) and multi-channel PWM receivers (35+ parameters) while leaving $> 6.5\text{ KB}$ of free SRAM stack headroom.
- **CRSF / ExpressLRS Hidden Parameter Flag Support ([`src/crsf/mod.rs`](src/crsf/mod.rs))**:
  - Obeyed bit 7 (`0x80`) of the parameter type byte (`is_hidden()`) to filter out internal/unintended receiver configuration fields (such as internal UIDs) from folder navigation views.
- **128×64 LCD Parameter Layout & Overlap Bugfix ([`src/ui/menu/screens/elrs.rs`](src/ui/menu/screens/elrs.rs))**:
  - Dynamically calculated maximum name widths based on value length, right-aligned values ending cleanly at $x = 124$, eliminated the off-screen clipping bug, and cleanly stripped parenthetical sensitivity suffixes (e.g. `250Hz(-108dBm)` $\to$ `250Hz`) when line space is constrained.
- **Immediate High-Speed Query Dispatch ([`src/crsf/mod.rs`](src/crsf/mod.rs))**:
  - Outbound sequential parameter queries and multi-chunk requests are dispatched immediately upon receipt and validation of preceding chunks/parameters, eliminating artificial pacing delays and matching native TBS-Agent and ELRS Lua wire performance.
- **Dynamic Device List Auto-Pruning ([`src/crsf/mod.rs`](src/crsf/mod.rs), [`src/ui/menu/screens/elrs.rs`](src/ui/menu/screens/elrs.rs))**:
  - Implemented automatic heartbeat tracking for all discovered devices. Devices not responding to 1 Hz pings for > 3000 ms (3 missed pings) are pruned from the `CRSF DEVICES` list, with list bounds and UI cursor selection safely clamped.
- **Hardware Interrupt-Driven USART2 RX Subsystem ([`src/crsf/uart.rs`](src/crsf/uart.rs))**:
  - **Atomic Lock-Free Ring Buffer**: Implemented 128-byte `RX_RING` with volatile pointer synchronization, safely decoupling high-speed 420,000 baud byte ingestion from the main loop and eliminating `USART_ISR_ORE` hardware overruns during long operations (e.g. LCD flushing).
  - **NVIC Priority Configuration**: Unmasks IRQ 28 in the NVIC with high priority (`0x40`) when CRSF mode is enabled, and masks it when disabled.
  - **Hardware ORE Auto-Clearing**: ISR detects and clears `USART_ISR_ORE` to prevent receiver lockups.
- **Dual-Target Host Unit Test Harness ([`.cargo/config.toml`](.cargo/config.toml), [`Cargo.toml`](Cargo.toml))**:
  - Established `cargo test-host` alias targeting `x86_64-unknown-linux-gnu` with `#![cfg_attr(not(test), no_std)]` in `src/lib.rs`.
  - Expanded host unit test suite to 48 passing tests covering 255-parameter pooling, hidden flag filtering, device pruning, and framing validation with zero hardware dependencies.
- **TBS-Agent Style Multi-Device Discovery & Device Picker ([`src/crsf/mod.rs`](src/crsf/mod.rs), [`src/ui/menu/screens/elrs.rs`](src/ui/menu/screens/elrs.rs))**:
  - Broadcasts 1 Hz discovery pings and registers all responding bus devices (transmitters `0xEE`, receivers `0xEC`, flight controllers `0xC8`) into a deduplicated table (`DiscoveredDevice`).
  - Presents an interactive `CRSF DEVICES` screen with role tags (`[TX]`, `[RX]`, `[FC]`), allowing pilots to configure either the transmitter module or over-the-air receiver directly.
- **Hierarchical Subfolder Tree Navigation ([`src/crsf/mod.rs`](src/crsf/mod.rs), [`src/ui/menu/screens/elrs.rs`](src/ui/menu/screens/elrs.rs))**:
  - Filters parameters by `parent` ID, rendering folder items (`CRSF_TYPE_FOLDER`) with trailing chevron (`>`).
  - Pressing `[OK]` drills down into subfolders; pressing `[ESC]` ascends to the parent folder or returns to the Device Picker at root level.
- **Modal In-Place Parameter Option Editing ([`src/crsf/mod.rs`](src/crsf/mod.rs), [`src/ui/menu/screens/elrs.rs`](src/ui/menu/screens/elrs.rs))**:
  - Pressing `[OK]` on a `SELECT` parameter enters Edit Mode, displaying the tentative value with `< Option >` brackets.
  - `[UP]` / `[DOWN]` cycles options locally without transmitting premature serial packets; pressing `[OK]` commits and transmits `0x2D Param Write` frame; `[ESC]` cancels without changes.
- **Comprehensive CRSF Protocol Documentation ([`docs/CRSF_PROTOCOL_SPEC.md`](docs/CRSF_PROTOCOL_SPEC.md), [`docs/CRSF_ELRS_GUIDE.md`](docs/CRSF_ELRS_GUIDE.md))**:
  - Detailed byte-level framing breakdown, wire timing diagrams, CRC-8 DVB-S2 poly formulas, parameter state machine transitions, command action lifecycle, and manual verification walkthrough.

### Changed
- **FlySky Standard Microsecond Channel Conversion ([`src/crsf/protocol.rs`](src/crsf/protocol.rs))**:
  - Replaced magic numbers and aligned CRSF 11-bit scaling to standard FlySky microsecond boundaries: `988 µs` (172 counts), `1500 µs` (992 counts), and `2012 µs` (1811 counts).
  - Applied global constants across mixer, trim, and protocol modules.
- **TBS CRSF Wire Framing Specification Adherence ([`src/crsf/protocol.rs`](src/crsf/protocol.rs))**:
  - Aligned all outbound parameter read, parameter write, and command frames to begin with wire sync byte `0xC8` (`CRSF_SYNC_BYTE`) per TBS CRSF specification rather than the target address, resolving over-the-air receiver parameter loading.
- **Dynamic Wire Destination Addressing ([`src/crsf/protocol.rs`](src/crsf/protocol.rs))**:
  - Changed `build_param_ext_frame()` to dynamically assign payload destination byte (`out_frame[3] = target;`), properly routing frames to transmitter modules (`0xEE`), receivers (`0xEC`), or flight controllers (`0xC8`).

### Fixed
- **CRSF Target Device ID Isolation & Remote Receiver Hijack Prevention ([`src/crsf/mod.rs`](src/crsf/mod.rs))**:
  - Filtered `handle_device_info_frame()` and `handle_param_entry_frame()` to strictly match `CONFIG_ENGINE.device_id` (`0xEE`). Previously, when a bound remote receiver (`0xEC`, e.g. `RM RP4TD-M 2400`) broadcast `DEVICE_INFO` over the air, `CONFIG_ENGINE.device_id` was overwritten mid-handshake from `0xEE` to `0xEC` and `param_count` was overwritten from 21 to 11. This caused subsequent chunk requests to be sent over the air to `0xEC` instead of the local TX module, causing parameter 1 to stall and fail with `"No parameters found"`.
- **Multi-Frame Parameter Accumulator & Option Buffer Expansion ([`src/crsf/mod.rs`](src/crsf/mod.rs))**:
  - Expanded `CHUNK_BUF` from 96 bytes to 320 bytes to safely reassemble full 4-chunk parameter streams (e.g. ExpressLRS `"Packet Rate"` with 10 options).
  - Expanded `Parameter.options` and parsing buffers from 48 bytes to 160 bytes so long option lists are not truncated.
  - Increased `MAX_PARAMS` from 16 to 24 to fully accommodate modules with 21 parameters (such as RadioMaster RP2).
- **Over-The-Air Chunk Sequencing and Duplicate Filtering ([`src/crsf/mod.rs`](src/crsf/mod.rs))**:
  - Added `expect_chunks_remain` sequence checking in `handle_param_entry_frame()`, safely discarding duplicate or out-of-order chunks from RF re-transmissions without corrupting the chunk accumulator buffer.
- **Parameter Read Timeout and Retry Recovery ([`src/crsf/mod.rs`](src/crsf/mod.rs))**:
  - Implemented timeout tracking (500 ms for local TX `0xEE`, 1000 ms for remote receivers `0xEC`) with up to 4 retries before automatically advancing to the next parameter, preventing the handset from locking up indefinitely on `"loading parameter 01 of 11"`.
- **UI Menu Button Debounce on Configuration Launch ([`src/ui/menu/screens/setup.rs`](src/ui/menu/screens/setup.rs), [`src/ui/menu/screens/elrs.rs`](src/ui/menu/screens/elrs.rs))**:
  - Set `ctrl.waiting_release = true` when triggering `[Configure Module]` and retry clicks to prevent 700 µs loop repeat invocations while the physical `[OK]` button is held down.
- **CRSF Receiver Wire Filter Acceptance ([`src/crsf/mod.rs`](src/crsf/mod.rs))**:
  - Added `CRSF_ADDRESS_CRSF_RECEIVER` (`0xEC`) to the frame start address filter in `poll_telemetry()`.
- **Parser Inter-Byte Framing Timeout ([`src/crsf/mod.rs`](src/crsf/mod.rs))**:
  - Added `LAST_RX_BYTE_MS` tracking. If $\ge 3\text{ ms}$ of bus silence elapses with an incomplete frame in `RX_BUF`, `RX_LEN` automatically resets to 0 to prevent parser desynchronization.
- **RadioMaster RP2 Receiver Device Info Ingestion ([`src/crsf/mod.rs`](src/crsf/mod.rs))**:
  - Verified and regression-tested real-world 27-byte capture from RadioMaster RP2 ExpressLRS receiver (`0xC8 0x19 0x29 0xEA 0xEE 52 4D 20 52 50 32 ... 15 00 0D`), ensuring proper parsing of `"RM RP2"`, 21 parameters, and generation of the outbound parameter read response.

---

## [0.16.0-rc.5] - 2026-09-24

### Summary
Safety-critical release resolving the watchdog in-flight trap hazard by introducing hardware reset cause inspection via `RCC_CSR` (`IWDGRSTF` / `WWDGRSTF`), bypassing startup throttle/switch interlocks and DFU settling delays on warm reboot to guarantee deterministic flight recovery in under 2 ms, and adding comprehensive safety architecture documentation.

### Fixed
- **Watchdog In-Flight Trap Resolution (`862dd55`)**:
  - **Hardware Reset Cause Latching ([`src/chip/mod.rs`](src/chip/mod.rs))**: Implemented `chip::check_and_clear_reset_flags()` to inspect STM32 `RCC_CSR` for `IWDGRSTF` (Independent Watchdog) and `WWDGRSTF` (Window Watchdog) status bits before atomically clearing all reset flags via `RMVF`.
  - **$< 2\text{ ms}$ Flight Recovery Pipeline ([`src/main.rs`](src/main.rs))**:
    - **DFU Settling Delay Bypass**: Skips `boot::check_dfu_entry` (eliminating ~6.25 ms contact debounce delay and preventing unintentional System ROM bootloader entry mid-flight) and calls `boot::init_keys()` directly.
    - **Interlock Invalidation**: Inhibits bind-on-boot and direct calibration wizard modal entry when rebooting from a watchdog event.
    - **Pre-Flight Safety Check Bypass**: Bypasses the throttle-at-idle and switch-position warning loop (`SAFETY WARNING!`), which previously trapped the transmitter in an infinite warning state with over-the-air channels locked to 1000 µs failsafe during mid-flight reboots.
    - **Immediate Acoustic Warning**: Replaces the blocking melodic startup chime with a non-blocking 3-beep alarm pattern (`buzzer.play_tone_pattern(2600, 60, 40, 3)`), providing clear acoustic feedback to the pilot while immediately resuming the 100 Hz flight control loop.

### Documentation
- **Safety Architecture & Recovery Flowchart (`27a2e6e`)**:
  - **Watchdog Recovery Architecture ([`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md))**: Added Section 2.1 detailing hardware IWDG configuration (40 kHz LSI oscillator isolation, prescaler `/64`, reload `1250`, 2.0s timeout), bounded `IWDG_SR` synchronization (`10_000` iteration cap), debug halt freeze (`DBGMCU_APB1_FZ.DBG_IWDG_STOP`), and a complete recovery flowchart.
  - **Pilot Safety Alarms ([`docs/USER_GUIDE.md`](docs/USER_GUIDE.md))**: Added Level 5 to Section 8 detailing in-flight watchdog reset detection, interlock bypass, and acoustic alarms.
  - **Hardware Map & Roadmap ([`README.md`](README.md))**: Updated peripheral specifications and completed milestones in Phase 15.

---

## [0.16.0-rc.4] - 2026-09-23

### Summary
Comprehensive architectural upgrade migrating the peripheral driver layer to direct register access via the Peripheral Access Crate (`pac`), implementing a 2.0-second hardware watchdog with debug halt freezing, introducing an 8 KB 4-page append-only log storage engine powered by `sequential-storage` (yielding sub-3ms non-blocking saves with zero page erases on edits), refining USB composite mode with silent CLI connection, and synchronizing all project documentation.

### Added
- **Pure PAC Hardware Peripheral Architecture (`7c45a2c`)**:
  - **System Clock & LSI Stabilization ([`src/chip/mod.rs`](src/chip/mod.rs))**: Migrated clock configuration and flash latency setup to pure PAC (`pac::RCC` and `pac::FLASH`). Added explicit LSI enable (`RCC_CSR.lsion`) with bounded wait loop on `RCC_CSR.lsirdy` to guarantee the 40 kHz internal low-speed oscillator is stable prior to peripheral and watchdog activation.
  - **Key Matrix Scanning ([`src/boot.rs`](src/boot.rs))**: Migrated `init_keys()` and `scan_keys()` to `pac::GPIOC`, `pac::GPIOD`, `pac::GPIOF`, and `pac::RCC`.
  - **Piezo PWM Buzzer ([`src/buzzer.rs`](src/buzzer.rs))**: Migrated PA8 alternate function routing and TIM1 PWM setup to `pac::GPIOA`, `pac::TIM1`, and `pac::RCC`.
  - **ST7567 LCD Driver ([`src/display/st7567.rs`](src/display/st7567.rs))**: Migrated 8-bit parallel bus control (`GPIOE->ODR`) and control strobe sequencing to direct PAC registers. Framebuffer is explicitly cleared (`clear_buffer()`) and pushed to the display (`flush()`) *before* backlight PWM is turned on, preventing power-on visual noise.
- **Deterministic 2.0s Hardware Watchdog (`b4deb69`)**:
  - Independent hardware watchdog implemented via `pac::IWDG` clocked by the 40 kHz LSI oscillator with prescaler `/128` (PR=5, 312.5 Hz tick rate) and reload count `625` (exact 2.000s timeout).
  - **Debug Halt Freezing**: Sets `DBGMCU_APB1_FZ.DBG_IWDG_STOP` with `RCC_APB2ENR.DBGMCUEN` pre-enabled, allowing SWD debuggers (ST-Link, probe-rs, GDB) to pause CPU execution on breakpoints without triggering watchdog resets.
  - **Standalone Zero-Cost Feed**: Inlined `watchdog::feed()` writing key `0xAAAA` to `IWDG_KR`, called at ~500 Hz at the bottom of the main execution loop and during storage operations.
- **Log-Structured Append-Only Flash Storage Engine (`832a72b`)**:
  - **4-Page Memory Allocation**: Pages 60, 61, 62, and 63 (`0x0801_E000 .. 0x0802_0000`, 8,192 bytes total) reserved exclusively for non-volatile storage.
  - **Log-Structured Engine**: Powered by `sequential-storage` (`sequential_storage::map`), mapping Key 0 to `RadioConfig` (128 bytes) and Keys 1..20 to `ModelConfig` profiles (128 bytes each).
  - **Sub-3ms Non-Blocking Saves**: Incremental updates append only ~132 bytes to the open log page in **~2.8 ms with zero page erases**, eliminating the ~50 ms UI stall and loop jitter of whole-page erasing.
  - **Automatic Wear-Levelled Compaction**: When all 4 sectors become full of historical revisions, `sequential-storage` automatically compacts active records into a newly erased page, rotating evenly across Pages 60–63.
  - **Multi-Tier Legacy Migration**: Probes sequential storage first; if empty, automatically imports legacy v3 snapshot data from Page 62 (`0x0801_F000`) or legacy v1/v2 data from Page 63 (`0x0801_F800`) before committing factory defaults.
- **Pure PAC Flash Driver (`FlashStorage`)**:
  - Direct PAC register implementation of `embedded_storage::nor_flash::NorFlash` and `MultiwriteNorFlash` using `pac::FLASH`.
  - Flash unlock sequence via `FLASH_KEYR` (`0x4567_0123`, `0xCDEF_89AB`) and status flag hygiene (`EOP`, `WRPRTERR`, `PGERR`).
- **Main Event Loop & Safety Lifecycle Integration (`1fd6a85`)**:
  - Deterministic boot order: early watchdog feed -> DFU check -> clock init & LSI stabilization -> SysTick -> clean LCD wipe -> watchdog start -> peripheral init -> pre-flight check -> main loop.
  - 100% stick throw preservation with adaptive pre-flight checks: uncalibrated checks raw ADC counts (`state.raw[2] > 1400`), calibrated checks normalized pulses (`state.sticks.throttle > -900`).
- **USB Composite Mode & Silent CLI Experience (`fc8c88f`)**:
  - EdgeTX composite device identity (`0x1209:0x4968`) with Interface Association Descriptors (IAD) enabling simultaneous 100 Hz HID Gamepad and CDC-ACM Virtual COM port.
  - Silent terminal connection: eliminated unsolicited banner transmission on USB enumeration, preventing buffer stalls, FIFO packet drops, and truncated greetings in terminal emulators (`picocom`, `minicom`, PuTTY).
  - Interactive prompt `i6x> ` rendered upon `[Enter]`, with built-in commands: `help`, `status`, `channels`, `telem`, `stream` (continuous 10 Hz JSON telemetry streaming, any key to pause), and `reboot`.

### Changed
- **Memory Map Partitioning (`memory.x`)**:
  - Clamped application flash partition `FLASH (rx)` to `120K` (`0x0800_0000 .. 0x0801_DFFF`, Pages 0–59), physically preventing linker code overflow from invading the storage sector at `0x0801_E000`.
- **Comprehensive Documentation Synchronization (`14874cf`)**:
  - Fully updated [`README.md`](README.md), [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md), [`docs/CALIBRATION_AND_STORAGE.md`](docs/CALIBRATION_AND_STORAGE.md), [`docs/HARDWARE_REFERENCE.md`](docs/HARDWARE_REFERENCE.md), [`docs/USB_SUBSYSTEM.md`](docs/USB_SUBSYSTEM.md), [`docs/MIXER.md`](docs/MIXER.md), and [`docs/USER_GUIDE.md`](docs/USER_GUIDE.md) to reflect PAC drivers, watchdog timing, 4-page sequential storage, and USB composite CLI.

### Fixed
- **Flash Control Register LOCK Bit Re-Assertion (`d6bea38`)**:
  - **Issue**: Standard `modify()` writes on `FLASH_CR` in `stm32f0xx-hal` re-wrote the read state of bit 7 (`LOCK`), inadvertently re-locking the flash peripheral before page erase or halfword programming could execute.
  - **Resolution**: Implemented PAC register writes using `write_with_zero` on `FLASH_CR` with `LOCK = 0`, ensuring flash remains unlocked throughout erase and write sequences.
- **Flash Status Register Error Flag Clearing & Page Erase Sequence (`4d3e244`)**:
  - **Issue**: Lingering `PGERR` or `WRPRTERR` flags from prior power cycles blocked subsequent erase operations.
  - **Resolution**: Cleared all status flags prior to operation and followed ST programming manual sequence (`PER` set, `AR` address write, `STRT` assert, `BSY` poll, `PER` clear).
- **Watchdog APB1 Freeze HardFault (`b4deb69`)**:
  - **Issue**: Writing to `DBGMCU_APB1_FZ` during startup caused an immediate bus fault and continuous buzzer lockup because the DBGMCU clock was not enabled.
  - **Resolution**: Asserted `RCC_APB2ENR.DBGMCUEN` before setting `DBGMCU_APB1_FZ.DBG_IWDG_STOP`.
- **USB Terminal Corrupted Greeting Banner (`fc8c88f`)**:
  - **Issue**: Sending an unsolicited 160-byte greeting banner immediately upon USB bus configuration caused terminal emulators connected later (via `picocom /dev/ttyACM0`) to display partial leftover fragments (`================================`).
  - **Resolution**: Removed automatic enumeration banner; terminal connects silently and provides clean interactive CLI on demand.

### Firmware Footprint Verification
```text
   text    data     bss     dec     hex filename
  89432    1876    1128   92436   16914 flysky-i6x-rs
```
- **Application Flash (.text + .data)**: **91,308 bytes (~89.2 KB)** used out of **120 KB (122,880 bytes)** code partition (**>30.8 KB / 25.7% free headroom**).
- **Static RAM (.data + .bss)**: **3,004 bytes (~2.9 KB)** out of **16 KB (16,384 bytes)** total SRAM (**>81% free**, >6.3 KB stack margin).
- **Non-Volatile Storage**: **8,192 bytes** (Pages 60–63, `0x0801_E000 .. 0x0802_0000`).

---

## [0.16.0-rc.1] - 2026-09-21

### Added
- **Unified UI Subsystem (`src/ui/`)**:
  - `src/ui/widgets.rs`: Shared zero-allocation UI components (`draw_header`, `draw_footer`, `draw_footer_split`, `draw_bar_gauge`, `draw_channel_gauge`, `draw_progress_bar`, `draw_list_row`, `navigate_4slot_list`).
  - `src/ui/format.rs`: Shared zero-allocation stack formatters (`format_vbat`, `format_percent`, `format_throttle_percent`, `format_trim`, `u32_to_hex`, `u32_to_dec_5`, `u16_to_dec_4`, `i8_to_dec`, `u8_to_dec`).
  - `src/ui/dashboard/`: Modular 4-page live flight dashboard (`pages/gimbals.rs`, `pages/channels.rs`, `pages/model.rs`, `pages/telemetry.rs`, `status_bar.rs`).
  - `src/ui/menu/`: Modular settings menu decomposed into discrete screen controllers.
  - Standardized 8-pixel footer geometry ($y = 55$ divider, $y = 62$ `FONT_4X6` baseline) across all flight dashboards and settings screens, eliminating text clipping on 128px displays.
- **Rich Audio & Switch Chimes (PR #4)**:
  - Melodic welcome and goodbye chimes with selectable tone styles (`RICH` vs `SIMPLE`).
  - Per-model configurable arm switch assignments and armed/disarmed audio notifications.
- **ExpressLRS / CRSF Integration & Hardened Command Lifecycle**:
  - Native ELRS configuration engine with 8 parameter slots (`CRSF_FRAMETYPE_PARAMETER_READ` 0x2C / `PARAMETER_WRITE` 0x2D / `PARAMETER_ENTRY` 0x2B), labeled `ELRS Setup (Beta)`.
  - Robust non-blocking `ActiveCommandState` machine for actions (Wi-Fi, Bind, etc.) with automatic 250ms periodic `STATUS_POLL` transmission, watchdog timeouts, and interactive modal confirmation dialogs.
  - Unified extended parameter wire frame serialization via `build_param_ext_frame`.
  - Native CRSF telemetry sensor dashboard on Flight Page 3 (LQ, RSSI dBm, SNR, Antenna, Output Power, RF Rate, Battery Voltage, Capacity consumed).
  - Live JSON telemetry streaming over USB CDC Serial including CRSF telemetry fields.
  - Configurable external module power switch polarity (PC13 Active HIGH / Active LOW).

### Changed
- Slimmed `src/main.rs` by over 570 lines, delegating inline display rendering to `DashboardController`.
- Reduced Flash `.text` footprint by ~984 bytes via formatter and widget deduplication.

### Fixed
- **Critical Boot Stack Overflow & Reverse Throttle Fix (`dev-crsf`)**:
  - **Incident Summary**: When building the `dev-crsf` branch, the throttle stick was consistently inverted at boot (causing the "THROTTLE NOT AT IDLE!" startup alarm to require full stick-up to pass, channel 3 showing 2000 µs at idle, and Flight Page 1 showing 100% with stick down). Rerunning gimbal calibration completed successfully but did not resolve the inversion.
  - **Root Cause Analysis**:
    - During system boot, nested calls from `main()` -> `input::init()` -> `storage::load_config()` -> `storage::load_storage()` -> `RadioStorage::default_factory()` repeatedly allocated 2,688-byte `RadioStorage` instances by value on the stack.
    - Combined with `main()`'s 7.3 KB stack allocation, this pushed the call-chain stack depth to **15,428 bytes**—on an STM32F072 microcontroller with only **16,384 bytes (16 KB) of total SRAM**.
    - In commit `66a0b94`, the addition of `CONFIG_ENGINE` (896 bytes in `.data`) shifted static variables higher in memory, placing `THROTTLE_CALIB` at `0x2000_0a10`.
    - When the stack reached down to `0x2000_03dc` during `load_config()`, stack frames directly collided with and corrupted static RAM, specifically writing non-zero data into `0x2000_0a1a` (`THROTTLE_CALIB.invert = true`).
    - Because `apply_calibration()` only updated `min`, `center`, and `max` endpoints, `THROTTLE_CALIB.invert` remained permanently set to `true`, causing `normalize()` to negate -1000 to +1000.
  - **Resolution**:
    - **In-Place Storage Loading (`load_storage_into`)**: Eliminated pass-by-value stack instantiation of 2,688-byte `RadioStorage`. Flash data is read directly into caller-provided memory.
    - **Lightweight `load_config()`**: Refactored `storage::load_config()` to read only the 128-byte `RadioConfig` header directly from Flash without instantiating full 20-model `RadioStorage`.
    - **Lightweight `load_saved_rx_id()`**: Refactored AFHDS 2A receiver ID loading to read the 4-byte `rx_id` directly from Flash without stack allocations.
    - **BSS Relocation & Downsizing of `CONFIG_ENGINE`**: Relocated `CONFIG_ENGINE` from `.data` to `.bss` (setting `device_id` to 0 in const constructor and assigning `0xEE` on handshake start). Reduced `MAX_PARAMS` to 8 and optimized option buffers, reducing size from 952 bytes to 516 bytes and shrinking `.data` back to 1,776 bytes.
    - **Immutable Axis Inversion Flags**: Updated `input::apply_calibration()` to explicitly enforce `invert = false` on `THROTTLE_CALIB` and `YAW_CALIB`, and `invert = true` on `ROLL_CALIB` and `PITCH_CALIB`.
    - **Stack Safety Margin Restored**: Maximum boot stack usage dropped from **15.4 KB down to ~7.2 KB**, leaving **> 6.3 KB of guaranteed uncorrupted safety margin** between the stack and static RAM.
- **CRSF Protocol Framing & Sync Byte Compliance (`tbs-fpv/tbs-crsf-spec`)**:
  - **Serial Sync Byte Compliance (`0xC8`)**: Conformed all outgoing extended frames (`DEVICE_PING` 0x28, `PARAMETER_READ` 0x2C, `PARAMETER_WRITE` 0x2D) to start with `0xC8` (`CRSF_SYNC_BYTE`). Previously `build_ping_frame` emitted broadcast `0x00` on the wire and parameter frames emitted destination `0xEE`, causing external module serial parsers to discard packets.
  - **Frame Length Field Off-By-One Fix**: Corrected length field (`out_frame[1]`) from 5 to 6 in `build_param_read_frame()` and `build_param_write_frame()` to accurately encompass `Type (1) + Dest (1) + Origin (1) + Param (1) + Chunk/Val (1) + CRC (1) = 6` per TBS specification.
  - **Unbounded Null-Terminator Search**: Fixed premature loop exit when scanning device and parameter names with lengths $\ge$ 20 or $\ge$ 16 bytes, preventing memory offset miscalculations when parsing parameter counts, options lists, and current values.
  - **Multi-Chunk Parameter Retrieval & Rapid Pipeline**: Automatically request subsequent parameter chunks when `chunks_remain > 0`, and immediately dispatch parameter 1 request upon receiving device info `0x29`.
  - **ELRS Status Keep-Alive**: Extended telemetry parser to accept and keep the telemetry connection alive on incoming `0x2E` (`CRSF_FRAMETYPE_ELRS_STATUS`) packets.
---

## [0.15.1] - 2026-09-19

### Changed
- Bumped version and refreshed release artifacts.
- Clippy `-D warnings` cleanup across telemetry match guards.
- Cargo bare-metal harness configuration updates.

---

## [0.15.0] - 2026-09-19

### Added
- USB composite device mode (HID Joystick + CDC Serial simultaneous operation).
- Matrix mixer engine with 16 configurable mix rules.
- AFHDS 2A autonomous continuous telemetry stream and sensor parsing.
