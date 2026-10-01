# FLIGHT CONTROL INPUTS & DIGITAL TRIMS

Documentation for analog stick sampling, gimbal potentiometer geometry, switch decoding, touchscreen navigation, digital trim controllers, and audio feedback on the FlySky FS-i6S.

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
| **10** | `PC0` | **Battery Voltage Sense** | $10\text{ k}\Omega / 5.1\text{ k}\Omega$ divider: `(raw * 977064) / 409500` mV |

---

## 2. Gimbal Potentiometer Geometry & Endpoints

FlySky FS-i6S gimbals use dedicated potentiometers that sweep almost their entire resistive track across mechanical stick movement (~3400 ADC counts total):
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

## 3. Digital Trim Subsystem (Touch & Modifier Trims)

The FlySky FS-i6S does not have mechanical trim rocker switches. Instead, it provides two complementary digital trim systems implemented in [`src/touch/nav.rs`](../src/touch/nav.rs):

### 1. Stick Modifier Trim Mode (In-Flight Ergonomic Trimming)
Using the rear tactile buttons as modifier keys, pilots can adjust trims in-flight without lifting their thumbs from the gimbals:
- **Hold Rear Left (`PA9`)**:
  - Deflect Left Stick Horizontally (threshold $> 350$ counts): Adjusts **Yaw Trim** (Left / Right).
  - Deflect Left Stick Vertically (threshold $> 350$ counts): Adjusts **Throttle Trim** (Up / Down).
- **Hold Rear Right (`PA10`)**:
  - Deflect Right Stick Horizontally (threshold $> 350$ counts): Adjusts **Roll Trim** (Left / Right).
  - Deflect Right Stick Vertically (threshold $> 350$ counts): Adjusts **Pitch Trim** (Up / Down).
- When engaged as a trim modifier, the rear buttons' normal navigation actions (`Cancel` / `OK`) are suppressed.

### 2. Virtual Touch Perimeter Targets (Direct Screen Trimming)
When viewing the main flight dashboard, tap zones along the LCD borders provide direct single-step trimming:
- **Left Edge ($x < 16$)**: Throttle Trim Up ($y < 32$) / Throttle Trim Down ($y \ge 32$).
- **Right Edge ($x > 112$)**: Pitch Trim Up ($y < 32$) / Pitch Trim Down ($y \ge 32$).
- **Bottom Edge ($y > 52$)**:
  - Yaw Trim Left ($x < 36$) / Yaw Trim Right ($36 \le x < 64$).
  - Roll Trim Left ($64 \le x < 92$) / Roll Trim Right ($x \ge 92$).

### Trim Authority & Modes
- **Authority**: Each axis provides ±25 discrete steps (±100 µs total authority), repeating every **90 ms** if held.
- **Selectable Throttle Trim Modes**: Configurable via the Radio Setup menu (`config.throttle_trim`):
  1. **`OFF (Lock)` (Default)**: Throttle trim is disabled; attempting to trim sounds a limit warning buzz (`1100 Hz`). Safe for multicopter flight controllers.
  2. **`IDLE (T-Trim)`**: OpenTX-style throttle trim for glow/gas/IC aircraft (100% trim authority at low stick, tapering to 0% at full throttle).
  3. **`LINEAR`**: Uniform trim (±100 µs) applied across the entire throttle stick throw.
- **Visual & Audio Feedback**:
  - Real-time bottom bar banner (`TRM A:+04`) and tick marks on channel slider gauges.
  - Dynamic pitch-shifted tones (`1500 Hz .. 2500 Hz`), zero-center confirmation chime (`2800 Hz`), and boundary alert buzz (`1100 Hz`).

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

## 5. Touchscreen & Rear Button Navigation Subsystem (`src/touch/nav.rs`, `src/boot.rs`)

The FS-i6S user interface is navigated seamlessly via capacitive touchscreen gestures, direct tap zones, and rear push-buttons:

### Touch Gestures & Hitbox Zones
- **Swipe Gestures**:
  - **Swipe UP**: Navigate Up / Decrement value.
  - **Swipe DOWN**: Navigate Down / Increment value.
  - **Swipe RIGHT**: Select / Confirm / OK.
  - **Swipe LEFT**: Cancel / Back / Exit.
- **Tap Hitbox Zones in Menus**:
  - **Top Row ($y < 22$)**: Up.
  - **Bottom Row ($y > 44$)**: Down.
  - **Left Edge ($x < 32$)**: Cancel / Exit.
  - **Right Edge ($x > 96$)**: OK / Select.
  - **Center Zone**: OK / Select.
- **Flight Dashboard Center Tap**:
  - Tapping the center area of the flight dashboard ($32 \le x \le 96$, $20 \le y \le 44$) opens the **Settings Menu**.

### Physical Rear Buttons
- **Rear Left Button (`PA9`)**:
  - Tap: Cancel / Back / Exit.
  - Hold >= 1.0s on flight dashboard: Resets flight countdown / stopwatch timer with on-screen HUD progress bar.
- **Rear Right Button (`PA10`)**:
  - Tap: OK / Select / Enter submenu.
  - Hold >= 1.2s on flight dashboard: Opens Settings Menu.
  - Hold during power-on: Launches 2-step gimbal calibration wizard immediately.
- **Cold DFU Combo**: Holding **`PA9` + `PA10`** during power-on forces an immediate zero-disassembly jump into the factory ROM DFU bootloader.

