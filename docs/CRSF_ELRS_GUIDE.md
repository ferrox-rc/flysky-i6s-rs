# CRSF / ExpressLRS Subsystem & Native Module Configurator

Technical guide and user documentation for the native Crossfire (CRSF) and ExpressLRS (ELRS) subsystem in the FlySky FS-i6S Rust firmware.

> [!IMPORTANT]
> **Subsystem Status & Ground Testing Notice:**
> The CRSF/ELRS subsystem implements the serial CRSF protocol specification in bare-metal Rust as an open-source community contribution. While the serial drivers, frame parsing, CRC arithmetic, and baud timing have been implemented per specification, pilots using external transmitter modules must conduct thorough bench tests, fail-safe verification, and physical ground range checks before flying any model. Like all open-source RC software, this module is provided strictly "AS IS" without warranty of any kind.

---

## 1. Hardware Interface

The FlySky FS-i6S motherboard provides an internal rear module connector and back port routed to STM32F072VB peripherals:

| Pin | Function | Peripheral | Description |
| :--- | :--- | :--- | :--- |
| `PD5` | **USART2_TX** | AF0 | Bidirectional CRSF serial TX to external ELRS/Crossfire TX module (PCB test pad `TX` above `J15`) |
| `PA15` | **USART2_RX** | AF1 | Interrupt-driven telemetry RX with 128-byte lock-free ring buffer (PCB test pad `RX` above `J15`) |
| `PC13` | **MOD_PWR** | GPIO Out | External module power rail switch (Configurable polarity: Active HIGH or Active LOW) |

Implemented in [`src/crsf/uart.rs`](../src/crsf/uart.rs). Dedicated `USART2` interrupt handler clears hardware overrun (`USART_ISR_ORE`) and buffers incoming high-speed bytes with zero dropped packets during LCD flushes. Inter-byte silence timeout ($\ge 3\text{ ms}$) guarantees deterministic frame resynchronization.

### FS-i6S PCB Connection Points (Connector J15)
Unlike the older FS-i6X, the FS-i6S exposes `USART2` directly via circular solder test pads silk-screened **`TX`** and **`RX`** located immediately above connector **`J15`** (between the 8-pin harness and the metal RF module shield). 
- **`TX` Pad:** Directly connected to `PD5` (`USART2_TX`). Connect to external module **RX**.
- **`RX` Pad:** Directly connected to `PA15` (`USART2_RX`). Connect to external module **TX** (telemetry).
- Connector `J15` routes these signals down to the bottom daughterboard carrying the Micro-USB port and the 4-pin round mini-DIN trainer port.

### PA15 Hardware Decoupling & Touchscreen Collision Strategies
On the FS-i6S, `PA15` is physically shared between `USART2_RX` and the hardware reset line (`TOUCH_RST`) of the FocalTech **FT6236** capacitive touch screen controller (routed to pin 4 of FPC connector `J13` and the `TRST` test pad). 

During 420 kbaud full-duplex serial communication, incoming telemetry bursts toggle `PA15` rapidly. Several hardware strategies are available to prevent touchscreen resets or jitter:

1. **Passive Low-Pass RC Filter (Non-Destructive / No Trace Cutting):**
   - At 420,000 baud, individual serial low bit pulses are only $\sim 2.38\ \mu\text{s}$ (with a maximum continuous 8N1 zero burst of $\sim 21.4\ \mu\text{s}$). The FT6236 reset comparator requires a sustained low pulse of $\ge 1.0\text{ ms}$ (typically $2\text{--}5\text{ ms}$) to trip.
   - Adding a series $10\text{ k}\Omega$ resistor with a $100\text{ nF}$ capacitor to GND ($\tau = 1.0\text{ ms}$) on the touch reset line cleanly filters out 420 kbaud telemetry chatter while still allowing the initial 20 ms cold-boot reset pulse to pass.
   - Even if prolonged line breaks or unpowered modules momentarily reset the touch IC, it recovers almost instantly with zero noticeable impact unless coinciding with an active touch swipe.

2. **Permanent VDD Tie-High (Cleanest Full Decoupling):**
   - The FT6236 features an internal Power-On Reset (POR) circuit and internal pull-up on `RST`. Ongoing software reset control is not strictly required after initial power-up.
   - Desolder the small inline series resistor (or cut the trace) between `PA15` and Pin 4 of the touch connector `J13` (near `TRST`), then tie `J13` Pin 4 directly to 3.3V (or a 10 kΩ pull-up).
   - This leaves `PA15` 100% dedicated to `USART2_RX` with zero touch interference.

3. **Software GPIO Remap to Unused RF Daughterboard Pad:**
   - If the internal A7105 RF module is disabled/desoldered, Port E pins become free GPIOs.
   - Disconnect touch `RST` from `PA15` at `J13` and solder a jumper to the `RF_GIO1` pad (`PE14`, MCU pin 45).
   - In firmware (`src/touch/ft6236.rs`), remap `TOUCH_RST` from `PA15` to `PE14`.

4. **Single-Wire Half-Duplex on `PD5`:**
   - If no hardware modification or soldering to `PA15` is desired, CRSF can operate in single-wire half-duplex on `PD5` (`USART2` with `HDSEL = 1`), leaving `PA15` permanently driven HIGH as `TOUCH_RST`. 
   - Provides full bidirectional telemetry at 150 Hz and 250 Hz packet rates with zero touch collision risk.

### Power Switch Polarity (PC13)
Hardware power circuits vary depending on how external modules are wired to the transmitter:
- **Active HIGH** (Default): Drives PC13 high to enable an N-channel MOSFET or active-high switch.
- **Active LOW**: Drives PC13 low to enable a P-channel MOSFET or PNP power stage.

The polarity can be toggled in `Radio Setup` -> `Ext Module Power: HIGH/LOW` and is persisted in non-volatile Flash.

### Supported Baud Rates
Baud rates can be selected per-model in `9. Protocol Setup`:
- **420,000 baud** (Default for ExpressLRS high-speed communication)
- **416,666 baud** (Standard Team BlackSheep Crossfire)
- **115,200 baud** (Low-speed debug / legacy transmitters)
- **921,600 baud** (Ultra-low latency for compatible external microcontrollers)

---

## 2. Flight Dashboard: Native CRSF Link Diagnostics

When `rf_protocol` is set to `1` (`CRSF / ELRS`), Page 4/4 of the flight dashboard transitions from the AFHDS 2A packet counter into a real-time link diagnostics screen:

```
+---------------------------------------------------------------+
| [M01:DRONE   ]   CRSF: 250Hz               [ 4.8V]            |
+---------------------------------------------------------------+
| LQ:   100%            | PWR:  100mW                           |
| RSSI: -72dBm          | RATE: 250Hz (Mode 7)                  |
| SNR:  +12dB           | BAT:  16.4V                           |
| ANT:  1 (Active)      | CAP:  450mAh                          |
+---------------------------------------------------------------+
| P4/4           CRSF LINK DIAGNOSTICS                          |
+---------------------------------------------------------------+
```

### Metrics Decoded:
- **Link Quality (`LQ`)**: Decoded from CRSF Frame `0x14` (`CRSF_FRAMETYPE_LINK_STATISTICS`), byte 8 (`uplink_link_quality`). Displayed as `0..100%`.
- **RSSI (`RSSI 1`)**: Decoded from byte 3 (`uplink_rssi_1`). Displayed directly in `dBm` (e.g. `-72dBm`).
- **SNR (`SNR`)**: Decoded from byte 9 (`uplink_snr`). Displayed in signed `dB` (e.g. `+12dB` / `-4dB`).
- **Active Antenna (`ANT`)**: Decoded from byte 7 (`active_antenna`). Shows `1` or `2`.
- **TX Power (`PWR`)**: ExpressLRS Status frame / link statistics transmit power in milliwatts (`mW`).
- **RF Packet Rate (`RATE`)**: Automatically resolved from `rf_mode` index to human-readable rates (`50Hz`, `100Hz`, `150Hz`, `250Hz`, `333Hz`, `500Hz`, `D250`, `D500`, `F500`, `F1000`).
- **Flight Pack Battery (`BAT` / `CAP`)**: Decoded from CRSF Frame `0x08` (`CRSF_FRAMETYPE_BATTERY_SENSOR`). Displays battery voltage in `0.1V` precision and consumed capacity in `mAh`.

---

## 3. Native Module Configurator

On standard EdgeTX and OpenTX radios, ExpressLRS module configuration is typically handled via a Lua script. Because the FS-i6S's STM32F072 microcontroller has 16 KB of SRAM, running a full Lua virtual machine is impractical on this platform.

To enable full on-radio module configuration, `flysky-i6s-rs` implements the **bidirectional CRSF parameter protocol** natively in bare-metal Rust with **zero dynamic heap allocation**:

### Parameter Exchange Protocol
```text
Radio (FS-i6S)                               External ELRS TX Module
      |                                                 |
      | -------- 0x28 (DEVICE_PING) ------------------> |
      | <------- 0x29 (DEVICE_INFO: Name, Count) ------ |
      |                                                 |
      | loop for each parameter:                        |
      | -------- 0x2C (PARAM_READ: ID, Chunk) --------> |
      | <------- 0x2B (PARAM_ENTRY: Name, Options) ---- |
      |                                                 |
      | [User changes setting or clicks action]         |
      | -------- 0x2D (PARAM_WRITE: ID, Value) -------> |
```

### Navigating the Configurator (TBS-Agent Style):

#### Step 1: Open the Configurator & Device Picker
1. Long-press **`[OK]`** on the flight screen to open the Main Menu.
2. Scroll to `9. Protocol Setup` and press **`[OK]`**.
3. Highlight `[Configure Module]` and press **`[OK]`**.
4. The radio broadcasts discovery pings (`0x28 Ping`) and opens the **`CRSF DEVICES`** selection screen:
   ```text
   +-----------------------------------+
   | CRSF DEVICES                   |# |
   | > RM RP2                     [TX] |
   |   RM RP4TD-M 2400            [RX] |
   |   Betaflight                 [FC] |
   |   Radiomaster VTX           [VTX] |
   | [OK] Select            [ESC] Back |
   +-----------------------------------+
   ```
5. Use **`[UP]`** / **`[DOWN]`** to scroll through discovered devices. Up to **16 devices** can be discovered simultaneously across the CRSF bus (e.g. transmitter module, receivers, flight controller, VTX, ESCs, telemetry sensors, lighting controllers, or sound modules). When more than 4 devices are present, a right-edge vertical scrollbar is automatically displayed and the row selection highlight dynamically adapts so text and role tags never overlap the scrollbar track. Devices are discovered dynamically via 1 Hz broadcast pings; if a device disconnects or is powered off, it is automatically pruned after 3 seconds.
6. Press **`[OK]`** to select that device and load its parameters immediately at full wire speed.

#### Step 2: Hierarchical Folder Navigation
- Parameters are grouped logically in folders per the module's firmware (e.g. `VTX Admin >`, `Wi-Fi Options >`).
- Folders display with a trailing chevron indicator (`>`).
- Press **`[OK]`** on a folder to drill down into its sub-parameters. The header updates to show the active folder name.
- When navigating deeply nested subfolders (up to 6 levels), the full folder stack preserves each folder's display name. Pressing **`[ESC]`** ascends back to the parent folder and restores the parent's actual title in the header (never generic `"Folder"`).
- Press **`[ESC]`** at the root parameter level to return to the **`CRSF DEVICES`** picker to switch devices.

#### Step 3: In-Place Modal Parameter Editing (Select & Integer)
- **Selection Parameters** (e.g., `Packet Rate`, `Power`):
  - Highlight the setting and press **`[OK]`** (footer displays `[OK] Edit`).
  - The field enters **Edit Mode**, displayed with selection brackets: `< 250Hz >`.
  - Press **`[UP]`** or **`[DOWN]`** to preview and cycle through options locally on screen without emitting premature serial commands.
  - Press **`[OK]`** to commit your choice: the radio transmits the `0x2D Param Write` frame over USART2 to the module and exits Edit Mode.
  - Press **`[ESC]`** to cancel editing without saving.
- **Integer Parameters** (e.g., receiver PWM channel output mapping `Output 1`, `Output 2`, offsets, trims):
  - Values and their unit strings (e.g., `4 ch`, `100 %`, `250 mW`, `0 us`) are rendered clearly on the right.
  - Press **`[OK]`** to enter modal editing: displays `< 4 ch >`.
  - Press **`[UP]`** / **`[DOWN]`** to increment or decrement the numeric value, automatically clamped within the device's allowable `[min, max]` limits.
  - Press **`[OK]`** to transmit the write frame (1-byte for `UINT8`/`INT8`, 2-byte big-endian for `UINT16`/`INT16`) to the receiver or transmitter module.
  - Press **`[ESC]`** to cancel without altering the setting.

#### Step 4: Triggering Command Actions
- Highlight an action command (such as `[Bind]` or `[Wi-Fi Mode]`) and press **`[OK]`**.
- If confirmation is required, an overlay prompt appears (`Run: [Bind]? [OK] Yes [ESC] No`).
- Press **`[OK]`** to execute or **`[ESC]`** to cancel. The status updates in real-time (`[Executing...]` -> `[Cmd]`).

> [!TIP]
> For the complete byte-level framing breakdown, wire timing diagrams, CRC-8 formulas, and state machine transitions, refer to the [CRSF Protocol Specification & Verification Guide](CRSF_PROTOCOL_SPEC.md).

---

## 4. USB CDC Telemetry Streaming

In `Serial` or `Composite` USB mode, the transmitter streams JSON telemetry over the virtual COM port (`i6s> stream` or `i6s> telem`):

```json
{
  "vbat": 5.18,
  "rssi": 98,
  "rx_v": 5.02,
  "tx": 15820,
  "rx": 15798,
  "err": 22,
  "crsf": {
    "conn": true,
    "lq": 100,
    "rssi_dbm": -72,
    "snr_db": 12,
    "ant": 1,
    "pwr_mw": 100,
    "rf_rate": "250Hz",
    "rx_vbat_mv": 16400,
    "rx_cap_mah": 450
  },
  "ch": [1500, 1500, 1000, 1500, 1500, 1500, 1500, 1500, 1500, 1500, 1500, 1500, 1500, 1500]
}
```

The CLI `status` command reports the active protocol:
```text
i6s> status
FlySky FS-i6S Rust Firmware v0.1.0
Protocol: CRSF / ExpressLRS (PD5 UART active)
{"vbat":5.18,...}
```
