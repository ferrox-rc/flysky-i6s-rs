# FLIGHT CONTROL INPUTS & DIGITAL TRIMS

Documentation for analog stick sampling, gimbal potentiometer geometry, switch decoding, digital trim controllers, and audio feedback on the FlySky FS-i6X.

---

## 1. ADC Channel Mapping (Mode 2)

The autonomous ADC1 scanner digitizes 11 channels continuously into SRAM via DMA1 Channel 1:

| DMA Ch | MCU Pin | Function | Direction / Scaling |
| :--- | :--- | :--- | :--- |
| **0** | `PA0` | **Roll / Aileron** (Right Horizontal) | Left (-1000) .. Right (+1000) [Inverted] |
| **1** | `PA1` | **Pitch / Elevator** (Right Vertical) | Down (-1000) .. Up (+1000) [Inverted] |
| **2** | `PA2` | **Throttle** (Left Vertical) | Bottom (-1000 / 0%) .. Top (+1000 / 100%) |
| **3** | `PA3` | **Yaw / Rudder** (Left Horizontal) | Left (-1000) .. Right (+1000) |
| **4** | `PA4` | **Switch SA** (2-Position) | UP (1000 µs) / DOWN (2000 µs) |
| **5** | `PA5` | **Switch SB** (3-Position) | UP (1000 µs) / MID (1500 µs) / DOWN (2000 µs) |
| **6** | `PA6` | **Potentiometer VRA / VR1** | Rotary dial: Left (-1000) .. Right (+1000) |
| **7** | `PA7` | **Potentiometer VRB / VR2** | Rotary dial: Left (-1000) .. Right (+1000) |
| **8** | `PB0` | **Switch SC** (3-Position) | UP (1000 µs) / MID (1500 µs) / DOWN (2000 µs) |
| **9** | `PB1` | **Switch SD** (2-Position) | UP (1000 µs) / DOWN (2000 µs) |
| **10** | `PC0` | **Battery Voltage Sense** | Resistor divider formula: `(raw * 100) / 421 + 20` |

---

## 2. Gimbal Potentiometer Geometry & Endpoints

FlySky FS-i6X gimbals use dedicated potentiometers that sweep almost their entire resistive track across mechanical stick movement (~3400 ADC counts total):
- **Horizontal Axes (Roll `A`, Yaw `R`)**: Wide mechanical clearance (~1670–1720 counts throw from center). Default `GIMBAL_H_HALF_SPAN = 1670`.
- **Vertical Axes (Pitch `E`, Throttle `T`)**: Narrower limit stops molded into the gimbal chassis (~1620–1650 counts throw from center). Default `GIMBAL_V_HALF_SPAN = 1580`.

### OpenTX Modified Moving Average (MMA) Jitter Filter
To remove potentiometer electrical jitter without introducing deadbands or control latency:
- If raw ADC change is >= 20 counts (stick actively in motion), the sample passes through immediately (**zero latency**).
- For micro-fluctuations (< 20 counts), an integer MMA filter (16x oversampling) smooths the reading:
  ```text
  filtered = filtered - prev + raw
  ```

### Battery Voltage Exponential Moving Average (EMA) Filter
Raw ADC measurements on `PC0` via the internal resistor divider exhibit ±10..20 mV of switching regulator ripple and noise. Unfiltered, this causes rapid fluctuations in the hundredths decimal digit (`X.YYV`), creating an unreadable visual blur on the LCD:
- Implemented an integer fixed-point IIR filter (alpha = 1/32, tau approx 640 ms):
  ```text
  EMA_k = EMA_{k-1} - (EMA_{k-1} >> 5) + (Sample_k << 3)
  ```
- On transmitter startup, the filter initializes directly with the first measured sample, eliminating boot delay while completely steadying the hundredths readout.

Implemented in [`src/input.rs`](../src/input.rs).

---

## 3. Digital Trim Subsystem

The transmitter has 4 trim rocker switches (8 directional switches) connected to columns 0 and 1 of the key matrix:

| Axis | Trim Switches | Key Matrix Bit | Authority |
| :--- | :--- | :--- | :--- |
| **Roll (A)** | Right (`TRM_RH_UP`) / Left (`TRM_RH_DWN`) | Bit 0 / Bit 1 | ±25 steps (±100 µs) |
| **Pitch (E)**| Up (`TRM_RV_UP`) / Down (`TRM_RV_DWN`) | Bit 2 / Bit 3 | ±25 steps (±100 µs) |
| **Throttle (T)**| Up (`TRM_LV_UP`) / Down (`TRM_LV_DWN`) | Bit 4 / Bit 5 | **Selectable (Off / Idle / Linear)** |
| **Yaw (R)** | Right (`TRM_LH_UP`) / Left (`TRM_LH_DWN`) | Bit 6 / Bit 7 | ±25 steps (±100 µs) |

### Behavior & Features
- **Single-Click & Auto-Repeat**: Instant single step on press; automatically repeats every **90 ms** if held for > 350 ms.
- **DFU Bootloader Lockout**: When the DFU inward trim combination is pressed (Roll Left + Yaw Right), trim adjustments are locked out to prevent accidental trim changes.
- **Selectable Throttle Trim Modes**: Configurable via the Radio Setup menu (`config.throttle_trim`):
  1. **`OFF (Lock)` (Default)**: Throttle trim rockers are disabled; pressing them sounds a limit warning buzz (`1100 Hz`). Safe for Betaflight, INAV, and ArduPilot flight controllers to avoid accidental disarm or arming lockouts.
  2. **`IDLE (T-Trim)`**: OpenTX-style throttle trim for glow/gas/IC aircraft. 100% trim authority at low stick (1000 µs, adjustable between 900..1100 µs for engine idle and cutoff), tapering linearly to **0% authority at full throttle** (2000 µs) so high throttle is never shifted or clipped.
  3. **`LINEAR`**: Standard uniform trim (±100 µs) applied across the entire throttle stick throw.
- **Display Banner & Gauge Indicator**: Adjusting any trim displays a real-time callout on the bottom bar (e.g. `TRM A:+04`), with visual tick marks drawn on the channel slider gauges. When throttle trim is enabled and active, a dynamic contrast tick mark appears on the throttle progress bar.

---

## 4. Hardware Piezo Buzzer (`src/buzzer.rs`)

The piezo buzzer on pin `PA8` is driven by **`TIM1_CH1`** in hardware PWM Mode 1:
- Clocked at 48 MHz with `PSC = 47` (1.000 µs per timer tick).
- Generates precise audio tones at 50% duty cycle (`CCR1 = ARR / 2`).

### Audio Tones & Chimes
- **Power-On Welcome Fanfare**: 4-note ascending fanfare ($C_6 \to E_6 \to G_6 \to C_7$, 660 ms) played during the startup splash screen when Tone Style is set to `Rich`. Plays a single tactile click (15 ms) when set to `Simple`.
- **Arming / Disarming Chimes**: 2-note rising chirp (`1800 Hz` -> `2400 Hz`) on motor arm, and falling chirp (`2400 Hz` -> `1800 Hz`) on disarm.
- **Pitch-Shifted Trim Step**: Tones scale dynamically with trim position (`1500 Hz` to `2500 Hz`).
- **Trim Center Confirm**: High-pitched distinctive tone (`2800 Hz`, 60 ms) when crossing zero.
- **Trim Limit Buzz**: Low warning buzz (`1100 Hz`, 45 ms) when attempting to exceed ±25 steps.
- **Calibration Chime**: Rising 2-tone chime (`2400 Hz` -> `2800 Hz`) when calibration is saved.
- **Watchdog Recovery Alert**: Rapid 3-beep warning pattern (`2600 Hz`, 60 ms on / 40 ms off) alerting the pilot that an in-flight watchdog reset was recovered.

---

## 5. Navigation Keypad & Auto-Repeat (`src/boot.rs`, `src/menu.rs`)

The transmitter keypad is scanned via the 3 x 4 GPIO key matrix:
- **`[UP]`** and **`[DOWN]`**:
  - Immediate single-step trigger on press.
  - If held for >= 300 ms, auto-repeats rapidly every **70 ms** for smooth scrolling through lists, rapid cycling through ASCII characters during model naming, and swift point adjustments in the throttle curve editor.
- **`[OK]`**:
  - Short tap confirms selections or enters submenus.
  - Long hold (>= 1.2s) on the main flight screen invokes the Settings Menu.
  - Hold during power-on triggers immediate stick calibration.
- **`[CANCEL]` (`[ESC]`)**:
  - Exits active menus, aborts calibration, or saves and finishes one-way receiver binding.
- **`[BIND]` (Dedicated Key on `PF2`)**:
  - Independent active-low GPIO input on pin `PF2`.
  - Filtered by software separation logic to support cycling display pages, initiating binding, and advancing field cursors without cross-mode interference.

