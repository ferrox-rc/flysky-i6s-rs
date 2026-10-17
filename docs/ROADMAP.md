# Implementation Roadmap & Phase Status

This document tracks the phased implementation, historical milestones, and ongoing development of `flysky-i6s-rs`.

---

## Current Firmware Footprint

- **Firmware Footprint (v0.2.0)**: **~113.6 KB Flash ROM** (`.text` 108,248B + `.data` 8,060B = 116,308B, leaving ~6.4 KB / ~5.4% headroom out of 120 KB partition) and **~8.9 KB static RAM** (`.data` 8,060B + `.bss` 1,056B = 9,116B, leaving >7.0 KB stack safety margin in 16 KB SRAM).
- **Authoritative Budget & Breakdown**: For exact section sizes, partition layouts, and stack guarantees, see [ARCHITECTURE.md (Memory Footprint)](ARCHITECTURE.md#4-memory-footprint).

---

## Phase Breakdown

### Phase 1: Board Bring-Up, Power Latch, & Touchscreen (COMPLETED)
- [x] Dual MCU support for both `STM32F072VB` and `APM32F072VB` (UID & DFU mapping).
- [x] Electronic power latch (`PB15`), soft power hold-to-shutdown (`PB14` >= 1.5s), and dual blue LEDs (`PD10`/`PD11`).
- [x] FocalTech FT6236 capacitive touchscreen driver over hardware `I2C1` (`PB8`/`PB9` @ 400 kHz, `PA15` RST, `PC12` INT).
- [x] Parallel 8-bit ST7567 driver for `GPIOE` (ODR write) + control lines with in-tree 1bpp bitmap fonts.
- [x] Zero-disassembly DFU bootloader entry via front buttons (`PA9` + `PA10`), `SYSTEM INFO` menu, and USB CLI (`dfu`).

### Phase 2: Analog & Digital Inputs (COMPLETED)
- [x] Continuous 11/15-channel DMA1 ADC1 scanner (0.23 ms complete scan).
- [x] OpenTX Modified Moving Average (MMA) micro-jitter filter (0 latency on stick movement).
- [x] Exponential moving average (EMA) filter on battery voltage ADC (`PC0`, $10\text{ k}\Omega / 5.1\text{ k}\Omega$ divider).
- [x] Decode switches (`SA..SD`), rotary pots (`VRA`, `VRB`), extended analog (`PC1..PC4`), and J4 tactile buttons (`PA13`/`PA14`).
- [x] Configurable Stick Modes 1, 2, 3, and 4 with decoupled logical controls (`aileron`, `elevator`, `throttle`, `rudder`).

### Phase 3: A7105 SPI Driver & Protocol Timing (COMPLETED)
- [x] Amiccom A7105 hardware SPI1 driver with antenna diversity TR switching (`PE10`/`PE11`/`PE12`).
- [x] Deterministic 16-channel FHSS hopping table generated from 96-bit silicon UID.
- [x] External HSE crystal (48.000 MHz) and calibrated `TIM16` timer (`PSC = 47`, `ARR = 3849`) for exact 3850.0 µs (259.74 Hz) frame sync.

### Phase 4: AFHDS 2A Over-the-Air Link & Telemetry (COMPLETED)
- [x] 18-channel interleaved 38-byte stick frame generation (988..2012 µs).
- [x] 4-phase bidirectional bind sequence with persistent RX ID Flash storage.
- [x] Cancel / Abort binding mode via `[ESC]` and support for one-way receivers (FS-A8S, Fli14).
- [x] Downlink telemetry reception window: live RSSI and RX battery voltage.

### Phase 5: Trims, Audio, & Calibration (COMPLETED)
- [x] Hardware PWM piezo buzzer driver on `PA8` (`TIM1_CH1`, 48 MHz / PSC 47) with `Rich` and `Simple` audio profiles.
- [x] Dual digital trim modes: Stick Modifier Mode (`PA9`/`PA10` + gimbals) and Virtual Touch Perimeter Targets.
- [x] Complete 3-Axis Instant Trim support (Roll, Pitch, Yaw) with neutral-stick reset.
- [x] Throttle trim safety modes (`OFF (Lock)`, `IDLE`, `LINEAR`).
- [x] Interactive adaptive 2-step gimbal & multi-pot endpoint calibration wizard with Flash persistence.

### Phase 6: Settings Menu, Diagnostics, & Backlight Dimming (COMPLETED)
- [x] Hierarchical Settings Menu (`src/ui/menu/`) navigated via touch gestures, tap hitboxes, and front buttons.
- [x] Hardware PWM backlight dimming driver on `PC9` (`TIM3_CH4`, 1 kHz PWM) while keeping stock `PF3` supported.
- [x] 18-channel live pulse width monitor with dynamic channel labels and microsecond readouts.
- [x] Real-time 12-bit Analog Diagnostics (`Diag Anas`) across 4 graphic pages (`ANALOG (1-6)`, `ANALOG (7-11)`, `SWITCHES (E/F/G/H)`, `EXT ANALOG (PC1-4)`).
- [x] System Information screen displaying MCU profile, 96-bit silicon UID, git commit hash, and one-tap DFU reboot.

### Phase 7: 20-Model Memory & Smoothed Throttle Curves (COMPLETED)
- [x] 20 independent model memory slots (`M01`..`M20`), each allocated an exact 128-byte profile.
- [x] Model Match: independent `rx_id` per model profile with dynamic RF switching on model change.
- [x] Per-model digital trims (Roll, Pitch, Throttle, Yaw) and 18-channel reversing bitmask.
- [x] Switchable 5-point and 9-point throttle curves with optional **Catmull-Rom cubic Hermite spline** smoothing.
- [x] Real-time curve graph visualization in the on-screen throttle curve editor.

### Phase 8: Multi-Page Flight Dashboard & Navigation Polish (COMPLETED)
- [x] Pixel-perfect 5-page uniform flight dashboard (Gimbals, CH 1..10 Monitor, CH 11..18 Monitor, Model Dashboard, Custom Telemetry).
- [x] Key auto-repeat for UP and DOWN navigation (300 ms hold threshold, 70 ms repeat interval).
- [x] Non-overlapping adaptive switch and potentiometer bar on Page 1/5 supporting up to 12 switches and 10 pots.

### Phase 9: Safety First Subsystem (COMPLETED)
- [x] Transmitter low battery voltage alarm with configurable threshold in `Radio Setup` (4.0V–5.0V).
- [x] Model-specific pre-flight startup checks across Stick Modes 1–4 (Airplane/Glider/Quad idle, Heli Throttle Hold, General centered throttle).
- [x] Radio inactivity idle alarm after 10 minutes without stick, touch, or key interaction.
- [x] Downlink telemetry RSSI range warnings: audible alerts below 40% (Warning) and 20% (Critical).

### Phase 10: Flight Control & 18-Channel Mixing (COMPLETED)
- [x] Dual Rates & Exponential (D/R & EXPO) on Roll, Pitch, and Yaw with integer cubic curves and switchable high/low rates.
- [x] Auxiliary Channel Source Mapping (`CH5`–`CH18`) across all sticks, pots (`VRA..VRF`), and switches (`SA..SH`, `SWI..SWL`).
- [x] Pre-configured aircraft templates: Elevon / Delta Wing, V-Tail, and Flaperon (with differential throw support).
- [x] EdgeTX / OpenTX-style 18-channel freeform matrix mixer with 8 user-configurable mix lines (Weight, Offset, Switch, ADD/MULT/REPLACE modes).
- [x] Real-time flight control loop decoupled from display flush via hardware Cortex-M SysTick timer (`src/time.rs`).

### Phase 12: Code Review Hardening & Radio Link Safety (COMPLETED)
- [x] Bounded SPI1, I2C1, and A7105 hardware wait loops with loop counters to prevent CPU lockup.
- [x] Gated RF frame broadcast until mixer publishes first valid packet; staleness watchdog fallback (~115 ms) to `PACKET_FAILSAFE`.
- [x] Autonomous failsafe frame broadcast every 1,569 packets (~6.0s).
- [x] In-flight Flash write protection while armed (`!pipeline.prev_armed`).

### Phase 13: USB Subsystem & Protocol Engine (COMPLETED)
- [x] Hardware USB Full-Speed (12 Mbps) peripheral on `PA11` / `PA12` with internal 1.5 kΩ pull-up resistor.
- [x] Native USB Joystick class (HID) at 100 Hz with OpenI6X/EdgeTX mapping (8 axes, 16 buttons).
- [x] Virtual COM Port (CDC-ACM) streaming universal JSON Lines (`ndjson`) telemetry and interactive CLI (`help`, `status`, `channels`, `telem`, `stream`, `dfu`, `reboot`).
- [x] Decoupled stack-safe on-the-fly USB mode switching in `Radio Setup` (`OFF`, `JOYSTICK`, `SERIAL`, `COMPOSITE`).
- [x] Silent RF & PPM standby in Joystick mode (`U:SIM` status indicator).

### Phase 14: CRSF / ExpressLRS & FS-i6S Extension Bay (COMPLETED)
- [x] Full Crossfire (CRSF) & ExpressLRS driver on `USART1` (`PB6` TX, `PB7` RX on `J15`) with 5 selectable baud rates (`115.2k`, `416.6k`, `420k`, `921.6k`, `1.875M`) and full/half-duplex modes (`HDSEL` with self-echo suppression).
- [x] Configurable external module power switch polarity on `PF6` (header `H1`: `HIGH (N)` / `LOW (P)`).
- [x] Native on-radio ExpressLRS/CRSF configurator engine (16-device discovery, Parameter 0 root query, 6-level folder stack, integer & select editing).

### Phase 15: Hardware Watchdog, Append-Only Storage, & Pure PAC Driver (COMPLETED)
- [x] Pure PAC register-level driver layer (`stm32f0xx_hal::pac`).
- [x] Independent Hardware Watchdog (`pac::IWDG`) with 2.0s timeout, LSI clock stabilization, bounded register sync, and `DBGMCU_APB1_FZ` debugger halt freezing.
- [x] In-Flight Watchdog Reset Recovery (`RCC_CSR`) restoring active RF control in < 2 ms.
- [x] 4-page (8 KB, Pages 60–63 at `0x0801_E000`..`0x0802_0000`) log-structured append-only storage engine with `sequential-storage`.

### Phase 16–19: Protocol Compliance, In-Tree Fonts, Flight Timers & Ergonomics (COMPLETED)
- [x] TBS Crossfire Rev 08 11-bit channel scaling formula adherence (`988..2012 µs` <-> `172..1811`).
- [x] Dual-target host unit test harness (`cargo test-host`) with 140 automated unit tests (`docs/TESTING.md`).
- [x] In-tree bare-metal 1bpp glyph lookup tables (`FONT_4X6_DATA` and `FONT_6X10_DATA`), removing runtime `embedded-graphics` font overhead.
- [x] EdgeTX-parity Flight Countdown / Stopwatch timer with multi-trigger modes, arm-switch integration, and Hold-to-Reset HUD progress bar.
- [x] Unipolar throttle mixer source (`Thr+`) and Model Duplicate / Copy utility.

### Phase 20: Trainer Port Subsystem & PPM In/Out (COMPLETED)
- [x] Direct PAC driver for `TIM15` (`src/trainer/`).
- [x] PPM Output on `PF10` (`TIM15_CH2`): Deterministic 22.5 ms 8-channel CPPM stream on `J15`.
- [x] PPM Input on `PF9` (`TIM15_CH1`): 0.5 µs input capture decoder for wired buddy-box training and FPV head-trackers.
- [x] Safety Handover Mixer (`src/trainer/handover.rs`): Student control when `SD` is Down, instant instructor stick-override takeover (> 5% deflection), and 50 ms signal-loss failsafe fallback.

### Phase 22: Universal Input Modes & Extended Hardware Suite (COMPLETED)
- [x] Universal configurable ADC input modes (`DEFAULT`, `2-POS`, `3-POS`, `6-POS`, `POT`, `POT-D`, `INST-TRIM`) for all 10 auxiliary channels (`SA..SD`, `VRA..VRB`, `VRC..VRF`).
- [x] Extended ADC inputs on `PC1`–`PC4` (`ADC_IN11`–`ADC_IN14`) with autonomous 15-channel DMA1 scanning (`0x7FFF`).
- [x] J4 SWD header switch expansion (`PA13` SG / `PA14` SH) and configurable rear tactile button functions (`SWE`/`SWF`, `TIMER RESET`, `INSTANT TRIM`, `TRIMS MOD`).

### Phase 23: Configurable Telemetry Dashboard & RF Power Management (COMPLETED)
- [x] Configurable 4-tile telemetry dashboard on Flight Page 5/5 (`CUSTOM TELEM`) with 2×2 quadrant layout.
- [x] Dedicated `TELEM DASH SETUP` screen under `Radio Setup` supporting 13 selectable sensors (including live `TIMER`).
- [x] Configurable AFHDS 2A transmitter RF output power (`100mW (HI)`, `10mW (MID)`, `1mW (LOW)`, `100uW (TST)`).
- [x] Automatic deep sleep power-down (`0x0F`) for the internal A7105 transceiver when operating in CRSF mode.

### Phase 24: Unified `ferrox` Core & Hardware Abstraction Layer (COMPLETED)
- [x] Unified crate name (`ferrox`) and 100% byte-for-byte identical core shared between `flysky-i6s-rs` and `flysky-i6x-rs`.
- [x] Strictly isolated board-specific hardware drivers inside `src/hw/` (`mod.rs`, `adc.rs`, `keys.rs`, `uart.rs`, `power.rs`, `touch/`) with zero dead-code stubs.
