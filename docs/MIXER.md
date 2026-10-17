# FLIGHT CONTROL & 18-CHANNEL MIXING ARCHITECTURE

Comprehensive reference for input conditioning (Dual Rates & Exponential), auxiliary channel assignment, built-in wing and tail templates, and the EdgeTX/OpenTX-inspired 18-channel freeform matrix mixer on the FlySky FS-i6S.

---

## 1. Overview & Control Pipeline

The mixing system in `flysky-i6s-rs` implements an interrupt-safe, deterministic, zero-heap 4-stage pipeline that runs every main execution cycle (~500 Hz):

```mermaid
flowchart TD
    subgraph S1 ["Stage 1: Physical Acquisition (11 Channels @ 500 Hz)"]
        GIM["Gimbals: Roll, Pitch, Thr, Yaw (-1000..+1000)"]
        POTS["Pots: VRA, VRB (-1000..+1000)"]
        SW["Switches: SA..SD (Up / Mid / Down)"]
    end

    subgraph S2 ["Stage 2: Input Conditioning & Rates"]
        DR["Dual Rates: High / Low (30%..100%) via Switch"]
        EXPO["Cubic Integer Expo (-100%..+100%)"]
        TC["Catmull-Rom Cubic Spline Throttle Curve"]
    end

    subgraph S3 ["Stage 3: Mixing & Templates"]
        TPL["Aircraft Template: Normal / Elevon / V-Tail / Flaperon"]
        AUX["Aux Source Mapping (CH5..CH18)"]
        MIX["8 Freeform Mix Rules (Weight, Offset, Mode, Switch)"]
    end

    subgraph S4 ["Stage 4: Outputs & Protocol Frame"]
        TRIM["Digital Trims (Roll, Pitch, Thr, Yaw)"]
        REV["18-Channel Reversing Bitmask"]
        FAIL["Failsafe Supervisor (Hold vs Pre-set pulses)"]
        RF["AFHDS 2A 18-Channel Interleaved Frame (988..2012 µs)"]
    end

    S1 --> S2 --> S3 --> S4
```

---

## 2. Input Conditioning: Dual Rates & Cubic Exponential

Before raw gimbal movements reach the mixer matrix, primary flight controls (Aileron, Elevator, Rudder) are conditioned with user-configurable **Dual Rates (D/R)** and **Exponential Curves (EXPO)**.

### Mathematical Formulation (Integer Arithmetic)
To ensure deterministic execution without floating-point emulation overhead on the Cortex-M0 core, exponential curves are computed using an exact integer cubic polynomial.

Given a normalized stick input `x` in the range `[-1000, +1000]` and active rate percentage `R` in `[30, 100]%`:

```text
x_scaled = (x * R) / 100
```

Let the normalized cubic component be:

```text
x_cubic = (x_scaled^3) / 1,000,000   (in range [-1000, +1000])
```

1. **Positive Expo (Expo > 0): Softens Center Sensitivity**
   Decreases stick sensitivity around the neutral stick position for smooth scale flying, while retaining 100% mechanical throw at gimbal endpoints:
   ```text
   Output = (x_scaled * (100 - Expo) + x_cubic * Expo) / 100
   ```

2. **Negative Expo (Expo < 0): Heightens Center Sensitivity**
   Increases stick sensitivity near center for aggressive 3D aerobatic maneuvers:
   ```text
   Output = x_scaled + ((x_scaled - x_cubic) * |Expo|) / 100
   ```

Implemented in [`src/mixer.rs`](../src/mixer.rs#L44-L64).

### D/R Switch Assignment
In the **Dual Rate / Expo** menu, the pilot selects an assigned hardware switch:
- `None`: High rate always active.
- `SA`: 2-position switch (UP = High Rate, DOWN = Low Rate).
- `SB`: 3-position switch (UP = High Rate, MID/DOWN = Low Rate).
- `SC`: 3-position switch (UP = High Rate, MID/DOWN = Low Rate).
- `SD`: 2-position switch (UP = High Rate, DOWN = Low Rate).

Independent High and Low rates (30%..100%) and Expo (-100%..+100%) can be configured per channel (Roll, Pitch, and Yaw).

---

## 3. Wing & Tail Aircraft Templates

For standard aircraft configurations, the firmware provides pre-configured wing and tail templates that automatically route and mix the conditioned stick inputs into the output matrix:

### 1. Normal (Standard 4-Channel)
- **CH1**: Roll / Aileron (Right Gimbal Horizontal)
- **CH2**: Pitch / Elevator (Right Gimbal Vertical)
- **CH3**: Throttle (Left Gimbal Vertical with Catmull-Rom spline curve)
- **CH4**: Yaw / Rudder (Left Gimbal Horizontal)

### 2. Elevon / Delta Wing (Flying Wings & Pusher Jets)
Combines Elevator and Aileron into two elevon surfaces on the trailing edge of the wing:
```text
CH1 (Left Elevon)  = (Pitch - Roll) / 2
CH2 (Right Elevon) = (Pitch + Roll) / 2
```
- When pulling back on the pitch stick (Up Elevator), both surfaces raise together.
- When deflecting the roll stick right, the right elevon raises and left elevon lowers.

### 3. V-Tail (V-Tail Gliders & Scale Planes)
Combines Elevator and Rudder into two angled tail surfaces:
```text
CH2 (Left V-Tail)  = (Pitch + Yaw) / 2
CH4 (Right V-Tail) = (Pitch - Yaw) / 2
```
- **CH1**: Standard Aileron control.
- When pulling back on pitch, both V-tail surfaces deflect upward.
- When yawing right, the left surface moves up and right surface moves down.

### 4. Flaperon (Dual Ailerons with Integrated Flaps)
Controls two independent wing servos for full-span ailerons and camber/flap control:
```text
CH1 (Left Aileron)  = Roll + (Flap / 2)
CH6 (Right Aileron) = -Roll + (Flap / 2)
```
- Flap deployment is driven by the physical source assigned to **CH6** in the `Aux Channels` menu (e.g. switch `SB` for 3-position flaps, or rotary knob `VRA` for variable camber).

---

## 4. Auxiliary Channel Remapping (CH5..CH18)

All 14 auxiliary channels (`CH5` through `CH18`) can be independently assigned to any physical control on the radio from the `Aux Channels` menu:

| Source ID | Label | Description | Output Range |
| :--- | :--- | :--- | :--- |
| **0** | `None` | Neutral output (no input source) | 1500 µs |
| **1** | `Roll` | Conditioned Aileron stick input | 988..2012 µs |
| **2** | `Pitch`| Conditioned Elevator stick input | 988..2012 µs |
| **3** | `Thr`  | Spline-curved Throttle stick input | 988..2012 µs |
| **4** | `Yaw`  | Conditioned Rudder stick input | 988..2012 µs |
| **5** | `VRA`  | Left rotary dial / jog wheel | 988..2012 µs |
| **6** | `VRB`  | Right rotary dial / jog wheel | 988..2012 µs |
| **7..10** | `SA..SD` | Physical switches `SA`..`SD` (2-pos, 3-pos, 6-pos, or pot per `AdcInputMode`) | 988..2012 µs |
| **31..32**| `SE..SF` | Rear tactile buttons `PA9`/`PA10` (when configured as `RC SW`) | UP: 988 µs / DN: 2012 µs |
| **33..34**| `SG..SH` | Header `J4` tactile buttons `PA13`/`PA14` (when `J4 Mode` = `SG/SH BTN`) | UP: 988 µs / DN: 2012 µs |
| **35..38**| `VRC..VRF`| Extended analog pots on `PC1..PC4` (when `Ext ADC (PC)` enabled) | 988..2012 µs |
| **39..42**| `SWI..SWL`| Extended switches on `PC1..PC4` (2-pos or 3-pos via `AdcInputMode`) | 988 / 1500 / 2012 µs |

### Factory Default Auxiliary Mapping
- `CH5`: **SA** (Arming switch / Flight mode)
- `CH6`: **SB** (3-position flight modes: Angle / Horizon / Acro)
- `CH7`: **VRA** (Gimbal tilt / Camber flap)
- `CH8`: **VRB** (Payload release / Volume)
- `CH9`: **SC** (3-position auxiliary: Beeper / OSD switch)
- `CH10`: **SD** (Throttle cut / Trainer switch)
- `CH11..CH18`: `None` (Centered 1500 µs)

---

## 5. Freeform Matrix Mixer Engine

For custom aircraft geometries, complex sailplanes, multi-engine craft, and safety interlocks, every model profile includes **8 freeform mix lines** (`Mix 1` through `Mix 8`).

### Mix Rule Parameters
Each mix line comprises a compact 6-byte struct stored in Flash memory:

```rust
pub struct MixLine {
    pub target_ch: u8,   // 0: Disabled, 1..18: Target Channel (CH1..CH18)
    pub source: u8,      // 0: None, 1..4: AETR, 5..6: VRA/VRB, 7..10: SA..SD, 11: MAX, 12..29: CH1..CH18, 30: Thr+, 31..42: SE..SWL
    pub weight: i8,      // -100% .. +100% (gain / authority)
    pub offset: i8,      // -100% .. +100% (center shift)
    pub switch: u8,      // 0: Always ON, 1..26: Physical switch position condition (SA^..SWLv)
    pub mode: u8,        // 0: ADD (+), 1: MULTIPLY (*), 2: REPLACE (:=)
}
```

### Multiplex Modes
When a mix line is active, it modifies the target channel based on its configured mode:
1. **`ADD (+)`**: Adds the weighted source to the current channel value:
   ```text
   Channel_new = Channel_current + ((Source * Weight) / 100) + (Offset * 10)
   ```
2. **`MULTIPLY (*)`**: Scales the current channel value by the source (e.g. gain knobs, variable differential):
   ```text
   Channel_new = (Channel_current * Term) / 1000
   ```
3. **`REPLACE (:=)`**: Overrides and replaces the target channel completely (e.g. throttle cut, emergency level/rescue switch):
   ```text
   Channel_new = ((Source * Weight) / 100) + (Offset * 10)
   ```

### Switch Conditions
A mix line can be gated by any hardware switch condition (`0` = `ON`, `1..10` = `SA^`..`SDv`, `11..14` = `SE^`..`SFv`, `15..18` = `SG^`..`SHv`, `19..26` = `SWI^`..`SWLv`). Flipping any physical switch while editing the `Switch:` field auto-detects and selects that switch condition immediately.

---

## 6. Practical EdgeTX / OpenTX Mixer Recipes

Here are proven mixer setups used by RC pilots:

### Recipe 1: Throttle Cut Safety Switch
*Prevents accidental motor start by locking CH3 to minimum pulse (988 µs) whenever switch `SD` is flipped DOWN:*
- **Target**: `CH3`
- **Source**: `MAX`
- **Weight**: `-100%`
- **Offset**: `0%`
- **Switch**: `SDv`
- **Mode**: `REPLACE (:=)`

### Recipe 2: Throttle-to-Elevator Pitch Compensation (Unipolar `Thr+`)
*High-thrust airplanes often balloon upward when applying full throttle. This mix automatically injects a small amount of down-elevator as throttle increases without introducing negative elevator deflection at idle:*
- **Target**: `CH2` (Elevator)
- **Source**: `Thr+` (Unipolar half-range throttle: 0 at idle, linearly ramping to +1000 at full throttle)
- **Weight**: `-10%`
- **Offset**: `0%`
- **Switch**: `ON`
- **Mode**: `ADD (+)`

### Recipe 3: Twin-Engine Differential Thrust
*Twin-motor planes or boats can spin tightly using motor thrust mixed with rudder:*
- **CH3 (Left Motor)**: Driven by normal throttle.
- **CH5 (Right Motor)**: In `Aux Channels`, set source to `Thr`.
- **Mix 1 (Left Motor Yaw Mix)**:
  - Target: `CH3`, Source: `Yaw`, Weight: `+20%`, Mode: `ADD (+)`
- **Mix 2 (Right Motor Yaw Mix)**:
  - Target: `CH5`, Source: `Yaw`, Weight: `-20%`, Mode: `ADD (+)`

### Recipe 4: Crow / Butterfly Sailplane Airbrakes
*For high-performance gliders: deploys flaps full down, raises both ailerons up, and feeds in down-elevator compensation to dive without gaining speed:*
- **Template**: `Flaperon` (CH1 Left Aileron, CH6 Right Aileron).
- **Mix 1 (Flap-to-Aileron Crow)**:
  - Target: `CH1`, Source: `VRA`, Weight: `+40%`, Mode: `ADD (+)`
- **Mix 2 (Elevator Compensation)**:
  - Target: `CH2`, Source: `VRA`, Weight: `-15%`, Mode: `ADD (+)`

---

## 7. Storage & Flash Memory Budget

All flight mixer parameters are stored directly inside each 128-byte [`ModelConfig`](../src/storage.rs) structure in the append-only sequential storage log (Keys 1..20 across Flash Pages 60–63 at `0x0801_E000`):

| Parameter | Type | Bytes | Offset in Model Profile |
| :--- | :--- | :--- | :--- |
| `channel_reverse` | `u32` | 4 | Offset 20..24 |
| `dr_switch` | `u8` | 1 | Offset 24 |
| `dr_high` | `[u8; 3]` | 3 | Offset 36..39 |
| `dr_low` | `[u8; 3]` | 3 | Offset 39..42 |
| `expo_high` | `[i8; 3]` | 3 | Offset 42..45 |
| `expo_low` | `[i8; 3]` | 3 | Offset 45..48 |
| `aux_channels` | `[u8; 14]` | 14 | Offset 54..68 |
| `wing_tail_mix` | `u8` | 1 | Offset 68 |
| `template_diff` | `i8` | 1 | Offset 69 |
| `mixes` | `[MixLine; 8]` | 48 | Offset 70..118 |

Total `ModelConfig` size: **exactly 128 bytes** (100% backward and forward compatible with v4-to-v5 sequential storage migration).
