# HARDWARE REFERENCE & BRING-UP NOTES

Technical reference documentation for the FlySky FS-i6X hardware. This document builds upon the foundational hardware reverse-engineering, register mappings, and schematics pioneered by the **OpenI6X** project and the open-source RC community.

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

## 2. Keypad & Trim Matrix (3 Columns × 4 Rows)

The radio faceplate buttons and trims are arranged in a 3×4 matrix scanned by the MCU, plus one dedicated direct button for Bind.

### Matrix Wiring

- **Columns (Outputs):** `PC6` (Col 0 / R1), `PC7` (Col 1 / R2), `PC8` (Col 2 / R3)
  - Driven LOW sequentially during scanning; kept HIGH at idle.
- **Rows (Inputs):** `PD12` (Row 0 / L1), `PD13` (Row 1 / L2), `PD14` (Row 2 / L3), `PD15` (Row 3 / L4)
  - Configured as inputs with internal pull-ups (`GPIO_PuPd_UP`). Active LOW when pressed.
- **Dedicated Bind Key:** `PF2`
  - Input with internal pull-up. Connected directly to GND when pressed (Active LOW).

### Matrix Key Map

| Row / Line | Column 0 (`PC6`) | Column 1 (`PC7`) | Column 2 (`PC8`) |
| :--- | :--- | :--- | :--- |
| **Row 0 (`PD12`)** | Roll Right (`TRM_RH_UP`) | Throttle Up (`TRM_LV_UP`) | **Down** (`KEY_DOWN`) |
| **Row 1 (`PD13`)** | **Roll Left** (`TRM_RH_DWN`) | Throttle Down (`TRM_LV_DWN`) | **Up** (`KEY_UP`) |
| **Row 2 (`PD14`)** | Pitch Up (`TRM_RV_UP`) | **Yaw Right** (`TRM_LH_UP`) | **OK / Enter** (`KEY_ENTER`) |
| **Row 3 (`PD15`)** | Pitch Down (`TRM_RV_DWN`) | Yaw Left (`TRM_LH_DWN`) | **Cancel / Exit** (`KEY_EXIT`) |

> [!NOTE]
> **Bootloader Combo:** Pushing both horizontal trims inward towards the power switch activates **Roll Left** (`PC6` + `PD13`) and **Yaw Right** (`PC7` + `PD14`). In OpenI6X this corresponds to mask `0x1080`.

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

The ST7567 controller contains 132 column segment drivers, while the FS-i6X physical LCD panel is 128 pixels wide.
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
| **Calib Success** | 2000 / 2800 Hz | 100 ms each | Confirmation chime when saving gimbals |

---

## 6. Safe DFU Bootloader Jump

The STM32F072 contains a factory-programmed DFU bootloader in System ROM (`0x1FFFC800`). The firmware can jump into this bootloader in software without physical access to the `BOOT0` pin.

### Jump Requirements

1. **Reset RCC:** Return all peripheral clocks to power-on defaults (HSI 8 MHz, PLL disabled).
2. **Clear SysTick & NVIC:** Disable SysTick timer and clear all pending interrupt requests in NVIC.
3. **SYSCFG Remap:** Remap System Memory to `0x00000000` via `SYSCFG->CFGR1` (`MEM_MODE = 0b01`).
4. **Re-Enable Global Interrupts:** **CRITICAL.** The ST factory DFU bootloader requires USB interrupts to enumerate on the host PC. Global interrupts must be enabled (`cortex_m::interrupt::enable()`) before executing the jump.
5. **Bootstrap:** Load Main Stack Pointer (`MSP`) from `0x1FFFC800` and branch to reset handler at `0x1FFFC804` via `cortex_m::asm::bootstrap`.

### Hardware Recovery & Initial Stock Flash (`R53`)

The hardware override for forcing the microcontroller into permanent ROM DFU bootloader mode is the **`R53`** solder pads located on the rear of the motherboard (accessible by removing the back case screws):
- **Function:** Connecting the two pads of `R53` pulls the microcontroller's `BOOT0` pin directly to 3.3V (`VDD`).
- **Initial Flashing from Stock:** Factory FlySky firmware does not contain the inward-trims software bootloader jump. To flash custom firmware (`flysky-i6x-rs` or `OpenI6X`) for the first time, `R53` must be momentarily bridged while powering the transmitter on.
- **Hardware Recovery:** If custom firmware ever hangs or crashes before scanning input keys, bridging `R53` during power-on guarantees access to the ST/Geehy ROM bootloader (`0483:df11` / `314b:0106`).
- **Removal:** The bridge only needs to be held during initial power-on; once the chip samples `BOOT0` at reset, the bridge can be released.
- **Reference:** See the [OpenI6X Flashing & Upgrading Wiki](https://github.com/OpenI6X/opentx/wiki/Flashing-&-Upgrading) for mainboard layout photos and test point markings.

---

## 7. Analog Inputs & ADC1 Channel Map

The FlySky FS-i6X uses a single 12-bit ADC peripheral (**ADC1**) paired with **DMA1 Channel 1** operating in circular mode to continuously scan 11 analog channels into SRAM without CPU intervention.

### Verified 11-Channel Mapping

| ADC Ch | MCU Pin | Function / Axis | Physical Input | Normal Expected Range | Notes |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **CH0** | `PA0` | **Roll / Aileron** | Right Stick Horizontal | ~1100 .. 2048 .. ~2900 | Spring return |
| **CH1** | `PA1` | **Pitch / Elevator** | Right Stick Vertical | ~1100 .. 2048 .. ~2900 | Spring return |
| **CH2** | `PA2` | **Throttle** | Left Stick Vertical | ~1100 .. ~2900 | Friction ratchet (no spring) |
| **CH3** | `PA3` | **Yaw / Rudder** | Left Stick Horizontal | ~1100 .. 2048 .. ~2900 | Spring return |
| **CH4** | `PA4` | **Switch SA** | 2-Position Toggle | Down &lt; 2000, Up &gt; 2000 | Resistor divider |
| **CH5** | `PA5` | **Switch SB** | 3-Position Toggle | Up &gt; 2500, Mid 1000..2500, Dwn &lt; 1000 | Resistor divider |
| **CH6** | `PA6` | **Potentiometer VRA** | Left Rotary Dial (VR1) | 0 .. 4095 (scaled 0..9) | Linear pot |
| **CH7** | `PA7` | **Potentiometer VRB** | Right Rotary Dial (VR2) | 0 .. 4095 (scaled 0..9) | Linear pot |
| **CH8** | `PB0` | **Switch SC** | 3-Position Toggle | Up &gt; 2500, Mid 1000..2500, Dwn &lt; 1000 | Resistor divider |
| **CH9** | `PB1` | **Switch SD** | 2-Position Toggle | Down &lt; 2000, Up &gt; 2000 | Resistor divider |
| **CH10**| `PC0` | **Battery Sense** | 4×AA Battery Pack | ~1600 .. 2300 (4.0V .. 6.0V) | 2:1 resistive divider |

> [!NOTE]
> **Pot & Switch Verification Note:** Prior reverse-engineering notes occasionally misidentified `VRB` as `PB0` and `SC` as `PA7`. Physical silicon testing proved conclusively that **`VRB` is `PA7`** and **`SC` is `PB0`**.

### Stick Calibration & Mechanical Offset

1. **Resting Center Offset:**
   - The mechanical potentiometers on the FS-i6X gimbals physically rest around **~1930 counts**, not the theoretical mathematical midpoint of **2048**.
   - If scaled assuming a fixed 2048 center, sticks report approximately **-13%** offset at resting neutral.
   - **Solution:** The firmware dynamically auto-calibrates resting centers at boot by taking an average of 16 full ADC scans after the first DMA conversion cycle completes.
2. **DMA First-Conversion Synchronization:**
   - Calling input calibration immediately after ADC initialization will read zeroes if the DMA buffer has not yet completed its first 11-channel sweep.
   - The driver waits for DMA1 Channel 1 Transfer Complete Flag (`TCIF1`) before sampling centers (`adc::wait_first_conversion()`).
3. **Endpoint Range Expansion:**
   - Rather than relying on rigid factory bounds, the piecewise calibrator expands its min/max endpoints dynamically whenever physical stick deflection exceeds the stored bounds, ensuring full `-1000 .. +1000` throw without clipping.
4. **Adaptive Noise / Jitter Filtering:**
    - To counteract track wear and ADC noise (especially prominent on the Rudder gimbal), an adaptive exponential moving average (EMA) filter is applied:
      - Movements <= 12 raw counts are filtered to eliminate jitter.
      - Rapid intentional movements (> 12 counts) bypass the filter completely to preserve zero-latency response.

### Battery Voltage Sensing

- Connected to `PC0` (ADC Channel 10) through a resistive voltage divider:
  ```text
  Voltage (in 0.1V units) = ((raw * 100) / 421) + 20
  ```
- Validated against physical AA battery pack voltages:
  - 4x NiMH (~4.8V): ~1930 raw counts -> `4.8V`
  - 4x Alkaline fresh (~6.0V): ~2440 raw counts -> `6.0V`

---

## 8. USB Interface & Rear Expansion Bay

### Hardware USB Interface (Micro-USB Port)
The FlySky FS-i6X mainboard routes the Micro-USB port directly to the STM32F072 hardware USB controller:

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
The 4-pin round rear port (and internal expansion header) connects to the MCU's hardware `USART2`:

| Pin / Net | MCU Pin | Function | Notes |
| :--- | :--- | :--- | :--- |
| **Signal TX** | `PD5` | `USART2_TX` (AF0) | Asynchronous serial output to external module |
| **Signal RX** | `PA15` | `USART2_RX` (AF1) | Serial telemetry downlink from external module |
| **Module Power**| `PC13` | Power Switch GPIO | Configurable polarity (Default High / Active Low supported in Radio Setup) |
| **Baud Rate** | Selectable | 8N1 | 420k (ELRS), 416.6k (TBS), 115.2k (Low), 921.6k (Fast) |

