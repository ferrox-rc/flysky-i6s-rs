# FLYSKY FS-I6X USER GUIDE & OPERATIONS MANUAL

A comprehensive guide to operating the `flysky-i6x-rs` firmware on the FlySky FS-i6X transmitter.

---

## 1. Physical Controls & Faceplate Layout

```
                  +-----------------------------------+
                  |         FlySky FS-i6X (Rust)      |
                  |                                   |
                  |  [SW A]   [SW B]  [SW C]   [SW D] |
                  |  (2-pos) (3-pos) (3-pos)  (2-pos) |
                  |                                   |
                  |       (VRA/VR1)     (VRB/VR2)     |
                  |       Left Dial     Right Dial    |
                  |                                   |
                  |    [LEFT GIMBAL]   [RIGHT GIMBAL] |
                  |    Throttle (V)    Pitch / ELE (V)|
                  |    Yaw / RUD (H)   Roll / AIL (H) |
                  |                                   |
                  |     [T-TRIM]          [E-TRIM]    |
                  |     [R-TRIM]          [A-TRIM]    |
                  |                                   |
                  |            [ ST7567 LCD ]         |
                  |              128x64 Mono          |
                  |                                   |
                  |  [BIND]    [UP]    [DOWN]    [OK] |
                  |  (Direct)          [CANCEL / ESC] |
                  +-----------------------------------+
```

### Keypad & Navigation Buttons
- **`[UP]`** / **`[DOWN]`**:
  - **On Flight Screen**: Cycles through the 4 dashboard pages (`1/4` -> `2/4` -> `3/4` -> `4/4` with `[DOWN]`, and reverse with `[UP]`).
  - **In Menus & Editors**: Navigate menu items, cycle characters, increment/decrement values.
  - **Auto-Repeat**: Holding **`[UP]`** or **`[DOWN]`** for >= 300 ms automatically repeats every **70 ms** for rapid scrolling through lists, swift character selection, and fast curve point editing.
- **`[OK]`**: Enter submenu, toggle setting, confirm values, advance character cursor in naming editor.
  - **Hold `[OK]` for 1.2 seconds** on any flight dashboard page: Opens the **Settings Menu**.
  - **Hold `[OK]` during Power-On**: Launches **Stick Calibration** immediately.
- **`[CANCEL]` (`[ESC]`)**:
  - **On Flight Screen (Hold >= 1.0s)**: Opens the **HUD Timer Reset Modal** with a real-time progress bar. Holding for 1.0s resets the flight timer back to its configured duration (or 0:00 for count-up), sounds a high chime, and flashes a `"TIMER RESET!"` confirmation. Releasing early aborts without resetting.
  - **In Menus**: Return to previous screen, exit edit mode, abort calibration, or complete one-way receiver binding.
- **`[BIND]` (Dedicated Button with Clean Separation Logic)**:
  - **Tap (`< 1.0s`) on Flight Screen**: Cycles through the 4 flight dashboard pages (`1/4` -> `2/4` -> `3/4` -> `4/4` -> `1/4`).
  - **Hold (`>= 1.0s`) on Flight Screen**: Initiates AFHDS 2A receiver binding.
  - **Hold during Power-On**: Launches AFHDS 2A binding subprogram immediately at boot.
  - **In Menus & Editors**: Functions as **`[TAB]` / Cursor Advance** (advances name characters or curve points) without triggering RF binding.

---

## 2. Multi-Page Flight Dashboard

The main flight screen features 4 switchable display pages cycled by tapping **`[BIND]`**. All 4 pages share a pixel-perfect uniform layout:
- **Top Status Bar (`y = 0..10`)**: Displays active model name, RF/telemetry status, and steady filtered battery voltage (`X.YYV`).
- **Top Divider (`y = 11`)**: Full-width horizontal line (`Line(0, 11) -> (127, 11)`).
- **Content Area (`y = 12..54`)**: Page-specific controls, gauges, and telemetry.
- **Bottom Divider (`y = 55`)**: Full-width horizontal line (`Line(0, 55) -> (127, 55)`).
- **Footer Info Bar (`y = 56..63`)**: Small text (`FONT_4X6`) rendered at baseline 62 (`y = 57..62`) for maximum vertical clearance.

### Page 1/4: Primary Gimbals & Trims
```
+-------------------------------------------------------------+
| MODEL 01                  RF:OK                     5.18V   | <- Status Bar (y=0..10)
|-------------------------------------------------------------| <- Top Line (y=11)
| A [====|==.======]  +15%   | E [========.=|==]   -22%       |
| T [========.     ]   45%   | R [====|==.======]    0%       |
| A:U  B:M  C:D  D:U                             V: 5/ 8      | <- Switches & Pots (y=44..53)
|-------------------------------------------------------------| <- Bottom Line (y=55)
| P1/4                         04:32            Hold OK:Menu  | <- Footer Bar (y=57..62)
+-------------------------------------------------------------+
```
- **Top Status Bar (y = 0..10)**:
  - **Left**: Active model name (up to 10 characters, e.g. `MODEL 01`).
  - **Center (`RF:OK` / `R: XX%` / `BIND` / `U:SIM` / `NO RF` / `E:XX`)**: RF link state, binding status, downlink telemetry RSSI, or **`U:SIM`** indicating active USB Simulator mode with silent RF standby.
  - **Right (`X.YYV`)**: Internal battery voltage stabilized by an exponential moving average (EMA) filter to eliminate switching jitter on the hundredths digit.
- **Gimbal Gauges (y = 12..43)**: Live channel sliders for Roll (`A`), Pitch (`E`), Throttle (`T`), Yaw (`R`) with center ticks, trim position ticks (`.`), and percentage readouts.
- **Switches & Pots Line (y = 44..53)**: Position of switches SA..SD (`U`=Up, `M`=Middle, `D`=Down) and rotary pots VRA/VRB (`0`..`9`), positioned cleanly above the line 55 divider.
- **Bottom Footer (y = 57..62)**: Displays active trim adjustment (`TRM A:+04`), or a 3-zone status bar with page index (`P1/4`), **Live Flight Timer** (e.g. `04:32`, inverted/blinking on expiration), and menu hint (`Hold OK:Menu`) in crisp small font (`FONT_4X6`).

### Page 2/4: 14-Channel Dual Column Monitor
```
+-------------------------------------------------------------+
| MODEL 01                  RF:OK                     5.18V   | <- Status Bar (y=0..10)
|-------------------------------------------------------------| <- Top Line (y=11)
|  1: [==========] 1500  |   8: [==========] 1500             |
|  2: [==========] 1500  |   9: [==========] 1500             |
|  3: [====      ] 1200  |  10: [==========] 1500             |
|  4: [==========] 1500  |  11: [==========] 1500             |
|  5: [          ] 1000  |  12: [==========] 1500             |
|  6: [==========] 1500  |  13: [==========] 1500             |
|  7: [==========] 1500  |  14: [==========] 1500             |
|-------------------------------------------------------------| <- Bottom Line (y=55)
| P2/4                           14-CH MONITOR                | <- Footer Bar (y=57..62)
+-------------------------------------------------------------+
```
- Real-time graphic bars and microsecond pulse readouts (1000..2000 µs) across all 14 AFHDS 2A channels simultaneously.
- Left column: CH 1..7 (Gimbals, SwA, SwB, VR1).
- Right column: CH 8..14 (VR2, SwC, SwD, Aux channels).
- Graphic bars are positioned at `y + 1` for pixel-perfect horizontal centering with the text labels, leaving 2px clearance above the line 55 divider.

### Page 3/4: Model Dashboard
```
+-------------------------------------------------------------+
| P3/4                         MODEL DASHBOARD                | <- Footer Bar (y=57..62)
+-------------------------------------------------------------+
```
- Full 10-character model name and synchronized model type (`AIRPLANE`, `GLIDER`, `HELI`, `QUAD`).
- Bound receiver 32-bit hex ID (`RxID`).
- Active throttle curve configuration (`5-PT` / `9-PT`, `LINEAR` / `SMOOTH`).
- Live telemetry readouts: Downlink RSSI percentage and receiver pack voltage (`RX: X.XXV`), or `AFHDS2A: DISCONNECTED` fitted within the screen width.

### Page 4/4: Dedicated Telemetry & RF Diagnostics
```
+-------------------------------------------------------------+
| MODEL 01                  RF:OK                     5.18V   | <- Status Bar (y=0..10)
|-------------------------------------------------------------| <- Top Line (y=11)
| RSSI: 98%              | RX:   5.12V                        |
| LINK: OK               | TX:   5.18V                        |
| TX:    1420            | mRSS:   92%                        |
| RX:    1398            | mRX:  4.92V                        |
|-------------------------------------------------------------| <- Bottom Line (y=55)
| P4/4                      TELEMETRY SENSORS                 | <- Footer Bar (y=57..62)
+-------------------------------------------------------------+
```
- **Live Signal & Diagnostics (Left Column)**:
  - `RSSI: XX%`: Real-time signal strength from receiver telemetry.
  - `LINK: OK / DISC`: Binary link status indicator.
  - `TX: XXXXX`: Total 2.4 GHz RF packets transmitted since boot.
  - `RX: XXXXX`: Total telemetry frames successfully acknowledged by receiver.
- **Power & Session Extremes (Right Column)**:
  - `RX: X.XXV`: Live flight receiver / BEC battery voltage.
  - `TX: X.XXV`: Live transmitter battery voltage.
  - `mRSS: XX%`: Lowest RSSI recorded during the active session (identifies signal dips or edge-of-range events).
  - `mRX: X.XXV`: Lowest flight pack voltage recorded (identifies brownout risks and servo sag).

---

## 3. Digital Trims & Audio Feedback

All 4 primary axes have dedicated digital rocker switches providing ±25 steps of trim authority (±100 µs):

### Trim Operation & Tones
- **Single Press**: Nudges trim by 1 step (4 µs). A short tone sounds.
- **Pitch Shift**: Tone pitch rises as trim increases (1500..2500 Hz), giving instant acoustic feedback of direction.
- **Center Return**: When passing through `0` (neutral), a distinctive high-pitched double-length tone (`2800 Hz`) sounds.
- **End of Travel**: Attempting to move past ±25 sounds a low-frequency warning buzz (`1100 Hz`).
- **Auto-Repeat**: Holding any trim switch for >= 350 ms automatically repeats steps at 90 ms intervals.

### Throttle Trim Safety Modes
Configurable in `Radio Setup`:
1. **`OFF (Lock)` (Recommended for Betaflight / INAV / Multirotors)**:
   - Throttle trim buttons are locked out. Pressing them sounds a warning tone without modifying output.
   - Prevents accidental disarm failures or motor spin-ups caused by bumping the throttle trim.
2. **`IDLE` (Traditional Glow / Nitro Engines)**:
   - Throttle trim only affects the lower half of the throttle stick range (1000..1500 µs), leaving maximum full-throttle output unchanged at 2000 µs.
3. **`LINEAR` (Electric Aircraft)**:
   - Throttle trim shifts the entire 1000..2000 µs range symmetrically.

---

## 4. Menu Navigation & Subsystem Breakdown

Hold **`[OK]` for 1.2 seconds** from the main flight screen to open the Settings Menu.

```
+------------------------------------+
| SETTINGS MENU                      |
|------------------------------------|
| > 1. MODEL SELECT                  |
|   2. MODEL SETUP                   |
|   3. DUAL RATE/EXPO                |
|   4. THR CURVE                     |
|   5. WING/MIXER                    |
|   6. AUX CHANNELS                  |
|   7. CH REVERSE                    |
|   8. RADIO SETUP                   |
|   9. RX SETUP & BIND               |
|  10. CHANNEL MONITOR               |
|  11. CALIBRATION                   |
|  12. ANALOG DIAG                   |
|  13. SYSTEM INFO                   |
|------------------------------------|
| [UP/DN] Move  [OK] Sel  [ESC] Exit |
+------------------------------------+
```

### Submenu 1: Model Select (`MODEL SELECT`)
- Displays all 20 model memory slots (`M01` through `M20`).
- The currently loaded model is marked with `[*]`.
- Use **`[UP]`** / **`[DOWN]`** to scroll through models; press **`[OK]`** to activate.
- **Real-Time Switching**: Switching models immediately applies the selected model's trims, channel reversing mask, throttle curve, and receiver ID.

### Submenu 2: Model Setup (`MODEL SETUP`)
- **Field 0: Name Editor**: 10-character ASCII model name (e.g. `QUAD 5IN  `, `TRAINER   `, `FOAMY 3D `).
  - Press **`[OK]`** to enter editing mode.
  - Use **`[UP]`** / **`[DOWN]`** to cycle characters (`A-Z`, `0-9`, `-`, `_`, space).
  - Press **`[OK]`** or **`[BIND]`** to confirm current character and advance cursor to next character.
  - Advancing past character 10 confirms the entire name and moves focus to Field 1 (`Type`).
  - Press **`[CANCEL]` (`[ESC]`)** at any time to finish editing name and return to field selection.
- **Field 1: Model Type**:
  - Press **`[OK]`** to cycle between `AIRPLANE`, `GLIDER`, `HELI`, and `QUAD`.
- **Field 2: Arm Switch (`Arm Sw:`)**:
  - Assign any physical switch and position condition as the model's arming switch: **`NONE`**, **`SA^`**, **`SAv`**, **`SB^`**, **`SB-`**, **`SBv`**, **`SC^`**, **`SC-`**, **`SCv`**, **`SD^`**, or **`SDv`**.
  - **Switch Auto-Detection**: Simply toggle the desired physical switch while editing to auto-detect and select it.
  - **Flight Safety & Acoustic Feedback**: Arming plays a rising tone chirp; disarming plays a falling tone chirp.
  - **Timer Integration**: Disarming pauses and unlatches the flight timer so the pilot can inspect total flight duration. Re-arming for the next flight automatically resets the timer back to its configured duration!
- **Field 3: Timer Duration (`Timer:`)**:
  - Configure the countdown timer in 30-second increments up to 60:00 (e.g. `05:00`), or set to `OFF` (0:00) to operate as a count-up stopwatch.
- **Field 4: Timer Trigger (`T-Trig:`)**:
  - Select the event that starts and runs the timer:
    - **`OFF`**: Timer disabled.
    - **`THs (RUN)`**: Throttle Switch mode—runs only while throttle stick $> 5\%$ ($> -900$), pauses when brought back to idle.
    - **`THt (LTCH)`**: Throttle Trigger / Latched mode—starts counting when throttle stick first exceeds 5%, and continues running non-stop throughout zero-throttle glides or multirotor descents.
    - **`ALWAYS ON`**: Continuous timer whenever the radio is powered on.
    - **Switch Conditions (`SA^` through `SDv`)**: Runs while the specified switch condition is active.
  - **Switch Auto-Detection**: Flipping any physical switch while editing `T-Trig` automatically sets that switch as the trigger.
- **Field 5: Model Copy / Duplicate (`Copy -> Mxx`)**:
  - Select a target model slot (`M01`..`M20`) and press **`[OK]`** to clone the current model's name, trims, curves, mixer lines, and protocol settings for safe experimentation.
- **Field 6: Bind RX (`[OK Bind]`)**:
  - Displays currently bound receiver ID (`Rx: XXXXXXXX`). Press **`[OK]`** to initiate receiver binding directly from Model Setup.
- **Field 7: Reset Defaults (`[OK Defaults]`)**:
  - Press **`[OK]`** to restore default trims, standard channel directions, linear curves, and mixer parameters for this model slot.

### Submenu 3: Dual Rate & Expo (`DUAL RATE/EXPO`)
Configures stick throw authority and center sensitivity for primary controls:
- **Switch**: Select physical hardware switch (`None`, `SA`, `SB`, `SC`, `SD`) to toggle between High Rates (UP) and Low Rates (MID/DOWN).
- **Channel**: Select axis to adjust (`Roll`, `Pitch`, `Yaw`).
- **Hi Rate / Lo Rate**: Adjust throw authority (30%..100% in 5% steps).
- **Hi Expo / Lo Expo**: Adjust center sensitivity (-100%..+100% in 5% steps). Positive expo softens stick sensitivity around center for smooth flight.

### Submenu 4: Throttle Curve Editor (`THR CURVE`)
Interactive curve engine with real-time on-screen curve visualization (49 x 37 pixel plot) and selected-point indicator dot:

```
+------------------------------------+
| THROTTLE CURVE                     |
|------------------------------------+
|   Pts: 9-PT        +--------------+|
|   Crv: SMOOTH      |     .---*    ||
| > P3: 50% <        |    /         ||
|                    |   /  *       ||
|                    |  /           ||
|                    +--------------+|
|------------------------------------+
| [OK] Next Pt   [ESC] Done          |
+------------------------------------+
```

- **Field 0 (`Pts:`)**: Toggle between **`5-PT`** and **`9-PT`**. The UI dynamically updates the point range indicator (`Pts: 1..5` in 5-point mode, `Pts: 1..9` in 9-point mode). Switching from 5-point to 9-point mode automatically **resamples** midpoint values between existing points (e.g. `[0, 25, 50, 75, 100]` -> `[0, 12, 25, 37, 50, 62, 75, 87, 100]`), preventing flat-zero dropoffs.
- **Field 1 (`Crv:`)**: Toggle between **`LINEAR`** (piecewise linear interpolation) and **`SMOOTH`** (**Catmull-Rom cubic Hermite spline** smoothing).
- **Field 2.. (`P1` .. `Pn`)**:
  - While navigating (`!editing`), scroll with **`[UP]`** / **`[DOWN]`** and press **`[OK]`** to enter point-editing mode (`> Pn: XX% <`).
  - While editing:
    - **`[UP]`** / **`[DOWN]`**: Adjust point value between 0% and 100% (with auto-repeat when held).
    - **`[OK]`**: Confirm current point and advance to next point (`P1` -> `P2` -> `...`). On the last point, exits edit mode.
    - **`[BIND]`**: Tabs to the next point (wraps to `P1`).
    - **`[CANCEL]` (`[ESC]`)**: Exits point-editing mode.
  - A real-time 3 x 3 pixel dot indicator is plotted directly on the curve graph at the coordinates of the actively selected point.

### Submenu 5: Wing & Tail Mixer (`WING/MIXER`)
- **Wing Template**: Cycle between `NORMAL`, `ELEVON/DELTA` (flying wings/jets: mixes Pitch & Roll on CH1/CH2), `V-TAIL` (gliders: mixes Pitch & Yaw on CH2/CH4), and `FLAPERON` (dual ailerons on CH1 & CH6 with flap input).
- **Freeform Mix Lines (`M1` .. `M8`)**: Press **`[OK]`** to edit any mix line:
  - **Target**: Output channel (`CH1`..`CH14` or `Disabled`).
  - **Source**: Control source (`Roll`, `Pitch`, `Thr`, `Yaw`, `VRA`, `VRB`, `SA..SD`, `MAX`, `CH1..CH14`).
  - **Weight / Offset**: Percentage scaling (-100%..+100%).
  - **Switch**: Activation condition (`ON`, `SA^`, `SAv`, `SB^`, `SB-`, `SBv`, `SC^`, `SC-`, `SCv`, `SD^`, `SDv`).
  - **Mode**: Multiplex method (`ADD (+)`, `MULT (*)`, `REPL (:=)`).

### Submenu 6: Auxiliary Channels (`AUX CHANNELS`)
Assigns physical controls (switches `SA..SD`, pots `VRA/VRB`, sticks, or `None`) to channels `CH5` through `CH14`.

### Submenu 7: Channel Reverse (`CH REVERSE`)
- Lists all 14 channels (CH1:ROL, CH2:PIT, CH3:THR, CH4:YAW, SwA..SwD, VR1, VR2).
- Press **`[OK]`** to toggle between **`NOR`** (Normal) and **`REV`** (Reversed).
- Calculations use hardware-standard inversion: `pulse = 3000 - pulse`.
- Automatically saved to non-volatile Flash upon exit.

### Submenu 8: Radio Setup (`RADIO SETUP`)
The Radio Setup menu features a scrollable 4-item viewport with 9px row heights and automatic vertical scrolling across 12 configuration parameters:
- **`Thr Trim:`**: Toggle between `OFF (Lock)`, `IDLE`, and `LINEAR`.
- **`Beeper:`**: Toggle audio sound between `ENABLED` and `MUTED`.
- **`Tones:`**: Select audio notification style between **`RICH`** (melodic multi-tone chime sequences) and **`SIMPLE`** (classic single-tone buzzer beeps). Toggling gives an immediate live audio preview!
- **`BL Timer:`**: LCD backlight auto-shutoff timeout: `ALWAYS ON`, `15 SEC`, `30 SEC`, or `60 SEC`. Touching any key or moving any stick wakes the backlight instantly.
- **`BL Level:`**: Backlight brightness level from `10%` to `100%` in 10% steps (supports both stock transistors and the `PC9` hardware PWM dimming mod).
- **`Contrast:`**: LCD Electronic Volume (EV) contrast adjustment from `20` to `50` in steps of 3 (default: **`37`** / `0x25`). Adjusting this value provides instant live visual preview on the ST7567 display and persists across reboots.
- **`Bat Warn:`**: Low battery alarm threshold from `4.0V` to `5.0V` in 0.1V steps (default: **`4.4V`**, or 1.10V/cell for 4xAA). When battery drops below this voltage, the status bar badge flashes inverted and an audible warning chirp sounds.
- **`USB Mode:`**: Selects active USB peripheral personality (switches on-the-fly without rebooting):
  - **`OFF`** (Default): Disables USB peripheral and D+ pullup to prevent unwanted PC inputs and minimize power draw.
  - **`JOYSTICK`**: 100 Hz native USB Gamepad for flight simulators with silent RF standby (zero RF radiation, cool running).
  - **`SERIAL`**: Virtual COM Port (CDC-ACM) at 115200 baud streaming live JSON telemetry while maintaining normal RF transmission.
  - **`COMPOSITE`**: Simultaneous HID Gamepad + CDC-ACM Virtual COM Port.
- **`PC13 Pwr:`**: Configures external module power polarity on `PC13`: `HIGH (N)` (default active-HIGH for N-channel MOSFET switches) or `LOW (P)` (active-LOW for P-channel MOSFET switches).
- **`Servo Hz:`**: AFHDS 2A receiver PWM servo refresh rate: cycle through `50 Hz`, `60 Hz`, `100 Hz`, `150 Hz`, `200 Hz`, `250 Hz`, `300 Hz`, `350 Hz`, `400 Hz` (default: **`50 Hz`**).
  > **Servo Safety Warning**: Standard analog servos (e.g. SG90, MG90S) **MUST** be driven at 50 Hz. Feeding higher frequencies to analog servos can cause severe jitter, excessive heating, and motor burnout. Only select rates above 50 Hz when all connected servos are high-speed digital servos rated for higher frame rates.
- **`RX Out:`**: AFHDS 2A receiver output signal format: **`PWM`** (individual servo pin outputs) or **`PPM`** (composite Pulse Position Modulation stream on CH1).
- **`Serial:`**: AFHDS 2A receiver serial bus protocol: **`i-BUS`** (`0xDE`, FlySky proprietary bidirectional serial protocol) or **`S.BUS`** (`0xDD`, Futaba/FrSky inverted serial stream for flight controllers). Transmitted immediately over the air to reconfigure the receiver hardware dynamically.

### Submenu 9: Protocol Setup (`PROTOCOL SETUP`)
Replaces the redundant bind menu with universal RF protocol management:
- **`Proto: AFHDS 2A`**: Uses the built-in A7105 transceiver. Displays active model name and bound receiver ID (e.g. `Rx ID: 1A2B3C4D`). Pressing **`[OK]`** triggers receiver binding. Pressing **`[UP]`** or **`[DOWN]`** cycles protocol.
- **`Proto: CRSF / ELRS`**: Drives external Crossfire or ExpressLRS transmitter modules connected to the rear expansion bay (`PD5` TX, `PA15` RX) with hardware power control on `PC13`. Pressing **`[OK]`** cycles selection through Protocol, Baud Rate, and Configure Module:
  - `Baud: 420k (ELRS)`: Default recommended speed for ExpressLRS.
  - `Baud: 416.6k (TBS)`: Standard TBS Crossfire module rate.
  - `Baud: 115.2k (Low)`: Low-speed compatibility / diagnostic rate.
  - `Baud: 921.6k (Fast)`: High-throughput ExpressLRS rate.
  - `[Configure Module]`: Opens the native TBS-Agent style configurator screen to discover devices, navigate subfolders, and configure settings.
  - *For complete wiring pinouts, power polarity settings, live telemetry dashboard metrics, and the configurator guide, see the dedicated [CRSF & ExpressLRS Subsystem Guide](CRSF_ELRS_GUIDE.md).*

### Submenu 10: Channel Monitor (`CHANNEL MONITOR`)
- Displays live pulse widths (1000..2000 µs) across all 14 channels with 40-pixel horizontal graphic bar indicators and exact microsecond numbers.
- Press **`[UP]`** / **`[DOWN]`** to toggle between Page 1 (CH1..CH7) and Page 2 (CH8..CH14).

### Submenu 11: Stick Calibration (`STICK CALIB`)
Launches the interactive 2-step calibration wizard (see Section 5 below).

### Submenu 12: Analog Diagnostics (`DIAG ANAS`)
- Multi-page graphic diagnostics screen matching the `CHANNEL MONITOR` layout with 40-pixel graphic fill bars and exact 4-digit raw decimal ADC counts (0..4095):
  - **Page 1 (`ANALOG (1-6)`)**: Stick gimbals & switches: `RH:AIL`, `RV:ELE`, `LV:THR`, `LH:RUD`, `SW:SA`, `SW:SB`.
  - **Page 2 (`ANALOG (7-11)`)**: Rotary pots, switches, & battery: `POT:V1`, `POT:V2`, `SW:SC`, `SW:SD`, `VBAT`.
- Press **`[UP]`** / **`[DOWN]`** to switch between Page 1 and Page 2.
- Press **`[CANCEL]` (`[ESC]`)** to return to the Main Menu.

### Submenu 13: System Information (`SYSTEM INFO`)
- Displays MCU type (`STM32F072VB` or `APM32F072VB`), 96-bit silicon UID, firmware version, Flash memory map, and storage statistics.

---

## 5. Flight Timer Subsystem & Pilot Audio Ergonomics

The `flysky-i6x-rs` firmware features an EdgeTX/OpenTX-parity flight timer engine designed to prevent unexpected battery depletion, track flight pack duration, and support diverse aircraft categories (fixed-wing, sailplane, and multirotor).

### 1. Timer Modes & Triggers (`T-Trig:`)
Configurable per-model in **`MODEL SETUP`** -> **`T-Trig:`**:

| Mode | LCD Display | Trigger Behavior | Ideal Application |
| :--- | :--- | :--- | :--- |
| **Disabled** | `OFF` | Timer is completely disabled. | Bench testing / non-timed setups |
| **Throttle Run** | `THs (RUN)` | Timer runs while throttle stick is $> 5\%$ ($> -900$), and **pauses** whenever throttle drops back to idle. | Electric airplanes & gliders where battery only discharges under motor power |
| **Throttle Latched** | `THt (LTCH)` | Starts counting when throttle stick first exceeds 5%, and **latches ON**, continuously running throughout the flight even during low-throttle descents or inverted glides. | FPV quadcopters & aerobatic aircraft |
| **Always On** | `ALWAYS ON` | Runs continuously whenever the transmitter is turned on. | Session duration / general stopwatch |
| **Switch Condition** | `SA^` .. `SDv` | Runs while the assigned switch position is active; pauses when inactive. | Manual switch timing / glider thermaling |

*Tip:* When editing `T-Trig:`, flipping any physical switch auto-detects and selects that switch position immediately.

### 2. Arm Switch Integration (Auto-Reset on Arm / Pause on Disarm)
When an **Arm Switch** (`Arm Sw:`) is configured in `MODEL SETUP`:
- **Disarm (Landing)**: Unlatches `THt` and pauses the timer immediately. The timer display freezes on screen so the pilot can inspect total airtime and remaining pack duration post-landing.
- **Re-Arm (Next Battery)**: Automatically resets the timer back to its initial countdown value (or 00:00 for count-up) and clears all elapsed accumulator seconds—ready for the next flight pack without touching any buttons.

### 3. Visual Hold-to-Reset HUD Progress Modal
To manually reset the timer on the flight dashboard at any time:
1. Press and hold **`[CANCEL]` (`[ESC]`)** on any dashboard page.
2. A high-contrast centered HUD modal appears with a live progress bar:
   ```text
   +------------------------+
   |      RESET TIMER       |
   |   [==============    ] |
   +------------------------+
   ```
3. Hold for **1.0 second**: The progress bar reaches 100%, an acoustic confirmation chime (`2400 Hz`) sounds, the timer resets to full duration, and a `"TIMER RESET!"` toast flashes for 600 ms.
4. *Safety Guard*: Releasing `[CANCEL]` before 1.0s aborts immediately without altering the timer.

### 4. Acoustic Countdown & Alarm Sequence
The audio engine provides distinctive non-blocking tone patterns during flight:
- **1-Minute Beep**: A short high chime (`2400 Hz`, 50 ms) sounds on every whole minute mark (`04:00`, `03:00`, `02:00`, `01:00`).
- **10-Second Countdown**: Single ticks (`2200 Hz`, 40 ms) sound every second from `00:10` down to `00:01`.
- **Elapsed Alarm (`00:00`)**: A 3-tone urgent alert (`2600 Hz`) sounds when the timer reaches zero.
- **Overdue Count-Up**: After reaching zero, the timer continues counting upward into negative time (`-00:01`, `-00:02`...) and blinks/inverts on the LCD footer, informing the pilot exactly how far overdue the flight is.

### 5. Non-Visual Potentiometer Center Detent Haptics
Because the FS-i6X hardware potentiometers (`VRA` and `VRB`) lack physical center detents, the firmware monitors knob rotation in real time:
- Whenever `VRA` or `VRB` crosses through mechanical center ($0$), a subtle audio click (`2200 Hz`, 10 ms) sounds.
- Enables pilots wearing FPV goggles or maintaining visual line-of-sight on their model to center flaps, gimbal pitch, or gain dials without glancing at the screen.

---

## 6. Gimbal & Potentiometer Calibration Procedure

Calibration ensures gimbals reach full travel without clipping or deadzones:

1. **Enter Calibration**:
   - Hold **`[OK]` for 1.2s** on the flight screen -> select `STICK CALIB`.
   - Alternatively, **hold `[OK]` while switching on the radio**.
2. **Step 1: Center Position**:
   - Let Roll, Pitch, and Yaw return to center springs.
   - Move Throttle (friction stick) to the physical middle (50%).
   - Center rotary dials VRA and VRB.
   - Press **`[OK]`** to capture neutral centers.
3. **Step 2: Limit Travel**:
   - Move both gimbals in wide circular motions touching all four corners.
   - Rotate VRA and VRB back and forth across their full rotation.
   - Watch the on-screen gauges fill out. Once an axis has measured sufficient travel, its indicator changes from `--` to `OK`.
   - When all axes display `OK`, press **`[OK]`** to save.
4. **Completion**:
   - The radio applies OpenTX-standard ~1.6% margin tolerances, plays a 2-tone success chime, and saves calibrations to Flash.

---

## 7. Receiver Binding Instructions

The firmware provides 4 convenient ways to initiate AFHDS 2A binding with clean separation logic:
1. **Hold `[BIND]` during Power-On**: Boots directly into RF bind mode.
2. **Hold `[BIND]` (>= 1.0s) on Flight Screen**: Initiates binding from any flight dashboard page.
3. **`MODEL SETUP` Menu**: Select Field 6 (`Bind RX`) and press **`[OK]`**.
4. **`RX SETUP & BIND` Menu**: Press **`[OK]`**.

### Method A: Two-Way Telemetry Receivers (FS-iA6B, FS-iA10B)
1. Power on the receiver with a bind plug inserted into the `B/VCC` port (LED flashes rapidly).
2. Initiate binding on the transmitter using any of the 4 methods above.
3. The transmitter broadcasts bind packets and prompts `BINDING`.
4. Once the receiver receives the hopping table, it sends its unique ID back.
5. The transmitter automatically captures the ID, plays a rising 2-tone chime, saves to the active model profile in Flash, and switches to normal transmission. The top status bar shows `RF:OK` and live RSSI.
6. Remove the bind plug and power cycle the receiver.

### Method B: One-Way Receivers (FS-A8S, Fli14, FS-iA6)
1. Hold the receiver's bind button while powering on (LED flashes rapidly).
2. Initiate binding on the transmitter.
3. Once the receiver's LED turns solid (indicating it has locked onto the transmitter's frequency hopping table), press **`[CANCEL]` (`[ESC]`)**.
4. The transmitter saves the bind state to Flash, double-beeps, and begins transmitting regular channel data.

---

## 8. Firmware Flashing, Full Flash Backup, & DFU Recovery

The FlySky FS-i6X can be backed up and flashed directly over USB without specialized hardware programmer probes:

### 1. Enter Factory ROM DFU Bootloader

#### A. Initial Flash from Stock FlySky Factory Firmware (R53 Bootloader Access)
The stock FlySky factory firmware does not include the software key-check logic to jump into the DFU bootloader. Therefore, entering DFU mode for the very first time requires access to the hardware `BOOT0` line:
1. Ensure the transmitter is switched **OFF** and remove the rear case screws.
2. Carefully separate the rear case. Be aware of the battery wires connected between the two halves. Once separated, locate the two unpopulated solder pads labeled **`R53`** on the back of the motherboard (near the microcontroller). It is best to connect the USB cable to the rear case now.
3. Momentarily bridge/short the two `R53` pads using tweezers, a jumper wire, or a screwdriver tip.
4. While holding the bridge across `R53`, have the USB cable connected to your PC and switch the transmitter power switch **ON**.
5. Bridging `R53` pulls the MCU's `BOOT0` pin to 3.3V, causing the chip to boot directly into its factory ROM DFU bootloader (`0483:df11` for STM32, `314b:0106` for APM32). The transmitter screen remains blank, and the PC detects the device as `STM32 BOOTLOADER`.
6. Once powered on, you can remove the bridge across `R53`. You do not need to keep it bridged while flashing.

> [!TIP]
> For board photos and detailed platform walk-throughs, refer to the [OpenI6X Flashing & Upgrading Documentation](https://github.com/OpenI6X/opentx/wiki/Flashing-&-Upgrading).

#### B. Upgrading from OpenI6X or flysky-i6x-rs (No Disassembly Needed)
Once custom firmware has been flashed to the radio, hardware pad bridging is never needed again:
- With the transmitter powered off, hold **Roll Left + Yaw Right** inward towards the power switch while switching on the radio.
- The LCD screen remains blank, and the transmitter enumerates over USB as `0483:df11` (STM32 BOOTLOADER).

### 2. Backup Entire Flash (CRITICAL BEFORE FIRST FLASH)
Before flashing any custom firmware, pull your entire 128 KB on-chip Flash memory to a local file for 100% safe, instant reversion:
```bash
# Backup complete 128 KB on-chip Flash (firmware + calibration + models)
dfu-util -a 0 -s 0x08000000:131072 -U stock_backup.bin
```

### 3. Flash flysky-i6x-rs Firmware
Flash the compiled binary via `dfu-util`:
```bash
# For STM32F072:
dfu-util -a 0 -s 0x08000000:leave -D flysky-i6x.bin

# For APM32F072:
dfu-util -a 0 -d 314b:0106 -s 0x08000000:leave -D flysky-i6x.bin
```
The radio will immediately reboot into the new firmware upon completion.

### 4. Restore / Revert Anytime
You can restore your original stock or OpenI6X backup file at any time:
```bash
dfu-util -a 0 -s 0x08000000:leave -D stock_backup.bin
```

---

## 9. Safety Systems & Audio Alarms

The firmware includes five levels of proactive safety protection inspired by OpenTX/EdgeTX and aerospace fail-safe design:

### 1. Pre-Flight Startup Checks (Throttle & Switch Safety Interlock)
- **Detection**: At power-on, the radio inspects the physical throttle position and all 4 toggle switches (`SA`, `SB`, `SC`, `SD`).
- **Safety Trigger**: If the throttle stick is > 5% above zero, or any switch is not in the safe **UP** position:
  - The transmitter intercepts normal boot and presents a dedicated **`SAFETY WARNING!`** screen.
  - RF transmission is locked into zero-throttle failsafe pulses (1000 µs) so motors cannot spin up.
  - An urgent alternating alarm tone (`warn_preflight`) sounds every 800 ms.
- **Clearing**: Moving the throttle stick to minimum and returning all switches to UP automatically clears the warning with a confirmation chirp and opens the flight screen. Alternatively, pressing **`[CANCEL]` (`[ESC]`)** bypasses the check.

### 2. Transmitter Low Battery Alarm
- **Threshold**: Configurable in `RADIO SETUP` -> `Bat Warn` (`4.0V` .. `5.0V`, default **`4.4V`**).
- **Visual Alert**: The battery voltage badge on the top right status bar blinks in inverted video (`[ 4.38V ]`).
- **Audio Alert**: The piezo buzzer sounds a double-chirp warning (`2400 Hz`) every 8 seconds.

### 3. Radio Inactivity Idle Alarm
- **Timeout**: 10 minutes (600 seconds).
- **Behavior**: If no physical sticks, switches, trims, or keys are moved for 10 minutes, the radio emits a reminder chime every 30 seconds to alert the pilot and prevent battery drain.

### 4. Telemetry RSSI Range Alarms
- **Low Signal Warning (RSSI < 40%)**: Sounds a caution beep (`2000 Hz`) every 6 seconds.
- **Critical Signal Alarm (RSSI < 20%)**: Sounds an urgent double-beep (`2800 Hz`) every 3 seconds to warn the pilot of imminent radio failsafe.

### 5. In-Flight Watchdog Recovery & Fast Return-to-Control
- **Hardware Protection**: An Independent Hardware Watchdog (`pac::IWDG`) runs autonomously off an isolated 40 kHz LSI oscillator with a 2.0-second timeout.
- **Warm Reboot Detection**: If an unexpected MCU reset occurs (e.g. from ESD, power rail fluctuation, or timing transient), the radio detects the watchdog reset signature in `RCC_CSR`.
- **Interlock Bypass**: Unlike traditional firmware that traps the pilot on a startup warning screen because the throttle is up mid-flight, `flysky-i6x-rs` automatically bypasses power-on throttle/switch interlocks and the DFU delay during watchdog recovery.
- **Instant Flight Recovery (< 2 ms)**: Active over-the-air channel transmission resumes in under **2 milliseconds**, well before receiver failsafe triggers.
- **Pilot Acoustic Warning**: An urgent 3-beep alarm pattern (`2600 Hz`) sounds immediately on reboot to notify the pilot of the event while maintaining uninterrupted flight control.

---

## 10. USB Subsystem & Flight Simulator Operations

The FlySky FS-i6X features a hardware USB Full-Speed port wired directly to the microcontroller (`PA11` / `PA12`). The `flysky-i6x-rs` firmware supports native plug-and-play USB Joystick control, Virtual COM Port telemetry, and silent RF running.

### 1. Flight Simulator Setup (Liftoff, Velocidrone, RealFlight)
1. In **`RADIO SETUP`**, ensure **`USB Mode`** is set to **`JOYSTICK`** (or `COMPOSITE`).
2. Connect a standard Micro-USB cable between the FS-i6X and your computer.
3. The radio automatically enumerates as **`FS-i6X Joystick`** on Windows, Linux, and macOS without requiring any drivers.
4. Open your flight simulator (e.g. Liftoff, Velocidrone, RealFlight, FPV Freerider):
   - Navigate to the simulator's Controller Settings.
   - Select `FS-i6X Joystick`.
   - Calibrate the 4 main axes: Throttle, Roll, Pitch, and Yaw.
   - Assign switches (SwA..SwD) to simulator functions like Arm, Flight Mode (Acro/Angle), or Turtle Mode.

### 2. Silent RF Standby (Zero RF Emission)
When connected via USB in `JOYSTICK` mode:
- The 2.4 GHz RF power amplifier and A7105 transceiver are placed in **Standby** mode.
- The flight screen status badge displays **`U:SIM`**.
- Benefits:
  - **Zero RF radiation**: Safe for close-up desktop simulator sessions.
  - **Cool running**: Prevents heat buildup from the RF amplifier.
  - **Battery savings**: Extends AA battery life significantly.
- Unplugging the USB cable or switching to `SERIAL` mode immediately restores normal RF transmission to your aircraft.

### 3. Virtual COM Port & Telemetry Streaming
In **`SERIAL`** or **`COMPOSITE`** mode, the radio exposes a standard virtual serial port (`/dev/ttyACM0` on Linux, `COMx` on Windows):
- **Clean Interactive CLI**: Connecting with a serial terminal (`sudo picocom /dev/ttyACM0 --imap lfcrlf`) connects silently without buffer spew. Pressing `[Enter]` displays the `i6x> ` prompt:
  - `help`: List available commands.
  - `status`: Show firmware version, RF protocol, and battery/telemetry overview.
  - `channels`: Print real-time pulse widths in JSON format `{"ch":[...]}`.
  - `telem`: Print a single JSON telemetry line on demand.
  - `stream`: Start continuous 10 Hz JSON telemetry streaming (press any key to stop).
  - `reboot`: Trigger a software system reset.
- **Universal Telemetry Streaming (10 Hz)**: Streams structured JSON Lines (`ndjson`) universally parseable by Python, Node.js, or WebSerial:
  ```json
  {"vbat":5.18,"rssi":98,"rx_v":5.02,"tx":15820,"rx":15798,"err":22,"ch":[1500,1500,1150,1500,1000,1000,1500,1500,1000,1000,1500,1500,1500,1500]}
  ```
