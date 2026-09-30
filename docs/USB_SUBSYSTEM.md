# USB Subsystem Architecture & Operations Manual

A comprehensive technical reference and operations guide for the USB subsystem of `flysky-i6x-rs` on the FlySky FS-i6X transmitter.

---

## 1. Overview & System Philosophy

The standard FlySky FS-i6X transmitter PCB is equipped with a native USB port wired directly to the microcontroller's hardware USB Full-Speed (12 Mbps) peripheral. Building on the USB joystick emulation established by the OpenI6X project, `flysky-i6x-rs` introduces a pure `no_std` Rust USB subsystem featuring:
- **Zero Heap Allocations**: Powered by `usb-device`, `stm32-usbd`, `usbd-hid`, and `usbd-serial` using static memory structures and direct hardware packet buffers.
- **Dual-Personality Operation**:
  - **Flight Simulator Joystick (HID)**: High-rate (100 Hz), jitter-free, native 8-axis 16-button gamepad for drone and RC flight simulators.
  - **Telemetry Virtual COM Port (CDC-ACM)**: High-speed serial diagnostics streaming real-time receiver telemetry, channel pulses, and accepting interactive CLI commands.
  - **Composite Mode**: Simultaneous HID Joystick + CDC-ACM Virtual COM Port on a single USB cable.
  - **Off / Charge-Only Mode**: Completely turns off the USB transceiver and disconnects pull-up resistors to eliminate power draw while charging.
- **Safety First & Silent RF Standby**: When operating in Simulator mode, the 2.4 GHz RF power amplifier and A7105 transceiver are completely shut down (zero RF emission, extended battery life, and cool operation).
- **Hard Real-Time RF Inviolability**: The USB interrupt priority (`0xC0`) is strictly lower than the RF timing interrupts (`TIM16` at `0x80` and `EXTI2` at `0x80`), mathematically guaranteeing that USB communication never causes jitter or dropped packets on the radio link.

---

## 2. Hardware Architecture & Clocking

### Pin Assignments & Interface
The STM32F072VB microcontroller features a built-in USB 2.0 Full-Speed device controller:

| Pin | Function | Alternate Function | Notes |
| :--- | :--- | :--- | :--- |
| **`PA11`** | `USB_DM` (Data -) | `AF0` | Connected directly to Micro-USB receptacle |
| **`PA12`** | `USB_DP` (Data +) | `AF0` | Connected directly to Micro-USB receptacle |
| **Internal** | 1.5 kΩ Pull-up | Software-Controlled | Enabled via `DPPU` (bit 15) in `USB_BCDR` (`0x4000_5C58`) |

> [!NOTE]
> Unlike early STM32F103 designs which required an external 1.5 kΩ pull-up resistor or a transistor circuit on `PA12`, the STM32F072 contains an integrated 1.5 kΩ pull-up resistor directly on silicon. It is automatically engaged by firmware when USB is enabled.

### 48 MHz Clock Tree Integration
The USB Full-Speed physical layer requires a strictly regulated 48.000 MHz clock source with < 0.25% frequency tolerance:
1. **Clock Source**: External 8.000 MHz crystal (HSE) on the FS-i6X mainboard.
2. **PLL Multiplier**: `8 MHz * 6 = 48.000 MHz`.
3. **Clock Routing**: The PLL output is routed directly to the USB peripheral by asserting the `USBSW` bit (bit 7) in the `RCC_CFGR3` register (`0x4002_1030`) during `chip::init_system_clock()`:
   ```rust
   // Route PLL (48 MHz) to USB clock source (USBSW = 1 in RCC_CFGR3 bit 7)
   const RCC_CFGR3: *mut u32 = 0x4002_1030 as *mut u32;
   let cfgr3 = ptr::read_volatile(RCC_CFGR3);
   ptr::write_volatile(RCC_CFGR3, cfgr3 | (1 << 7));
   ```
4. **Packet Memory Area (PMA)**: 1024 bytes of dedicated SRAM located at `0x4000_6000`, accessed via 16-bit word accesses (`MemoryAccess::Word16x2`).

---

## 3. USB Operating Modes

The active USB mode is configured in **`Radio Setup -> USB Mode`** and saved persistently in Flash:

```
[ RADIO SETUP ]
-----------------------------
Thr Trim:        OFF (Lock)
Beeper:          ENABLED
BL Timer:        ALWAYS ON
BL Level:        100%
Contrast:        37
Bat Warn:        4.4V
USB Mode:        JOYSTICK  <-- Cycle with [OK]
-----------------------------
[OK] Toggle/Cycle   [ESC] Back
```

### Mode Comparison Matrix

| Mode | Value | HID Gamepad | Virtual COM Port | RF Transceiver State | Primary Use Case |
| :--- | :---: | :---: | :---: | :---: | :--- |
| **`OFF`** | **`0`** | Inactive | Inactive | **Active (100 mW Transmitting)** | Default: Charge-only, no USB enumeration, zero PC connection |
| **`JOYSTICK`** | **`1`** | **Active (100 Hz)** | Inactive | **Standby / Muted (0 mW)** | Flight simulators (Liftoff, Velocidrone, RealFlight) |
| **`SERIAL`** | **`2`** | Inactive | **Active (115200)** | **Active (100 mW Transmitting)** | Telemetry logging, blackbox monitoring, interactive CLI |
| **`COMPOSITE`**| **`3`** | **Active (100 Hz)** | **Active (115200)** | **Active (100 mW Transmitting)** | Dual-purpose simulator & ground control station |

> [!NOTE]
> **On-The-Fly Mode Switching**: When switching modes in `Radio Setup`, the firmware asserts a Single-Ended Zero (SE0) physical disconnect by driving both `PA11` (`USB_DM`) and `PA12` (`USB_DP`) LOW for ~150 ms and issuing an APB1 peripheral reset. The host PC detects a clean physical cable unplug and re-enumerates the newly selected personality instantly without requiring a power cycle.

---

## 4. USB Gamepad / Joystick (HID)

When `JOYSTICK` or `COMPOSITE` mode is enabled, the transmitter enumerates on Windows, macOS, and Linux as a standard plug-and-play **DirectInput / SDL2 Human Interface Device (HID)** without requiring any external drivers or software.

### USB Identity
- **Joystick Mode**:
  - **Vendor ID (VID)**: `0x1209` (pid.codes open hardware)
  - **Product ID (PID)**: `0x4F54` (Official OpenTX / EdgeTX Radio Joystick)
  - **Product Name**: `FS-i6X Joystick`
  - **Manufacturer**: `FlySky`
  - **Serial Number**: `FS-I6X-SIM`
- **Composite Mode**:
  - **Vendor ID (VID)**: `0x1209`
  - **Product ID (PID)**: `0x4968` (Official EdgeTX Radio Composite)
  - **Product Name**: `FS-i6X Radio`
- **Serial Mode**:
  - **Vendor ID (VID)**: `0x0483` (STMicroelectronics)
  - **Product ID (PID)**: `0x5740` (Standard Virtual COM Port)
  - **Product Name**: `FS-i6X Serial`

### Report Descriptor (8 Axes, 16 Buttons)
The HID descriptor adheres strictly to OpenI6X and EdgeTX mapping conventions to ensure immediate, zero-configuration compatibility across all major RC simulators:

```
                +---------------------------------------+
                |    18-Byte USB HID Report Format      |
                +---------------------------------------+
                | Byte 0..1   | 16 Digital Buttons      |
                | Byte 2..3   | Axis X: Roll (AIL - CH1)|
                | Byte 4..5   | Axis Y: Pitch (ELE - CH2)|
                | Byte 6..7   | Axis Z: Throttle (THR - CH3)|
                | Byte 8..9   | Axis Rz: Yaw (RUD - CH4)|
                | Byte 10..11 | Axis Rx: VRA Pot (CH7)  |
                | Byte 12..13 | Axis Ry: VRB Pot (CH8)  |
                | Byte 14..15 | Slider: CH5 Aux 1       |
                | Byte 16..17 | Dial: CH6 Aux 2         |
                +---------------------------------------+
```

#### Axis Normalization
All stick and potentiometer channels are calculated through the flight mixer pipeline (incorporating calibration, dual rates, expos, and trims), clamped to 1000..2000 µs, and linearly scaled to 11-bit USB axis units (0..2047, centered at 1024):
$$ \text{USB Axis} = \left( \frac{\text{Pulse}_{\mu s} - 1000}{1000} \right) \times 2047 $$

#### Button Mapping (Switches SA..SD & Aux Channels)
Physical switches and auxiliary channels are mapped cleanly to digital buttons. In standard neutral position (all switches UP), **all buttons report 0 (released)**, eliminating phantom keystrokes or desktop focus lock on Linux systems:

| Button | Source | Active Condition |
| :---: | :--- | :--- |
| **Button 1** | Switch SA (2-pos) | Down position |
| **Button 2** | Switch SB (3-pos) | Mid position |
| **Button 3** | Switch SB (3-pos) | Down position |
| **Button 4** | Switch SC (3-pos) | Mid position |
| **Button 5** | Switch SC (3-pos) | Down position |
| **Button 6** | Switch SD (2-pos) | Down position |
| **Buttons 7..12** | Channels 9..14 | High pulse (`pulse > 1500 µs`) |
| **Buttons 13..16** | Spare | 0 (Released) |

### Compatible Flight Simulators
The native joystick mode has been verified with:
- **SeligSim** (Linux)

It is expected to work with any flight simulator:
- **Liftoff: FPV Drone Racing** (Steam / PC / Mac)
- **VelociDrone FPV Racing Simulator**
- **RealFlight Evolution / RF9**
- **FPV Freerider / FPV Freerider Recharged**
- **Uncrashed: FPV Drone Simulator**
- **DRL Simulator (Drone Racing League)**
- **AccuRC 2 Precision Flight Simulator**

---

## 5. Virtual COM Port (CDC-ACM) & Interactive CLI

When `SERIAL` or `COMPOSITE` mode is selected, the transmitter exposes a standard virtual serial port (`/dev/ttyACM0` on Linux, `COMx` on Windows).

### Connection Parameters
- **Baud Rate**: 115,200 bps (Virtual CDC-ACM transfers at full 12 Mbps USB speed regardless of baud setting).
- **Data Bits**: 8
- **Parity**: None (N)
- **Stop Bits**: 1
- **Flow Control**: None

### Silent Connection & Interactive CLI
To avoid buffer fragmentation, stale bytes, or unsolicited text spew when connecting terminal emulators, the USB serial port initializes **silently**:
- When opening a terminal (e.g. `picocom /dev/ttyACM0 --imap lfcrlf`), no unsolicited banner is blasted over the wire.
- Pressing **`[Enter]`** displays the interactive prompt: `i6x> `.
- Local echo and backspace handling (`0x08`, `0x7F`) are supported.

```bash
$ sudo picocom /dev/ttyACM0 --imap lfcrlf
Terminal ready

i6x> help
Commands:
  help      - Show this help
  status    - Firmware & battery info
  channels  - Dump RF channel values (CH1..CH14)
  telem     - Dump single telemetry frame
  stream    - Start continuous telemetry streaming (any key to stop)
  reboot    - Reboot transmitter
i6x> 
```

### Supported CLI Commands

| Command | Description | Example Output |
| :--- | :--- | :--- |
| **`help`** | Displays available serial CLI commands | `Commands: help, status, channels, telem, stream, reboot` |
| **`status`** | System health, firmware version, RF protocol, and telemetry | `FlySky FS-i6X Rust Firmware v...` + telemetry line |
| **`channels`** | Real-time channel pulse widths in JSON format | `{"ch":[1500,1500,1150,1500,1000,...]}` |
| **`telem`** | Dumps a single telemetry frame with sensor metrics and odometer | `{"vbat":5.18,"rssi":98,"rx_v":5.02,"tx":15820,"rx":15798,"err":22,"ch":[...]}` |
| **`stream`** | Starts continuous 10 Hz JSON telemetry streaming (press any key to stop) | `[Streaming telemetry @ 10Hz. Press any key to stop.]` |
| **`reboot`** | Safely triggers a software system reset via NVIC | `Rebooting...` |

### Telemetry Stream Format (Universal JSON Lines)
When `stream` is active (or when queried with `telem`), the transmitter outputs structured JSON Lines (`ndjson`) suitable for real-time visualization or logging:
```json
{"vbat":5.18,"rssi":98,"rx_v":5.02,"tx":15820,"rx":15798,"err":22,"ch":[1500,1500,1150,1500,1000,1000,1500,1500,1000,1000,1500,1500,1500,1500]}
```

---

## 6. Safety & Silent RF Standby

### Zero-RF Simulator Mode
Operating an RF transmitter indoors in close proximity to a computer monitor, router, and user's body is undesirable:
1. It creates unnecessary 2.4 GHz spectrum pollution.
2. It draws significant battery current (~80–120 mA) through the RF power amplifier.
3. The internal RF module produces waste heat over long simulator sessions.

When **`JOYSTICK`** mode is active and the USB cable is connected:
- The A7105 transceiver is immediately strobed into **`STROBE_STANDBY`** (`0xA0`).
- The antenna front-end switches are set to **`RF_MODE_OFF`** (`0b0000_0011`), turning off both the power amplifier (PA) and low-noise amplifier (LNA).
- The `TIM16` packet transmission interrupt handler returns immediately without pulsing the radio or modifying the A7105 FIFO.
- The flight screen replaces the RF indicator with **`U:SIM`**.
- As soon as the USB cable is unplugged or the mode is switched to `SERIAL`, normal RF transmission automatically resumes.

### Deterministic Real-Time Concurrency
In embedded safety-critical avionics, flight-control tasks must never suffer priority inversion or starvation from peripheral I/O:
- **`TIM16` RF Interrupt (Priority 0x80)**: Fires every 3.850 ms (260 Hz) to transmit AFHDS 2A packets. Highest peripheral priority.
- **`EXTI2` RF Packet Ready (Priority 0x80)**: Handles A7105 TX/RX FIFO events.
- **`SysTick` Monotonic Timer (Priority 0x40)**: Drives system timekeeping.
- **`USB` Full-Speed Interrupt (Priority 0xC0)**: Lowest peripheral priority.

If a large USB transfer or host polling stall occurs, the hardware NVIC preempts the USB routine instantaneously to execute the RF transmission. **The radio link is 100% mathematically protected.**

---

## 7. Storage Compatibility & Configuration

To guarantee backwards compatibility with existing users' saved models, all additions are strictly size-invariant:

### `RadioConfig` Binary Layout (Strictly 128 Bytes)
```
Offset  0..4   : Magic (0x46534B59 "FSKY")
Offset  4..8   : Version (CONFIG_VERSION = 4)
Offset  8      : Active Model (0..19)
Offset  9      : Throttle Trim Method
Offset 10      : Audio Enabled
Offset 11      : Backlight Timeout
Offset 12      : Backlight Brightness
Offset 13      : Vbat Warn Threshold
Offset 14      : LCD Contrast
Offset 15      : usb_mode (0: Off, 1: Joystick, 2: Serial, 3: Composite) <-- Default: 0 (Off)
Offset 16..48  : Stick Calibrations (4 x 8B)
Offset 48..64  : Pot Calibrations (2 x 8B)
Offset 64..128 : Reserved (64B)
```

### Protocol Selection (`ModelConfig`)
In Menu Item 9 (**`Protocol Setup`**), the radio supports per-model RF output selection:
- **`AFHDS 2A`**: Uses built-in A7105 transceiver. Pressing `[OK]` triggers receiver binding.
- **`CRSF / ELRS`**: Drives external Crossfire / ExpressLRS transmitter modules connected to the rear expansion bay (`PD5` TX / `PA15` RX) with active power control on `PC13`. Pressing `[OK]` toggles between protocol and selectable baud rates (`420k (ELRS)`, `416.6k (TBS)`, `115.2k (Low)`, `921.6k (Fast)`). Field `crsf_baud` occupies byte 115 in `ModelConfig`.
