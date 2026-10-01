# FS-i6S Open-Source Ecosystem & Architectural Context

The FlySky FS-i6S is an advanced, touch-enabled RC transmitter powered by an ARM Cortex-M0 microcontroller (STM32F072VB). This document provides context on the open-source firmware evolution for this platform and the technical design philosophies behind `flysky-i6s-rs`.

---

## 1. Heritage & Historical Foundations

The open-source firmware journey on the FlySky platform was pioneered by **Kuba (qba667)**, **Janek (ajjjjjjjj)**, and the contributors to the **OpenI6X** project. Their work proved that modern transmitter architectures could run effectively on constrained Cortex-M0 microcontrollers with 128 KB Flash and 16 KB SRAM, deciphering:
- ST7567 LCD 8-bit parallel bus timings and initialization sequences.
- Amiccom A7105 SPI transceiver control registers and RF hopping algorithms.
- Hardware timer and ADC peripheral routing on STM32F072.

`flysky-i6s-rs` builds directly on this heritage, bringing a standalone, clean-slate `no_std` Rust architecture to the **FlySky FS-i6S**.

---

## 2. FS-i6S Hardware Evolution: Touchscreen & Electronic Power

While sharing the STM32F072VB core and A7105 RF transceiver with the older FS-i6X, the **FlySky FS-i6S** introduces fundamental hardware changes:

1. **FocalTech FT6236 Capacitive Touchscreen:**
   - Replaces the mechanical 3×4 key matrix with a hardware I2C1 capacitive multi-touch panel (`PB8` SCL, `PB9` SDA @ 400 kHz, `PA15` RST, `PC12` INT).
   - Introduces swipe gesture navigation (Up/Down/Left/Right) and direct hitboxes.
2. **Electronic Power Latching & Soft Shutdown:**
   - `PB15` electronic power latch must be asserted HIGH immediately at boot to sustain the power rail.
   - `PB14` soft power button sense detects hold duration (>= 1.5s), triggering an animated shutdown sequence with safe Flash storage compaction before cutting power.
   - Dual blue power LEDs on `PD10`/`PD11`.
3. **Rear Tactile Buttons & Digital Trims:**
   - Dedicated rear push-buttons on `PA9` (Cancel / Back) and `PA10` (OK / Select).
   - Replaces mechanical trim rockers with two high-precision digital trimming modes:
     * **Stick Modifier Mode:** Hold rear buttons while deflecting gimbals for instant thumb-on-stick trimming.
     * **Virtual Touch Targets:** Direct tap zones along the flight screen perimeter.
4. **Zero-Disassembly DFU Bootloader:**
   - Cold-boot combo: Holding **Rear Left (`PA9`) + Rear Right (`PA10`)** during power-on jumps straight into factory ROM DFU without opening the radio case.
   - Software menu trigger: `Diag -> [OK] Reboot DFU` (via SRAM flag `0xDEADBEEF`).
   - USB CDC CLI command: `dfu` or `reboot bootloader`.

---

## 3. Design Philosophy of `flysky-i6s-rs`

- **Clean-Slate Bare-Metal `no_std` Rust:** Zero dynamic heap allocation (`alloc` free), zero allocator stalls, and guaranteed memory safety on Cortex-M0.
- **Deterministic RF Timing:** Dedicated hardware timer (`TIM16`) for frame synchronization, autonomous DMA ADC scanning, and a decoupled execution loop.
- **Embedded CRSF & ExpressLRS Engine:** Native bidirectional CRSF parameter parsing directly in Rust to configure external modules on the touch interface without a Lua engine.
- **Safety First & Instant Recovery:** Watchdog reset recovery in $< 2\text{ ms}$ bypassing interlocks, and safe hold-to-shutdown Flash flushing.

---

## 4. Backing Up, Flashing, & Reversion

Because the STM32F072 features a permanent factory DFU bootloader in System ROM, pilots can explore different firmwares safely with **zero disassembly**:

1. **Enter Bootloader Mode:**
   - Hold both rear push-buttons (**`PA9` + `PA10`**) while powering ON.
2. **Back Up Current Firmware (Recommended):**
   ```bash
   dfu-util -a 0 -s 0x08000000:131072 -U backup_full.bin
   ```
3. **Flash flysky-i6s-rs:**
   ```bash
   dfu-util -a 0 -s 0x08000000:leave -D flysky-i6s.bin
   ```
4. **Restore Anytime:**
   ```bash
   dfu-util -a 0 -s 0x08000000:leave -D backup_full.bin
   ```

For detailed technical documentation on the internal architecture of `flysky-i6s-rs`, please see [ARCHITECTURE.md](ARCHITECTURE.md).
