# HARDWARE REFERENCE & BRING-UP NOTES

Technical reference documentation for the FlySky FS-i6S hardware. This document details the pinout, touchscreen interface, electronic power management, register mappings, and system architecture for the FS-i6S platform.

---

## 1. Microcontroller & Memory

- **Primary MCU:** STMicroelectronics **STM32F072VB** (Cortex-M0 @ 48 MHz).
- **Secondary Clone Variant:** Geehy **APM32F072VB** (pin- and register-compatible clone).
- **Memory Map:**
  - **Flash:** 128 KB (`0x0800_0000 .. 0x0801_FFFF`, 64 pages × 2048 bytes)
    - **Application Firmware:** 120 KB (`0x0800_0000 .. 0x0801_DFFF`, Pages 0–59)
    - **Non-Volatile Storage:** 8 KB (`0x0801_E000 .. 0x0801_FFFF`, Pages 60–63, 4-page append-only log)
  - **SRAM:** 16 KB (`0x2000_0000 .. 0x2000_3FFF`)

### Dual-MCU Silicon Details

| Parameter | STM32F072VB | APM32F072VB |
| :--- | :--- | :--- |
| **96-bit Silicon UID** | `0x1FFFF7AC` | `0x1FFFF7E8` |
| **System Memory ROM (DFU)** | `0x1FFFC800` | `0x1FFFF000` / `0x1FFFC400` |
| **USB DFU VID:PID** | `0483:df11` | `314b:0106` |
| **Flash Page Size** | 2048 bytes (64 pages) | 2048 bytes |

---

## 2. Power Management, Touchscreen, & Front Buttons

The FlySky FS-i6S eliminates the mechanical 3×4 key matrix of older transmitters, introducing an electronic power management latch, a FocalTech FT6236 capacitive multi-touchscreen, and dual front tactile push-buttons.

### Power Management & Soft Shutdown Circuitry

- **Electronic Power Latch (`PB15`):**
  - Configured as GPIO Output Push-Pull (`GPIO_Mode_OUT`).
  - **Critical Initialization:** Must be driven HIGH (`PB15 = 1`) as the absolute first instruction in `main()` (`power::init()`). If `PB15` is not driven HIGH immediately after the user releases the physical power button, the hardware power rail collapses and the radio shuts down instantly.
  - **DFU Bootloader Persistence:** When transitioning to the factory ROM DFU bootloader, `rcc_deinit()` preserves the `GPIOB` clock gate (`RCC_AHBENR = 0x0004_0014`), and `power::latch_on()` is called before branching to System ROM so the unit remains powered over USB.
- **Power Button Sense (`PB14`):**
  - Configured as GPIO Input with internal pull-up (`GPIO_PuPd_UP`). Active LOW when the front power buttons are pressed.
  - **Debounced Boot Guard:** The `PowerManager` requires the pilot to release the power button after initial power-on before arming the shutdown counter, preventing accidental immediate shutdown.
  - **Hold-to-Shutdown (>= 1.5s):** Holding the power buttons for >= 1.5 seconds triggers an on-screen animated shutdown modal, safely flushes any unwritten Flash configuration to non-volatile storage, and de-asserts `PB15` (`PB15 = 0`) to cleanly cut battery power.
- **Dual Blue Power LEDs (`PD10`, `PD11`):**
  - Configured as GPIO Output Push-Pull (`GPIO_Mode_OUT`).
  - Driven HIGH to illuminate the dual blue LED rings integrated into the front power buttons.

### Touchscreen Subsystem (FocalTech FT6236)

- **Controller:** FocalTech **FT6236** Capacitive Touch Screen Controller.
- **Bus Interface:** Hardware **I2C1** operating in Fast Mode at **400 kHz**:
  - **`PB8`**: `I2C1_SCL` (Alternate Function 1, Open-Drain with external pull-up).
  - **`PB9`**: `I2C1_SDA` (Alternate Function 1, Open-Drain with external pull-up).
  - **`PA15`**: Hardware Reset Line (`TOUCH_RST`, Active LOW, 5 ms low pulse on startup).
  - **`PC12`**: Touch Interrupt Line (`TOUCH_INT`, Active LOW). Signals when fresh touch coordinates or gestures are available.
- **Protocol & Coordinates:**
  - I2C Device Address: `0x38` (7-bit address).
  - A 7-byte burst read from register `0x00` extracts:
    - Byte 1: Gesture ID (`0x10` Swipe Up, `0x14` Swipe Right, `0x18` Swipe Down, `0x1C` Swipe Left).
    - Byte 2: Touch point count (`TD_STATUS`, 0..2).
    - Bytes 3..4: $X$ coordinate (12-bit, MSB + LSB) and Event Flag (0 = Press Down, 1 = Lift Up, 2 = Contact, 3 = No Event).
    - Bytes 5..6: $Y$ coordinate (12-bit, MSB + LSB).
- **Coordinate Transformation & 90° Axis Swap:**
  - The physical touch sensor has a native resolution of 320×320 and is mounted rotated relative to the ST7567 128×64 LCD:
    $$ x_{lcd} = \left( \text{raw}_y \times 128 \right) / 320 $$
    $$ y_{lcd} = 63 - \left( \text{raw}_x \times 64 \right) / 320 $$
  - Both coordinates are clamped to $x \in [0, 127]$ and $y \in [0, 63]$.

### Front Tactile Buttons & Digital Trims

The front dual tactile buttons (integrated into the power button assemblies with blue LEDs) provide hardware navigation and in-flight trimming:
- **`PA9`**: Left Front Tactile Button (Active LOW, internal pull-up).
  - In Menus / Flight: Functions as **Cancel / Back / Exit**.
  - Digital Trim Mode: Left Stick Trim Modifier.
- **`PA10`**: Right Front Tactile Button (Active LOW, internal pull-up).
  - In Menus / Flight: Functions as **OK / Select / Confirm**.
  - Digital Trim Mode: Right Stick Trim Modifier.
- **Zero-Disassembly DFU Bootloader Combo:**
  - Holding **`PA9` + `PA10`** simultaneously while pressing the power button triggers the zero-disassembly hardware cold-boot jump into the factory ROM DFU bootloader (`0x1FFF_C800`).
- **Digital Trim Architecture (Replacing Mechanical Rockers):**
  1. **Stick Modifier Mode:**
     - Holding **Left Front (`PA9`)** + Left Stick deflection: Vertical adjusts Throttle Trim; Horizontal adjusts Yaw Trim.
     - Holding **Right Front (`PA10`)** + Right Stick deflection: Vertical adjusts Pitch Trim; Horizontal adjusts Roll Trim.
     - Stick deflection threshold: $> 350$ counts from center.
  2. **Virtual Touch Hitboxes:**
     - Left border ($x < 16$): Throttle Trim Up / Down.
     - Right border ($x > 112$): Pitch Trim Up / Down.
     - Bottom border ($y > 52$): Yaw Trim Left/Right and Roll Trim Left/Right.

---

## 3. ST7567 128×64 Parallel LCD Display

The transmitter uses a Sitronix **ST7567** (or compatible) monochrome LCD controller connected via an 8-bit parallel bus operating in 6800-series mode.

### Pinout & Signals

| Signal | MCU Pin | Function & Idle State |
| :--- | :--- | :--- |
| **D0 .. D7** | `PE0 .. PE7` | Full-byte parallel data bus written via `GPIOE->ODR[7:0]` |
| **RS** | `PB3` | Command / Data select: Low = Command, High = Graphic data |
| **RST** | `PB4` | Active Low hardware reset (pulse Low for >= 20 µs) |
| **RW** | `PB5` | Read / Write select: Kept **LOW** for write mode |
| **CS** | `PD2` | Chip Select: Kept **LOW** to permanently enable the display |
| **RD / E** | `PD7` | 6800-series latch strobe: Data is latched on **High -> Low** transition |

### Controller Dimensions & Column Offset

The ST7567 controller contains 132 column segment drivers, while the FS-i6S physical LCD panel is 128 pixels wide.
- Active display starts at **Column 4** (`col_start = 0x04`).
- Pages: 8 vertical pages (8 * 8 = 64 rows), each byte containing 8 vertical pixels (LSB at top).
- Total SRAM framebuffer size: 128 * 8 = 1024 bytes.

### Initialization Sequence

```
0xE2 -> Software Reset
0xAE -> Display OFF
0xA4 -> Normal RAM display mode
0xA3 -> Bias Select 1/7
0xC0 -> COM Scan Normal (COM0 -> COM63)
0xA1 -> SEG Scan Inverse (SEG131 -> SEG0) [Corrects 180° inversion]
0x2F -> Power Control: Booster, Regulator & Follower all ON
0x23 -> V0 Internal Resistor Ratio (011)
0x81 -> Electronic Volume Mode Set (Contrast)
0x25 -> Contrast Level (0x00 .. 0x3F)
0x40 -> Display Start Line 0
0xB0 -> Page Address 0
0x04 -> Column Address Low Nibble = 4 (Centers display)
0x10 -> Column Address High Nibble = 0
0xAF -> Display ON
```

---

## 4. Backlight Circuitry

### Stock Configuration (Unmodded)
- **Control Pin:** **`GPIOF` Pin 3 (`PF3`)**
- **Polarity:** **Active HIGH** (`PF3 = 3.3V` turns the backlight ON).
- **Circuit:** `PF3` drives the base of an NPN switching transistor through a series resistor. The transistor's collector pulls the LED cathode string to ground.
- **Dimming:** Digital ON/OFF only. `PF3` does not support hardware timer PWM.

### Optional Hardware PWM Mod (Dimming)
- **Control Pin:** **`GPIOC` Pin 9 (`PC9`)**
- **Circuit:** Solder jumper added from the unpopulated `PC9` pad to the backlight transistor base pad (`BL`).
- **Dimming:** Driven via `TIM3_CH4` (AF0) with hardware PWM for variable brightness levels (0..100%).
- **Credit:** This universal solution was designed and documented by the OpenI6X project contributors (notably Kuba / qba667 and Janek / ajjjjjjjj), providing hardware PWM control without conflicting with any other radio peripherals.
- **Software Strategy:** The firmware simultaneously drives `PF3` and `PC9` HIGH, supporting both stock and modded hardware transparently.

> [!NOTE]
> **Pin Verification:** Ensure connections are made to `PC9` rather than `PB1`. `PB1` is physically routed to Switch SD (ADC Channel 9).

---

## 5. Piezo Buzzer Audio Driver

The audible beeper is a passive piezoelectric transducer driven by hardware PWM:
- **Control Pin:** **`GPIOA` Pin 8 (`PA8`)**
- **Timer / Channel:** **`TIM1_CH1`** configured in Alternate Function 2 (`AF2`, push-pull).
- **Clock Configuration:** `TIM1` clocked at 48 MHz with prescaler `PSC = 47` yielding an exact 1.000 µs tick count.
- **Tone Generation:** Variable period register (`ARR = 1,000,000 / freq_hz`) and 50% duty cycle (`CCR1 = ARR / 2`).
- **Advanced Timer Output:** Requires Main Output Enable bit set in Break and Dead-Time Register (`TIM1->BDTR |= TIM_BDTR_MOE`).
- **Non-Blocking Sequencing:** The audio state machine tracks duration via `buzzer.tick(dt_ms)` in the main loop, automatically disabling timer output on tone completion without blocking RF interrupts.

| Audio Event | Frequency (Hz) | Duration (ms) | Description |
| :--- | :--- | :--- | :--- |
| **Boot Click** | 2250 Hz | 15 ms | Friendly power-on acoustic confirmation |
| **Nav Click** | 2400 Hz | 12 ms | Light feedback when pressing menu buttons |
| **Trim Step** | 1500 .. 2500 Hz | 25 ms | Dynamic pitch shifting with trim step offset |
| **Trim Center** | 2800 Hz | 60 ms | High-pitch confirmation when reaching 0 neutral |
| **Trim Limit** | 1100 Hz | 45 ms | Low warning buzz when hitting ±25 limits |
| **Bind Success** | 2200 / 2800 Hz | 80 ms each | Two-tone rising fanfare upon binding receiver |
| **Calib Success** | 2000 / 2800 Hz | 100 ms each | Confirmation chime when savi---

## 6. Safe DFU Bootloader Jump & Power Latch Retention

The STM32F072 contains a factory-programmed DFU bootloader in System ROM (`0x1FFFC800`). The FS-i6S firmware supports jumping into this bootloader with **zero disassembly**, while maintaining the electronic power latch on `PB15`.

### Jump Requirements & Power Latch Preservation

1. **Retain Power Latch:** On the FS-i6S, power is electronically held by `PB15`. If RCC de-initialization disables `GPIOB`, `PB15` will float and power will be cut instantly. In `chip::rcc_deinit()`, the `GPIOB` peripheral clock is explicitly preserved:
   ```rust
   // Reset AHB peripheral clocks but KEEP GPIOB enabled (bit 18) for PB15 power latch
   rcc.ahbenr.write(|w| unsafe { w.bits(0x0004_0014) });
   ```
   Furthermore, `power::latch_on()` is called immediately before and after RCC de-initialization.
2. **Clear SysTick & NVIC:** Disable SysTick timer and clear all pending interrupt requests in NVIC.
3. **SYSCFG Remap:** Remap System Memory to `0x00000000` via `SYSCFG->CFGR1` (`MEM_MODE = 0b01`).
4. **Re-Enable Global Interrupts:** **CRITICAL.** The ST factory DFU bootloader requires USB interrupts to enumerate on the host PC. Global interrupts must be enabled (`cortex_m::interrupt::enable()`) before executing the jump.
5. **Bootstrap:** Load Main Stack Pointer (`MSP`) from `0x1FFFC800` and branch to reset handler at `0x1FFFC804` via `cortex_m::asm::bootstrap`.

### Entry Methods (Zero-Disassembly)

1. **Cold-Boot Combo:** Holding both **Front Buttons (`PA9` + `PA10`)** simultaneously during power-on triggers `boot::check_dfu_entry()`, immediately transferring control to the factory ROM DFU bootloader.
2. **Software Touch Menu:** Navigating to **`Settings -> Diag -> [OK] Reboot DFU`** sets a magic flag (`0xDEADBEEF`) at SRAM address `0x2000_3FF0` and performs a system reset. On reboot, `boot::check_dfu_entry()` detects the flag, clears it, and jumps directly into DFU.
3. **USB CDC Serial CLI:** Sending `dfu` or `reboot bootloader` over the virtual serial console calls `boot::reboot_to_dfu()`.

---

## 7. Analog Inputs & ADC1 Channel Map

The FlySky FS-i6S uses a single 12-bit ADC peripheral (**ADC1**) paired with **DMA1 Channel 1** operating in circular mode to continuously scan 11 analog channels into SRAM without CPU intervention.

### Verified 11-Channel Mapping

| ADC Ch | MCU Pin | Function / Axis | Physical Input | Normal Expected Range | Notes |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **CH0** | `PA0` | **Roll / Aileron** | Right Stick Horizontal | ~1100 .. 2048 .. ~2900 | Spring return |
| **CH1** | `PA1` | **Pitch / Elevator** | Right Stick Vertical | ~1100 .. 2048 .. ~2900 | Spring return |
| **CH2** | `PA2` | **Throttle** | Left Stick Vertical | ~1100 .. ~2900 | Centered or ratchet |
| **CH3** | `PA3` | **Yaw / Rudder** | Left Stick Horizontal | ~1100 .. 2048 .. ~2900 | Spring return |
| **CH4** | `PA4` | **Switch SA** | 2-Position Toggle | Down < 2000, Up > 2000 | Resistor divider |
| **CH5** | `PA5` | **Switch SB** | 3-Position Toggle | Up > 2500, Mid 1000..2500, Dwn < 1000 | Resistor divider |
| **CH6** | `PA6` | **Potentiometer VRA** | Left Rotary Dial (VR1) | 0 .. 4095 (scaled 0..9) | Linear pot |
| **CH7** | `PA7` | **Potentiometer VRB** | Right Rotary Dial (VR2) | 0 .. 4095 (scaled 0..9) | Linear pot |
| **CH8** | `PB0` | **Switch SC** | 3-Position Toggle | Up > 2500, Mid 1000..2500, Dwn < 1000 | Resistor divider |
| **CH9** | `PB1` | **Switch SD** | 2-Position Toggle | Down < 2000, Up > 2000 | Resistor divider |
| **CH10**| `PC0` | **Battery Sense** | 4×AA Battery Pack | ~1400 .. 2100 (4.0V .. 6.0V) | $10\text{ k}\Omega / 5.1\text{ k}\Omega$ divider |

### Battery Voltage Sensing (FS-i6S Scaling)

The FlySky FS-i6S uses a precision $10\text{ k}\Omega$ (upper) and $5.1\text{ k}\Omega$ (lower) resistive divider on `PC0` (ADC Channel 10):
- **Divider Ratio:** $\frac{R_1 + R_2}{R_2} = \frac{10 + 5.1}{5.1} = 2.960784...$
- **Voltage Formula (in mV):**
  $$ V_{bat} = \left( \text{raw} \times 3300 \times 2.960784 \right) / 4095 = \frac{\text{raw} \times 977064}{409500} $$
- Implemented with 64-bit integer arithmetic in [`src/input.rs`](../src/input.rs):
  ```rust
  ((raw as u32 * 977064) / 409500) as u16
  ```
- Validated across operating battery levels:
  - 4× NiMH Rechargeable (~4.8V): ~2011 counts -> `4800 mV`
  - 4× Alkaline Fresh (~6.0V): ~2514 counts -> `6000 mV`

---

## 8. USB Interface & Rear Expansion Bay

### Hardware USB Interface (Micro-USB Port)
The FlySky FS-i6S mainboard routes the Micro-USB port directly to the STM32F072 hardware USB controller:

| Pin | Function | Mode | Description |
| :--- | :--- | :--- | :--- |
| **`PA11`** | `USB_DM` | Alternate Function 0 (`AF0`) | USB Full-Speed Data - line |
| **`PA12`** | `USB_DP` | Alternate Function 0 (`AF0`) | USB Full-Speed Data + line |
| **Silicon Internal** | 1.5 kΩ Pull-up | Software-Controlled | Engaged by setting bit 15 (`DPPU`) in `USB_BCDR` (`0x4000_5C58`) |

- **Packet Memory Area (PMA)**: 1024 bytes located at `0x4000_6000` (`MemoryAccess::Word16x2`).
- **Clock Tree**: Clocked directly from 48.000 MHz PLLCLK via `RCC_CFGR3` bit 7 (`USBSW = 1`).
- **Modes Supported**:
  - **HID Gamepad (`0x1209:0x4F54`)**: 100 Hz 8-axis 16-button gamepad for flight simulators.
  - **CDC-ACM Serial (`0x0483:0x5740`)**: Interactive CLI and 10 Hz / 20 Hz telemetry streaming.
  - **Composite (`0x1209:0x4968`)**: Simultaneous HID Gamepad and CDC-ACM Virtual COM Port via USB Interface Association Descriptors (IAD).
  - **Off**: D+ pull-up disconnected, peripheral clock gated to eliminate battery consumption during charging.

### Rear Expansion Bay & Trainer Port (CRSF / ELRS Ready)
The 4-pin round rear port (and internal expansion header / connector `J15`) connects to the MCU's hardware `USART2`:

| Pin / Net | MCU Pin | Function | Notes |
| :--- | :--- | :--- | :--- |
| **Signal TX** | `PD5` | `USART2_TX` (AF0) | Serial output to external module; broken out at test pad **`TX`** directly above `J15` |
| **Signal RX** | `PA15` | `USART2_RX` (AF1) | Serial telemetry downlink; broken out at test pad **`RX`** directly above `J15` (shared with `TOUCH_RST`) |
| **Module Power**| `PC13` | Power Switch GPIO | Configurable polarity (Default High / Active Low supported in Radio Setup) |
| **Baud Rate** | Selectable | 8N1 | 420k (ELRS), 416.6k (TBS), 115.2k (Low), 921.6k (Fast) |

#### PCB Test Pads & Wiring (`J15`)
Directly above internal connector `J15` (between the 8-pin harness and the internal RF module shield), two circular solder test pads are silk-screened **`TX`** and **`RX`**:
- **`TX` Pad:** `PD5` (`USART2_TX`). Wire to external CRSF module `RX`.
- **`RX` Pad:** `PA15` (`USART2_RX`). Wire to external CRSF module `TX` (telemetry).

#### PA15 Decoupling from Touch Controller (`TOUCH_RST`)
On the FS-i6S, `PA15` is also connected to Pin 4 (`RST`) of touch FPC connector `J13` (`TOUCH_RST`). For full-duplex operation, several decoupling options exist:
1. **Low-Pass RC Filter (Non-Destructive):** Place an inline $10\text{ k}\Omega$ resistor and $100\text{ nF}$ capacitor to GND ($\tau = 1.0\text{ ms}$) on the touch reset line. Filters out 420 kbaud serial transitions ($\sim 2.4\ \mu\text{s}$) while allowing the 20 ms boot pulse to pass.
2. **Permanent VDD Tie-High:** Remove the series resistor / cut the trace between `PA15` and `J13` Pin 4, and tie `J13` Pin 4 to 3.3V (FT6236 uses internal Power-On Reset).
3. **GPIO Remap:** Rewire `J13` Pin 4 to an unused RF module pad (e.g. `RF_GIO1` / `PE14`) and update the driver in `src/touch/ft6236.rs`.
4. **Single-Wire Half-Duplex:** Alternatively, run single-wire half-duplex CRSF on `PD5` (`HDSEL = 1`), leaving `PA15` untouched as a static GPIO High.

See [CRSF / ExpressLRS Subsystem Guide](CRSF_ELRS_GUIDE.md) for complete details.

#### Electrical Reset Dynamics & Baud Rate Sensitivity Analysis

The potential for touchscreen reset stems from the electrical characteristics of the FocalTech FT6236 controller and UART framing physics:

- **Active-LOW Reset Physics:** The FT6236 `RST` pin is active-LOW (0V / GND triggers reset, 3.3V is normal operating mode). An internal pull-up resistor keeps the line HIGH when idle.
- **UART Framing & Contiguous LOW Duration:** In asynchronous serial (8N1), the line idles HIGH. Data is framed with 1 Start Bit (LOW), 8 Data Bits, and 1 Stop Bit (HIGH). The longest possible contiguous LOW duration in a single frame occurs when sending byte `0x00` (Start Bit + 8 zero bits = 9 bit times), after which the Stop Bit forces the line back HIGH to 3.3V.
- **Reset Trigger Threshold:** The FT6236 hardware reset comparator requires a sustained LOW pulse of **$\ge 1.0\text{ ms}$ ($1000\ \mu\text{s}$)** to trigger a reset.

The following table compares each supported firmware baud rate against the $1\text{ ms}$ reset threshold:

| Baud Rate | Protocol / Profile | Single Bit Time ($t_{\text{bit}}$) | Max Continuous Low Pulse ($9\text{ bits}$) | Safety Margin to $1\text{ ms}$ Reset | Likelihood of Normal Data Triggering Reset |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **921,600 baud** | Fast CRSF / Ultra-low latency | **$1.09\ \mu\text{s}$** | **$9.8\ \mu\text{s}$** | **$102\times$ safety margin** | **Effectively 0%** |
| **420,000 baud** | Standard ExpressLRS | **$2.38\ \mu\text{s}$** | **$21.4\ \mu\text{s}$** | **$47\times$ safety margin** | **Effectively 0%** |
| **416,666 baud** | Standard TBS Crossfire | **$2.40\ \mu\text{s}$** | **$21.6\ \mu\text{s}$** | **$46\times$ safety margin** | **Effectively 0%** |
| **115,200 baud** | Legacy / Debug Serial | **$8.68\ \mu\text{s}$** | **$78.1\ \mu\text{s}$** | **$13\times$ safety margin** | **Very Low (higher noise susceptibility)** |

##### Key Insights:
1. **Faster Baud Rates Are Inherently Safer During Active Data Streaming:** At 420k and 921.6k baud, individual zero-bit pulses ($1\text{--}2\ \mu\text{s}$) are dozens of times too short to overcome the FT6236's internal RC filter and trip the reset comparator.
2. **True Cause of Accidental Resets:** Active serial packet transmission does not cause touchscreen resets. Accidental resets are triggered by **static / non-data LOW states**:
   - **Unpowered or Booting External Module:** A module whose TX pin floats to 0V or pulls LOW while powered off holds `PA15` LOW continuously, keeping the touch controller pinned in reset.
   - **Baud Rate Mismatches & Serial Breaks:** If the radio and module baud rates do not match, framing errors can trigger prolonged serial break conditions ($> 1\text{ ms}$ LOW).
   - **Cable Disconnects / Floating High-Z Lines:** Disconnecting an external module without a pull-up resistor can allow `PA15` to float into the CMOS undefined/low region.

