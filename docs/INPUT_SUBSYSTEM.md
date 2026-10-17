# FLIGHT CONTROL INPUTS & DIGITAL TRIMS

Documentation for analog stick sampling, gimbal potentiometer geometry, universal configurable input modes (`AdcInputMode`), touchscreen navigation, digital trim controllers, and audio feedback on the FlySky FS-i6S.

---

## 1. ADC Channel Mapping (`src/hw/adc.rs`, `src/input.rs`)

The autonomous `ADC1` scanner digitizes 11 standard channels (`0x07FF`) or 15 channels (`0x7FFF` when `Ext ADC (PC)` is enabled) continuously into SRAM via `DMA1` Channel 1:

| DMA Ch | MCU Pin | Function | Direction / Default Scaling |
| :--- | :--- | :--- | :--- |
| **0** | `PA0` | **Right Stick Horizontal (`RH`)** | Left (-1000) .. Right (+1000) [Inverted] |
| **1** | `PA1` | **Right Stick Vertical (`RV`)** | Down (-1000) .. Up (+1000) [Inverted] |
| **2** | `PA2` | **Left Stick Vertical (`LV`)** | Bottom (-1000 / 0%) .. Top (+1000 / 100%) |
| **3** | `PA3` | **Left Stick Horizontal (`LH`)** | Left (-1000) .. Right (+1000) |
| **4** | `PA4` | **Switch SA** (Default: 2-Pos) | UP (-1000 / 988 µs) / DOWN (+1000 / 2012 µs) |
| **5** | `PA5` | **Switch SB** (Default: 3-Pos) | UP (-1000) / MID (0 / 1500 µs) / DOWN (+1000) |
| **6** | `PA6` | **Potentiometer VRA / VR1** | Left Jog Wheel: Left (-1000) .. Right (+1000) |
| **7** | `PA7` | **Potentiometer VRB / VR2** | Right Jog Wheel: Left (-1000) .. Right (+1000) |
| **8** | `PB0` | **Switch SC** (Default: 3-Pos) | UP (-1000) / MID (0 / 1500 µs) / DOWN (+1000) |
| **9** | `PB1` | **Switch SD** (Default: 2-Pos) | UP (-1000 / 988 µs) / DOWN (+1000 / 2012 µs) |
| **10** | `PC0` | **Battery Voltage Sense** | $10\text{ k}\Omega / 5.1\text{ k}\Omega$ divider: `(raw * 977064) / 409500` mV |
| **11** | `PC1` | **Ext Analog VRC / SWI** (`ADC_IN11`)| Left (-1000) .. Right (+1000) or Switch `SWI` |
| **12** | `PC2` | **Ext Analog VRD / SWJ** (`ADC_IN12`)| Left (-1000) .. Right (+1000) or Switch `SWJ` |
| **13** | `PC3` | **Ext Analog VRE / SWK** (`ADC_IN13`)| Left (-1000) .. Right (+1000) or Switch `SWK` |
| **14** | `PC4` | **Ext Analog VRF / SWL** (`ADC_IN14`)| Left (-1000) .. Right (+1000) or Switch `SWL` |

### Stick Mode Remapping (Mode 1 .. Mode 4)
Physical gimbals (`rh`, `rv`, `lv`, `lh`) are decoupled from logical primary flight controls (`aileron`, `elevator`, `throttle`, `rudder`) based on `Radio Setup -> Stick Mode:`:
- **Mode 1**: Right Vertical = Throttle, Right Horizontal = Roll, Left Vertical = Pitch, Left Horizontal = Yaw.
- **Mode 2 (Default)**: Left Vertical = Throttle, Left Horizontal = Yaw, Right Vertical = Pitch, Right Horizontal = Roll.
- **Mode 3**: Right Vertical = Throttle, Right Horizontal = Yaw, Left Vertical = Pitch, Left Horizontal = Roll.
- **Mode 4**: Left Vertical = Throttle, Left Horizontal = Roll, Right Vertical = Pitch, Right Horizontal = Yaw.

---

## 2. Universal Configurable ADC Input Modes (`AdcInputMode`)

All 10 auxiliary analog inputs (`SA..SD`, `VRA..VRB`, and extended `VRC..VRF` on `PC1..PC4`) can be individually configured in **`Radio Setup -> Inputs` (`INPUT MODES`)**:

| Mode Value | Enum Variant | UI Label | Behavior |
| :---: | :--- | :--- | :--- |
| **`0`** | `AdcInputMode::Default` | `DEFAULT` | Uses stock hardware assignment (`SA`/`SD` = 2-Pos, `SB`/`SC` = 3-Pos, `VRA`..`VRF` = Pot) |
| **`1`** | `AdcInputMode::TwoPos` | `2-POS` | 2-position switch (`< 2048` -> Down `+1000`, `>= 2048` -> Up `-1000`) |
| **`2`** | `AdcInputMode::ThreePos` | `3-POS` | 3-position switch (`> 3000` -> Up `-1000`, `1000..=3000` -> Mid `0`, `< 1000` -> Down `+1000`) |
| **`3`** | `AdcInputMode::SixPos` | `6-POS` | 6-position flight mode resistor ladder switch (`1..6`, scaled uniformly across `-1000..+1000`) |
| **`4`** | `AdcInputMode::Pot` | `POT` | Continuous potentiometer/slider without center detent (midpoint `(min + max) / 2`) |
| **`5`** | `AdcInputMode::PotDetent` | `POT-D` | Continuous potentiometer with calibrated center detent and acoustic center-crossing click (`2200 Hz`, 10 ms) |
| **`6`** | `AdcInputMode::InstantTrim`| `INST-TRIM`| Momentary switch triggering 3-axis Instant Trim capture on downward edge |

---

## 3. Gimbal Potentiometer Geometry & Filtering

FlySky FS-i6S gimbals sweep ~3400 ADC counts across mechanical stick movement:
- **Horizontal Axes (Roll `A`, Yaw `R`)**: `GIMBAL_H_HALF_SPAN = 1670`.
- **Vertical Axes (Pitch `E`, Throttle `T`)**: `GIMBAL_V_HALF_SPAN = 1580`.

### OpenTX Modified Moving Average (MMA) Jitter Filter
To remove potentiometer electrical jitter without introducing deadbands or control latency:
- If raw ADC change is >= 20 counts (stick actively in motion), the sample passes through immediately (**zero latency**).
- For micro-fluctuations (< 20 counts), an integer MMA filter (16x oversampling) smooths the reading:
  ```text
  filtered = filtered - prev + raw
  ```

### Battery Voltage Exponential Moving Average (EMA) Filter
Raw ADC measurements on `PC0` via the internal resistor divider exhibit ±10..20 mV of switching regulator ripple and noise. An integer fixed-point IIR filter ($\alpha = 1/32$, $\tau \approx 640\text{ ms}$) stabilizes the hundredths readout (`X.YYV`):
```text
EMA_k = EMA_{k-1} - (EMA_{k-1} >> 5) + (Sample_k << 3)
```

---

## 4. Digital Trim Subsystem & 3-Axis Instant Trim (`src/hw/touch/nav.rs`, `src/trim.rs`)

The FlySky FS-i6S replaces mechanical trim rockers with three complementary digital trim systems:

### 1. Stick Modifier Trim Mode (In-Flight Ergonomic Trimming)
When `Rear L` and/or `Rear R` are configured to `TRIMS MOD` in `Radio Setup`:
- **Hold Left Rear Button (`PA9`)**:
  - Deflect Left Stick Horizontally (threshold $> 350$ counts): Adjusts **Yaw Trim** (Left / Right).
  - Deflect Left Stick Vertically (threshold $> 350$ counts): Adjusts **Throttle Trim** (Up / Down).
- **Hold Right Rear Button (`PA10`)**:
  - Deflect Right Stick Horizontally (threshold $> 350$ counts): Adjusts **Roll Trim** (Left / Right).
  - Deflect Right Stick Vertically (threshold $> 350$ counts): Adjusts **Pitch Trim** (Up / Down).

### 2. Virtual Touch Perimeter Targets (Direct Screen Trimming)
When viewing the main flight dashboard, tap zones along the LCD borders provide direct single-step trimming:
- **Left Edge ($x < 16$)**: Throttle Trim Up ($y < 32$) / Throttle Trim Down ($y \ge 32$).
- **Right Edge ($x > 112$)**: Pitch Trim Up ($y < 32$) / Pitch Trim Down ($y \ge 32$).
- **Bottom Edge ($y > 52$)**:
  - Yaw Trim Left ($x < 36$) / Yaw Trim Right ($36 \le x < 64$).
  - Roll Trim Left ($64 \le x < 92$) / Roll Trim Right ($x \ge 92$).

### 3. Complete 3-Axis Instant Trim (`INSTANT TRIM`)
Triggered by pressing the Right Rear button (`PA10`, when `Rear R:` is set to `INSTANT TRIM`) or flipping any auxiliary switch configured as `INST-TRIM`:
- **Active Stick Deflection Capture**: Evaluates current Aileron (Roll), Elevator (Pitch), and Rudder (Yaw) stick deflections using symmetric nearest-step rounding (`(val ± 20) / 40`), adding the offsets directly into the model's trims (clamped to `±25`).
- **Neutral-Stick Reset**: Toggling Instant Trim while all three primary sticks are at neutral center (`0, 0, 0` steps) resets Roll, Pitch, and Yaw trims back to `0` and plays the center confirmation chime (`2800 Hz`).

### Trim Authority & Modes
- **Authority**: Each axis provides ±25 discrete steps (±100 µs total authority), repeating every **90 ms** if held, with automatic 2-second debounced Flash persistence when disarmed.
- **Selectable Throttle Trim Modes** (`Radio Setup -> Thr Trim:`):
  1. **`OFF (Lock)` (Default)**: Throttle trim is disabled; attempting to trim sounds a limit warning buzz (`1100 Hz`).
  2. **`IDLE (T-Trim)`**: OpenTX-style throttle trim (100% authority at idle, tapering to 0% at full throttle).
  3. **`LINEAR`**: Uniform trim (±100 µs) across the entire throttle throw.

---

## 5. Hardware Piezo Buzzer (`src/buzzer.rs`)

The piezo buzzer on pin `PA8` is driven by **`TIM1_CH1`** in hardware PWM Mode 1 (`PSC = 47`, 1.000 µs tick):
- **Power-On Welcome Fanfare**: 4-note ascending fanfare ($C_6 \to E_6 \to G_6 \to C_7$, 660 ms) under `ToneStyle::Rich`, or a single tactile click (15 ms) under `ToneStyle::Simple`.
- **Switch Toggle Beep (`Sw Sound`)**: Subtle acoustic feedback when toggling physical switches (configurable in `Radio Setup -> Sw Sound`).
- **Arming / Disarming Chimes**: 2-note rising chirp (`1800 Hz` -> `2400 Hz`) on motor arm, and falling chirp on disarm.
- **Pitch-Shifted Trim Step**: Tones scale dynamically with trim position (`1500 Hz` to `2500 Hz`), with `2800 Hz` center confirmation and `1100 Hz` limit buzz.

---

## 6. Touchscreen, Rear Buttons, & J4 Switch Expansion (`src/hw/touch/`, `src/hw/keys.rs`)

### Touch Gestures & Hitbox Zones
- **Swipe Gestures**: Swipe UP/DOWN (scroll/cycle pages), Swipe RIGHT (OK/Select), Swipe LEFT (Cancel/Back).
- **Tap Hitbox Zones in Menus**: Top Row ($y < 22$) = Up, Bottom Row ($y > 44$) = Down, Left Edge ($x < 32$) = Cancel, Right/Center = OK.
- **Flight Dashboard Center Tap**: Tapping the center area ($32 \le x \le 96$, $20 \le y \le 44$) opens the **Settings Menu**.

### Configurable Rear/Front Tactile Buttons (`PA9` / `PA10`)
- **In Menus**: `PA9` = Cancel/Back, `PA10` = OK/Select.
- **In Flight Mode**:
  - `Rear L (`PA9`)`: Configurable in `Radio Setup` as `RC SW (SWE)` (2-pos switch `SE`), `TIMER RESET`, or `TRIMS MOD`.
  - `Rear R (`PA10`)`: Configurable in `Radio Setup` as `RC SW (SWF)` (2-pos switch `SF`), `INSTANT TRIM`, or `TRIMS MOD`. Holding `PA10` >= 1.2s opens the Settings Menu.
- **Header `J4` Expansion (`PA13` / `PA14`)**: When `J4 Mode` is set to `SG/SH BTN`, `PA13` and `PA14` operate as active-LOW switches `SG` and `SH`.
