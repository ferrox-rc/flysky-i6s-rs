# Implementation Roadmap & Phase Status

This document tracks the phased implementation, historical milestones, and ongoing development of `flysky-i6x-rs`.

---

## Current Firmware Footprint

- **Application Flash ROM**: **~110.3 KB** (`.text` 93,340 B + `.rodata` 8,844 B + `.data` 7,924 B + `.vector_table` 192 B = 110,300 B total) used out of **120 KB** partition (**>9.7 KB / 8.1% free headroom**).
- **Static RAM**: **~8.9 KB** (`.data` 7,924 B + `.bss` 1,008 B = 8,932 B total) out of **16 KB** available (**>44% SRAM free** with **>7.0 KB** guaranteed stack safety margin).
- **Non-Volatile Storage**: **8,192 bytes** (Pages 60–63) managed as an append-only log with automatic wear levelling.

---

## Phase Breakdown

### Phase 1: Board Bring-Up & Display (COMPLETED)
- [x] Dual MCU support for both `STM32F072VB` and `APM32F072VB` (UID & DFU mapping).
- [x] Parallel 8-bit ST7567 driver for `GPIOE` (ODR write) + control lines with `embedded-graphics`.
- [x] Screen orientation correction, column 4 offset, and factory backlight driver (`PF3`).
- [x] Fast power-on boot and reliable DFU bootloader invocation.

### Phase 2: Analog & Digital Inputs (COMPLETED)
- [x] Continuous 11-channel DMA1 ADC1 scanner (0.23 ms complete scan).
- [x] OpenTX Modified Moving Average (MMA) micro-jitter filter (0 latency on stick movement).
- [x] Exponential moving average (EMA) filter on battery voltage ADC (`PC0`) to stabilize hundredths digit.
- [x] Decode 2-pos / 3-pos switches (`SA..SD`), rotary pots (`VRA`, `VRB`), and battery voltage (`PC0`).
- [x] Correct physical Mode 2 channel mapping (`PA0` Roll, `PA1` Pitch, `PA2` Throttle, `PA3` Yaw).

### Phase 3: A7105 SPI Driver & Protocol Timing (COMPLETED)
- [x] Amiccom A7105 hardware SPI1 driver with antenna diversity TR switching (`PE10`/`PE11`/`PE12`).
- [x] Deterministic 16-channel FHSS hopping table generated from 96-bit silicon UID.
- [x] External HSE crystal (48.000 MHz) and calibrated `TIM16` timer (`PSC = 47`, `ARR = 3849`) for exact 3850.0 µs (259.74 Hz) frame sync.

### Phase 4: AFHDS 2A Over-the-Air Link & Telemetry (COMPLETED)
- [x] 14-channel 38-byte stick frame generation (1000..2000 µs).
- [x] 4-phase bidirectional bind sequence with persistent RX ID Flash storage.
- [x] Cancel / Abort binding mode via `[ESC]` (Cancel key) and support for one-way receivers (FS-A8S, Fli14).
- [x] Downlink telemetry reception window: live RSSI and RX battery voltage.

### Phase 5: Trims, Audio, & Calibration (COMPLETED)
- [x] Hardware PWM piezo buzzer driver on `PA8` (`TIM1_CH1`, 48 MHz / PSC 47) with distinct audio tones.
- [x] Digital trim controller with single-click, 90ms auto-repeat, audio feedback, and DFU lockout.
- [x] Throttle trim safety lock (Option 2) for flight controller arming protection.
- [x] Interactive 2-step gimbal & pot endpoint calibration wizard with Flash persistence.

### Phase 6: Settings Menu, Diagnostics, & Backlight Dimming (COMPLETED)
- [x] Hierarchical Settings Menu (`src/menu.rs`) navigated via `UP`, `DOWN`, `OK`, and `ESC`.
- [x] Radio Setup: Throttle Trim toggle (Option 2 safety lock), Beeper audio toggle, Backlight timeout (15s/30s/60s/Off), and Brightness level (10%..100%).
- [x] Hardware PWM backlight dimming driver on `PC9` (`TIM3_CH4`, 1 kHz PWM) supporting the popular backlight hardware mod while keeping stock `PF3` supported.
- [x] 14-channel live pulse width monitor with graphical bars and microsecond readouts (`1000..2000 µs`).
- [x] Real-time 12-bit Analog Diagnostics (`Diag Anas`) split into 2 graphic pages with 40px fill bars matching Channel Monitor.
- [x] System Information screen displaying MCU profile, 96-bit silicon UID, clock speed, and memory usage.

### Phase 7: 20-Model Memory & Smoothed Throttle Curves (COMPLETED)
- [x] 20 independent model memory slots (`M01`..`M20`), each allocated an exact 128-byte profile.
- [x] Multi-sector Flash driver across Pages 62 & 63 (`0x0801_F000`..`0x0801_FFFF`, 4 KB) with automatic legacy migration.
- [x] Model Match: independent `rx_id` per model profile with dynamic RF switching on model change.
- [x] Per-model digital trims (Roll, Pitch, Throttle, Yaw) and 14-channel reversing bitmask.
- [x] Switchable 5-point and 9-point throttle curves with optional **Catmull-Rom cubic Hermite spline** smoothing.
- [x] Real-time curve graph visualization (44 x 36 pixels) in the on-screen throttle curve editor.
- [x] Model setup: 10-character ASCII model name editor and aircraft type selector.

### Phase 8: Multi-Page Flight Dashboard & Navigation Polish (COMPLETED)
- [x] Pixel-perfect 4-page uniform flight dashboard (Gimbals, 14-CH Monitor, Model Dashboard, Telemetry Sensors) with shared top bar and small text footers.
- [x] Dedicated BIND button clean separation logic (tap = cycle flight pages, hold 1s = bind, boot hold = bind, menus = cursor advance).
- [x] Key auto-repeat for UP and DOWN navigation keys (300 ms hold threshold, 70 ms repeat interval).
- [x] Dynamic point range indicator (`Pts: 1..5` vs `Pts: 1..9`) in throttle curve editor.

### Phase 9: Safety First Subsystem (COMPLETED)
- [x] Transmitter low battery voltage alarm with configurable threshold in `Radio Setup` (4.0V–5.0V), flashing status bar display, and periodic audio alarm.
- [x] Pre-flight startup checks: detects raised throttle stick (> 5%) and unsafe switch states upon boot with audible alarm and RF throttle motor interlock.
- [x] Radio inactivity idle alarm: sounds periodic reminder chirps after 10 minutes without stick or key interaction.
- [x] Downlink telemetry RSSI range warnings: audible alerts when signal drops below 40% (Warning) and 20% (Critical).

### Phase 10: Flight Control & Mixing (COMPLETED)
- [x] Dual Rates & Exponential (D/R & EXPO) on Roll, Pitch, and Yaw with integer cubic curves and switchable high/low rates.
- [x] Auxiliary Channel Source Mapping: remapping any physical switch or potentiometer to any output channel (CH5–CH14).
- [x] Pre-configured aircraft templates: Elevon / Delta Wing, V-Tail, and Flaperon (dual ailerons with auxiliary flap input).
- [x] EdgeTX / OpenTX-style 14-channel freeform matrix mixer with 8 user-configurable mix lines (Weight, Offset, Switch, ADD/MULT/REPLACE modes).
- [x] Dedicated UI editors in Settings Menu: `Dual Rate/Expo`, `Wing/Mixer` (with mix line editor), and `Aux Channels`.
- [x] Real-time flight control loop decoupled from display flush via hardware Cortex-M SysTick timer (`src/time.rs`).
- [x] High-rate channel updates (sub-30 µs execution at kHz pass rates) with 30 Hz display throttling.
- [x] Lightweight 4-sample stick filter with dynamic deadband bypass (`diff >= 6`) for instantaneous step-response.
- [x] Elimination of 64-bit software division emulation (`__aeabi_ldivmod`) across all mixing and expo math.
- [x] Automated Flash storage sanitization (`RadioStorage::sanitize`) enforcing valid operating limits.

### Phase 12: Code Review Hardening & Radio Link Safety (COMPLETED)
- [x] Bounded SPI1 and A7105 hardware wait loops with loop counters to prevent CPU lockup.
- [x] Re-ordered A7105 FIFO writes to force `STANDBY` before loading payload, preventing FIFO pointer corruption.
- [x] Autonomous failsafe frame broadcast every 1,569 packets (~6.0s) ensuring receiver failsafe synchronization.
- [x] Front-end LNA saturation mitigation during binding (`RF_MODE_OFF`).
- [x] Collision iteration bounds on pseudo-random frequency hopping table generation.
- [x] Protected Flash page erase and halfword programming with critical sections in `src/storage.rs`.

### Phase 13: USB Subsystem & Protocol Engine (COMPLETED)
- [x] Hardware USB Full-Speed (12 Mbps) peripheral on `PA11` / `PA12` with internal 1.5 kΩ pull-up resistor.
- [x] Native USB Joystick class (HID) at 100 Hz with OpenI6X/EdgeTX mapping (8 axes: X/Y/Z/Rz/Rx/Ry/Sliders, 16 buttons with 0-state neutral release to eliminate Linux ghost typing).
- [x] Virtual COM Port (CDC-ACM) streaming universal JSON Lines (`ndjson`) telemetry at 20 Hz and interactive CLI commands (`help`, `status`, `channels`, `telem`, `reboot`).
- [x] True on-the-fly USB mode switching in `Radio Setup` (SE0 disconnect pulse + hardware APB1 reset without requiring reboot). Default mode: `OFF`.
- [x] Silent RF standby running in Joystick mode: A7105 transceiver and PA/LNA frontend are placed in standby (zero RF radiation, cool running) with `U:SIM` status indicator.
- [x] Non-volatile `USB Mode` setting in `Radio Setup` (`OFF`, `JOYSTICK`, `SERIAL`, `COMPOSITE`).
- [x] Main Menu Item 9 repurposed as `Protocol Setup` supporting `AFHDS 2A` internal RF and external `CRSF / ELRS` transmitter modules.

### Phase 14: CRSF / ExpressLRS & Memory Hardening (COMPLETED)
- [x] Full Crossfire (CRSF) & ExpressLRS driver on `USART2` (`PD5` TX, `PA15` RX) with selectable baud rates (420k, 416.6k, 115.2k, 921.6k).
- [x] Configurable external module power switch polarity on `PC13` (`Radio Setup` -> `Ext Module Power: HIGH/LOW`).
- [x] Native on-radio ExpressLRS configurator engine (0 heap allocations, reads/writes parameters and runs module commands).
- [x] Native CRSF link diagnostics screen on Flight Page 4 (LQ, RSSI dBm, SNR dB, Active Antenna, TX Power mW, RF Rate, Battery Voltage, Capacity).
- [x] Universal JSON telemetry streaming over USB CDC including full CRSF downlink telemetry metrics.
- [x] Zero-alloc in-place Flash loading (`load_storage_into`) and lightweight header reads (`load_config`), cutting boot stack depth in half (from 15.4 KB to ~7.2 KB) and ensuring > 6.3 KB safety margin in SRAM.

### Phase 15: Hardware Watchdog, Append-Only Storage, & Pure PAC Driver (COMPLETED)
- [x] Pure PAC register-level driver layer replacing HAL overhead (`stm32f0::stm32f0x2::pac`).
- [x] Independent Hardware Watchdog (`pac::IWDG`) with 2.0s timeout, LSI clock stabilization, bounded register sync, and `DBGMCU_APB1_FZ` debugger halt freezing.
- [x] In-Flight Watchdog Reset Recovery: Detects `RCC_CSR` watchdog flags (`IWDGRSTF`/`WWDGRSTF`), bypassing power-on safety traps (throttle/switch interlocks) and DFU settling delays to restore active RF control in < 2 ms with an acoustic pilot alert.
- [x] 4-page (8 KB, Pages 60–63 at `0x0801_E000`..`0x0802_0000`) log-structured append-only storage engine with `sequential-storage`.
- [x] Sub-3ms (~2.8 ms) non-blocking saves with zero page erases on model and setting updates.
- [x] Automatic multi-tier migration from legacy v1/v2/v3 snapshot layouts to sequential storage.
- [x] Linker script memory layout update (`FLASH (rx)` length = 120 KB, Pages 0–59).
- [x] USB Composite mode (Joystick + CDC Serial) and clean silent interactive CLI on connection.

### Phase 16: CRSF Protocol Compliance & Automated Testing (COMPLETED)
- [x] TBS Crossfire Rev 08 11-bit channel scaling formula adherence across all 16 channels.
- [x] Standardize radio pulse width range to FlySky standard 988..2012 µs (center 1500 µs, span 1024 µs).
- [x] Multi-frame chunk reassembly for ExpressLRS configuration parameters.
- [x] Expand parameter slots to 16 items and string buffers to 48 bytes.
- [x] CRSF protocol specification and manual bench verification guide (`docs/CRSF_PROTOCOL_SPEC.md`).
- [x] Dual-target host unit test harness (`cargo test-host`) with 36 automated unit tests across curves, trims, mixer, and protocol state machines (`docs/TESTING.md`).

### Phase 17: Modern UI Glyphs & Visual Experience (COMPLETED)
- [x] Ferrox-RC startup splash screen with 28x27 kinetic delta emblem, firmware identifier, and version display (1200 ms).
- [x] Synchronized power-on welcome fanfare (4-note ascending chime $C_6 \to E_6 \to G_6 \to C_7$, 660 ms) with monotonic cadence independence.
- [x] 12x12 Material Design vector icon glyphs for model types (Airplane, Heli, Quad, Glider, General) and system features.
- [x] 3-slot graphical list navigation (14px row height) with smooth viewport scrolling and return position preservation.
- [x] Right-edge proportional scrollbar widget with position indicator (`1/13`).
- [x] 5-page flight dashboard with 18-channel split monitor (CH 1..10 on P2/5, CH 11..18 on P3/5).
- [x] Dynamic horizontal battery gauge with live 3-stage charge bars (fills right-to-left from base, empties left-to-right from tip).
- [x] Dual split horizontal potentiometer bar for VRa (top) and VRb (bottom) with center ticks on P1/5.
- [x] Mode 2 right-aligned throttle bar and physical switch position arrow glyphs (`^`/`-`/`v`).
- [x] `ModelType::General` support for boats, rovers, and robotics surface craft.

### Phase 18: Voice Audio Subsystem & Hardware Mod (BRANCH: `feat/dfplayer-voice-audio`)
- [x] DFPlayer Mini hardware serial audio driver on `USART` / dedicated pin.
- [x] Spoken telemetry announcements (battery voltage, low RSSI, timer elapsed).
- [x] Audible switch position announcements and flight mode voice prompts.

### Phase 19: Flight Timer, Mixer Polish, & Pilot Ergonomics (COMPLETED)
- [x] EdgeTX-parity Flight Countdown / Stopwatch timer with multi-trigger modes (`THs (RUN)`, `THt (LTCH)`, `ALWAYS ON`, and switch triggers `SA^`..`SDv`).
- [x] Auto-reset upon arming and freeze upon disarming when Arm Switch is assigned.
- [x] Visual HUD Hold-to-Reset progress bar (holding `[CANCEL]` for 1.0s with animated bar and confirmation toast).
- [x] Dedicated `[UP]` / `[DOWN]` page navigation on flight dashboard.
- [x] Acoustic countdown beeps (1-min warning, 10s..1s countdown, persistent tone at 0) and formatted flight dashboard display.
- [x] Elevon & V-Tail saturation resolution (standardizing `(p ± r) / 2` throw limits).
- [x] Wing/Tail differential throw activation utilizing existing `template_diff` parameter.
- [x] Unipolar / half-range mixer source option (`Thr+`) to eliminate negative pitch-down at idle on compensation mixes.
- [x] Physical switch auto-detection in menu editors (toggling any physical switch auto-selects its condition).
- [x] Model Duplicate / Copy utility in `MODEL SETUP` for safe mixer experimentation.
- [x] Non-visual potentiometer center acoustic detent click when crossing neutral center on `VRA` and `VRB`.
- [x] 18-channel i-BUS & AFHDS 2A over-the-air encoding with `ModelConfig` v5 and automatic v4 flash migration.
- [x] CRSF Parameter 0 root-folder query and child-ID selective discovery.
- [x] CRSF dynamic command timeout polling, live command info text (`[Binding]`, `[OK]`, `[Failed]`), and clamped info/string parameter rendering.
- [x] CRSF integer parameter support (`UINT8`, `INT8`, `UINT16`, `INT16`) with in-place modal editing, unit display, and multi-byte writes.
- [x] CRSF nested subfolder title retention stack preserving parent names up to 6 levels deep.
- [x] Unified configurator buffers: `MAX_PARAMS = 48` per folder, `MAX_PARAM_MAP = 96`, and `STRING_POOL_SIZE = 1280` bytes.

### Phase 20: Trainer Port Subsystem & PPM In/Out (PLANNED / BRANCH: `feat/trainer-ppm`)
- [ ] Direct PAC driver for `TIM15` (1 µs tick resolution at 48 MHz).
- [ ] PPM Output on `PF10` (`TIM15_CH2`): Deterministic 22.5 ms 8-channel CPPM stream powering the iRangeX iRX6 multi-protocol module and USB simulator dongles.
- [ ] PPM Input on `PF9` (`TIM15_CH1`): Input capture decoder for wired buddy-box training and FPV head-trackers.
- [ ] Safety Handover Mixer: Instant instructor takeover threshold (> 5% stick deflection) and sub-50 ms failsafe timeout fallback.

### Phase 21: Wireless SBUS Trainer & Setup Suite (PLANNED / BRANCH: `feat/trainer-sbus`)
- [ ] `USART2` hardware-inverted (`RXINV`) SBUS receiver decoder (100k baud 8E2) for wireless buddy-box links.
- [ ] Dedicated on-radio `Trainer Setup` screen with live student-vs-instructor graphic monitor bars and link state diagnostics.
- [ ] Auxiliary channel rate-limiter ("Servo Slow") for realistic flap deployment and gear doors without aerodynamic ballooning.

### Phase 22: Hardware Extension Suite — SE/SF Switches & P7 Header ADC (COMPLETED / BRANCH: `feat/hardware-extensions-se-sf-p7`)
- [x] Auxiliary digital switches `SE` on `PC12` and `SF` on `PC15` with internal pull-ups (`PUPDR = 01`) for 2-position toggle mods.
- [x] Auxiliary ADC inputs `AD12`–`AD15` on `PC2`–`PC5` broken out on unpopulated `P7` header for up to 4 extra potentiometers, sliders, or 6-pos switches (`VRC`, `VRD`, `VRE`, `VRF`).
- [x] Autonomous 15-channel DMA1 scanning mode in `src/adc.rs` (`ADC1_CHSELR = 0xF7FF`) with zero CPU overhead.
- [x] Non-volatile hardware extension toggles in `Radio Setup` (`Ext Switches: [OFF/PC12+PC15]`, `P7 Header: [OFF/AD12-AD15]`) utilizing existing reserved bytes in `RadioConfig`.
- [x] Real-time diagnostic visualization of `PC12`/`PC15` pin logic and `AD12`–`AD15` raw ADC voltages in `Analog Diag` (Pages 3 & 4).
- [x] Full matrix mixer and auxiliary channel assignment (`CH5`–`CH18`) support for `SE`, `SF`, `VRC`, `VRD`, `VRE`, `VRF`.
- [x] Dynamic 6-switch dashboard display on gimbals flight screen (`SA`–`SF`).
- [x] Native USB Gamepad mapping of `SE` and `SF` to discrete buttons 11 and 12 for simulator use.
