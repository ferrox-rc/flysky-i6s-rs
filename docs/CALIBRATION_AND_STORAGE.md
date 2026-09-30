# STICK CALIBRATION & FLASH PERSISTENCE

Technical documentation for the interactive 2-step calibration wizard and the non-volatile Flash configuration storage on the FlySky FS-i6X.

---

## 1. Flash Storage Layout (`src/storage.rs`)

Configuration and model memories are stored in an 8 KB append-only log across the final four 2 KB sectors of the microcontroller's 128 KB internal Flash:
- **Flash Storage Range:** `0x0801_E000 .. 0x0802_0000` (Pages 60, 61, 62, & 63 of STM32F072VB).
- **Total Sector Size:** 8,192 bytes (4 × 2048-byte pages).
- **Storage Engine:** Log-structured append-only Key-Value map powered by the `sequential-storage` crate (`sequential_storage::map`).
- **Key-Value Mapping:**
  - **Key `0`:** Global radio configuration (`RadioConfig`, 128 bytes).
  - **Keys `1..=20`:** Model memory slots 1 through 20 (`ModelConfig`, 128 bytes each).
- **Append-Only Write Performance:**
  - Incremental updates write only ~132 bytes to the active flash log page in **~2.8 ms** (measured on hardware).
  - **Zero page erases** during routine operation: editing trims, curves, or model settings causes zero perceptible UI stutter or audio glitch.
  - **Automatic Wear-Levelled Compaction:** When all 4 pages become full of historical revisions, `sequential-storage` automatically compacts current active records into a newly erased page, rotating evenly across Pages 60–63.
  - **Watchdog Protection:** During multi-page compaction routines, the hardware watchdog (`pac::IWDG`) is explicitly fed between page erase cycles, preventing 2.0s resets.

### Storage Structures (`RadioStorage` v3)

All storage structures are compiled with strict C-compatible alignment (`#[repr(C)]`) and compile-time size assertions (`core::mem::size_of`):

```rust
pub const FLASH_MAGIC: u32 = 0x4653_4B59; // "FSKY"
pub const CONFIG_VERSION: u32 = 3;
pub const NUM_MODELS: usize = 20;

#[repr(C)]
pub struct ChannelCalib {
    pub min: u16,      // Minimum endpoint (raw ADC counts)
    pub center: u16,   // Neutral resting center (raw ADC counts)
    pub max: u16,      // Maximum endpoint (raw ADC counts)
    pub _pad: u16,     // 32-bit alignment padding
}

/// Global radio settings (exactly 128 bytes)
#[repr(C)]
pub struct RadioConfig {
    pub magic: u32,                // 0x4653_4B59 ("FSKY")
    pub version: u32,              // Config structure version (3)
    pub active_model: u8,          // Current model index (0..19)
    pub throttle_trim: u8,         // 0: OFF (Lock), 1: IDLE (T-Trim), 2: LINEAR
    pub audio_enabled: u8,         // 0: Muted, 1: Enabled
    pub backlight_timeout: u8,     // 0: Always On, 1: 15s, 2: 30s, 3: 60s
    pub backlight_brightness: u8,  // 1..10 (10%..100%, default 10)
    pub vbat_warn_deci: u8,        // 40..50 (4.0V..5.0V, default 44 = 4.4V)
    pub lcd_contrast: u8,          // 15..55 (default 37 / 0x25)
    pub usb_mode: u8,              // 0: Joystick, 1: Serial, 2: Composite, 3: Off
    pub sticks: [ChannelCalib; 4], // 0: Roll, 1: Pitch, 2: Throttle, 3: Yaw (32 bytes)
    pub pots: [ChannelCalib; 2],   // 0: VRA, 1: VRB (16 bytes)
    pub _reserved: [u8; 64],       // Reserved expansion space (Total: 128 bytes)
}

/// A single freeform mix rule in the matrix mixer (6 bytes)
#[repr(C)]
pub struct MixLine {
    pub target_ch: u8,   // 0: Disabled, 1..14: Target Channel CH1..CH14
    pub source: u8,      // 0: None, 1: Roll, 2: Pitch, 3: Thr, 4: Yaw, 5: VRA, 6: VRB, 7..10: SA..SD, 11: MAX, 12..25: CH1..CH14
    pub weight: i8,      // -100% .. +100%
    pub offset: i8,      // -100% .. +100%
    pub switch: u8,      // 0: Always On, 1..10: Switch condition
    pub mode: u8,        // 0: Add (+), 1: Multiply (*), 2: Replace (:=)
}

/// Per-model profile configuration (exactly 128 bytes)
#[repr(C)]
pub struct ModelConfig {
    pub name: [u8; 10],            // 10-char ASCII model name (e.g. "QUAD 5IN  ")
    pub model_type: u8,            // 0: Airplane, 1: Glider, 2: Helicopter, 3: Multirotor / Quad
    pub _pad0: u8,                 // Align rx_id
    pub rx_id: u32,                // Bound receiver ID (Model Match)
    pub trims: [i8; 4],            // -25 .. +25 (Roll, Pitch, Throttle, Yaw)
    pub channel_reverse: u16,      // 14-bit channel reversing mask (bit 0=CH1 .. bit 13=CH14)
    pub dr_switch: u8,             // 0: None, 1: SA, 2: SB, 3: SC, 4: SD
    pub thr_curve_pts: u8,         // 5 or 9 points
    pub thr_curve_smooth: u8,      // 0: Linear interpolation, 1: Catmull-Rom spline
    pub thr_curve: [u8; 9],        // Throttle curve points (0..100%)
    pub dr_high: [u8; 3],          // High rates: AIL, ELE, RUD (50..100%)
    pub dr_low: [u8; 3],           // Low rates: AIL, ELE, RUD (30..100%)
    pub expo_high: [i8; 3],        // High expo (-100..+100%)
    pub expo_low: [i8; 3],         // Low expo (-100..+100%)
    pub timer_secs: u16,           // 46..48: Countdown timer in seconds (e.g. 300 = 5 min)
    pub timer_source: u8,          // 48: 0: Off, 1: THs (Thr>5%), 2: THt (Thr Latched), 3: Always On, 4..13: SA^..SDv
    pub protocol_subtype: u8,      // 49: 0: PWM, 1: PPM, 2: i-BUS, 3: S.BUS
    pub failsafe_thr: u16,         // 50..52: Failsafe throttle pulse in µs (e.g. 1000)
    pub aux_channels: [u8; 10],    // 52..62: Source for CH5..CH14
    pub wing_tail_mix: u8,         // 62: 0: Normal, 1: Elevon/Delta, 2: V-Tail, 3: Flaperon
    pub template_diff: i8,         // 63: Differential / mix ratio (-100..+100)
    pub mixes: [MixLine; 8],       // 64..112: 8 freeform mix rules (8 * 6 = 48 bytes)
    pub failsafe_mode: u8,         // 112: 0: Hold last, 1: Custom pulses
    pub failsafe_timeout: u8,      // 113: 10..50 (1.0s..5.0s)
    pub rf_protocol: u8,           // 114: 0: AFHDS 2A, 1: CRSF / ELRS
    pub crsf_baud: u8,             // 115: 0: 420k, 1: 416.6k, 2: 115.2k, 3: 921.6k
    pub arm_switch: u8,            // 116: 0: None, 1: SA^, 2: SAv, 3: SB^, 4: SB-, 5: SBv, 6: SC^, 7: SC-, 8: SCv, 9: SD^, 10: SDv
    pub _reserved: [u8; 11],       // 117..128: 11 reserved bytes (Total: 128 bytes)
}

/// Unified Flash image layout (exactly 2,688 bytes in memory)
#[repr(C)]
pub struct RadioStorage {
    pub radio: RadioConfig,                // 128 bytes
    pub models: [ModelConfig; NUM_MODELS], // 20 * 128 = 2,560 bytes
}
```

### Pure PAC Flash Hardware Driver (`FlashStorage`)
The underlying flash driver implements `embedded_storage::nor_flash::NorFlash` and `MultiwriteNorFlash` directly using the STM32 PAC (`stm32f0::stm32f0x2::pac::FLASH`):
- **Unlock Sequence:** Unlocks flash control registers via `FLASH_KEYR` (`0x4567_0123`, `0xCDEF_89AB`).
- **Register Hygiene (`write_with_zero`):** Uses `write_with_zero` on `FLASH_CR` to prevent inadvertent re-assertion of the `LOCK` bit when setting `PER` (Page Erase) or `PG` (Programming).
- **Error Flag Clearing:** Clears pending `EOP`, `WRPRTERR`, and `PGERR` flags in `FLASH_SR` before initiating erase or write commands.
- **16-Bit Halfword Programming:** Programs data in halfword increments with hardware `BSY` polling and post-write verification.

### Automatic Migration & Backward Compatibility
The configuration loader follows an automatic multi-tier fallback:
1. **Sequential Storage Check**: Probes the 4-page log across Pages 60–63 (`0x0801_E000`). If valid keys exist, loads radio config (Key 0) and model profiles (Keys 1..20).
2. **Legacy v3 Snapshot Migration**: If sequential storage is empty, probes the old snapshot base at Page 62 (`0x0801_F000`) for `magic == 0x4653_4B59`. If found, migrates `RadioConfig` and all 20 `ModelConfig` records into the sequential log.
3. **Legacy v1/v2 Migration**: If Page 62 is unprogrammed, probes Page 63 (`0x0801_F800`) for legacy signatures:
   - Copies existing stick and pot calibration into `storage.radio`.
   - Copies existing `rx_id` into Model 01.
   - Copies radio preferences (audio, backlight, contrast, etc.).
   - Migrates the structure into sequential storage.
4. **Factory Default Fallback**: If no valid signature is found anywhere, initializes clean default calibrations, creates default model names (`MODEL 01` through `MODEL 20`), sets standard 5-point linear throttle curves (`[0, 25, 50, 75, 100]`), and commits Key 0 through Key 20 to the append-only log.

---

## 2. Endpoint Calibration Wizard (`src/calib.rs`)

The calibration wizard provides an interactive on-screen workflow to measure the true physical limits of the transmitter gimbals and rotary dials:

```mermaid
stateDiagram-v2
    [*] --> Inactive : Flight Screen
    Inactive --> Step1_Center : Hold OK for 1.2s (or on boot)
    
    state Step1_Center {
        [*] --> ReleaseGuard1 : Wait for OK Release
        ReleaseGuard1 --> WaitingForOK : Center sticks & pots (Throttle to 50%)
        WaitingForOK --> CaptureCenter : Press OK
    }
    
    Step1_Center --> Step2_Limits : Tone & Transition
    Step1_Center --> Inactive : Press Cancel (Exit without saving)
    
    state Step2_Limits {
        [*] --> ReleaseGuard2 : Wait for OK Release
        ReleaseGuard2 --> Tracking : Stir sticks in circles & turn pots
        Tracking --> Tracking : Extent bars fill & OK checks appear
        Tracking --> SaveToFlash : Press OK (when all 4 sticks are OK)
    }
    
    Step2_Limits --> Inactive : Press Cancel (Exit without saving)
    
    SaveToFlash --> Inactive : Success Chime & Return to Flight Screen
```

---

## 3. Calibration Steps Explained

### Step 1 (Center)
1. **Key Release Guard**: When entering the wizard from the flight dashboard by holding `OK` for 1.2s, the wizard requires `OK` to be physically released before accepting input. This prevents accidentally skipping Step 1.
2. **Neutral Reference**: The user leaves Roll, Pitch, and Yaw spring-centered, manually positions the friction Throttle stick to the physical middle (50%), and centers VRA/VRB.
3. **Capture**: Pressing `[OK]` records `centers[0..5] = raw[0..5]` and initializes `mins` and `maxs` to the center reference.

### Step 2 (Limits & Margins)
1. **Dynamic Tracking**: The user moves both sticks in full circles touching all 4 corners, and rotates VRA & VRB from stop to stop.
2. **Real-Time Display**:
   - **Left side**: 4 stick gauges (`A`, `E`, `T`, `R`) with symmetric 30-pixel inner spans (`x = 11..41` left, `x = 41..71` right). Extent fill lines and live cursor ticks track the full throw. Once an axis has moved >= 250 counts in both directions, its status changes from `--` to `OK`.
   - **Right side**: Live gauges for `V1` (VRA) and `V2` (VRB) with 16-pixel extent fill lines and `OK` status indicators.
3. **Potentiometer Physics & ADC Range**:
   - The STM32 12-bit ADC spans `0..4095` counts (0V..3.3V).
   - However, standard rotary potentiometers have an electrical rotation angle of ~270°, whereas transmitter gimbals physically only tilt ±25° (a total travel of ~50°).
   - Consequently, the physical wiper only traverses ~20%..25% of the resistive element, producing raw ADC counts between ~380 and ~3720. The hardware physically cannot output 0 or 4095.
   - The calibration gauges scale dynamically to this physical travel (rather than fixed full-rail 0..4095), allowing the fill and cursors to cleanly reach the outer edges of the screen boxes at physical stops.
4. **Tolerance Margin Application**:
   When `[OK]` is pressed, the wizard applies OpenTX standard margin (`STICK_TOLERANCE = 64`):
   ```text
   effective_min = center - ((center - min) * 63) / 64
   effective_max = center + ((max - center) * 63) / 64
   ```
   This ~1.6% margin ensures ±100% (and 0% / 100% for throttle) is reliably reached right at the gimbal bezel without straining the gimbal arms.
5. **Commit**: Updates active runtime calibration in `input::apply_calibration`, writes `RadioConfig` to Flash, plays the 2-tone success chime, and displays `CALIBRATION SAVED!`.

---

## 4. Operational Shortcuts

- **Hold `OK` for 1.2s** on the flight dashboard: launches calibration.
- **Hold `OK` during Power-On**: launches calibration immediately on boot.
- **Press `Cancel` (`ESC`)**: aborts calibration at any stage without changing saved values.

---

## 5. SRAM Budgeting & Zero-Alloc In-Place Loading

The STM32F072VB microcontroller features **16 KB (16,384 bytes) of internal SRAM** (`0x2000_0000` .. `0x2000_4000`). Because the stack grows downward from `0x2000_4000` while static `.data` and `.bss` variables grow upward from `0x2000_0000`, strict stack budgeting is required:

### Stack vs. Static Safety Architecture
- **Targeted Key Reads (`load_radio_config`)**: Routines requiring only system settings read Key 0 (128 bytes) directly from the sequential storage log without loading the 20-model array.
- **Granular Model Profile Loading (`load_model_config`)**: Individual model slots are read directly by key (Keys 1..20) into caller-provided references.
- **In-Place Storage Loading (`load_storage_into`)**: Rather than returning a 2,688-byte `RadioStorage` struct by value on the stack, storage routines populate caller-provided references directly (`storage: &mut RadioStorage`).
- **Direct Receiver ID Extraction**: The AFHDS 2A driver directly loads the active model's `rx_id` from sequential storage without allocating temporary buffers.
- **BSS Relocation**: Volatile runtime caches (such as ExpressLRS parameter cache `CONFIG_ENGINE`) are zero-initialized in `.bss` rather than occupying `.data`.
- **Memory Margins**:
  - Total static RAM (`.data` + `.bss`): **~3,004 bytes** (terminates at `0x2000_0bb0`).
  - Total stack consumption during deepest boot call: **~7,200 bytes** (stack lowest point: `0x2000_23f4`).
  - **Guaranteed safety buffer**: **> 6.3 KB of unallocated headroom**, preventing any risk of stack collision with static stick calibration structures.
