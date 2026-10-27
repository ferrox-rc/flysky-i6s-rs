# Aftermarket Hall Effect Gimbal Guide

Guide for installing, wiring, and configuring aftermarket Hall effect gimbals in `ferrox` (`flysky-i6x-rs`).

---

## 1. Supported Hardware

`ferrox` supports standard 3-wire analog ratiometric Hall effect gimbals, including:
- **RadioMaster AG01 Mini** (CNC Hall Gimbals for TX12 / Zorro)
- **RadioMaster Pocket Hall Gimbals**
- **FlySky FS-HZCZ03-ADJ** (Nirvana / Paladin Hall Gimbals)
- **Jumper T-Pro / T-Lite Hall Gimbals**

---

## 2. Electrical Pinout & Wiring

Each gimbal axis (Horizontal and Vertical) uses a standard 3-pin harness:
1. **`VCC` (3.3V)**: Must connect to the regulated **3.3V** supply rail on the mainboard.
   > [!CAUTION]
   > **DO NOT** power Hall sensor boards from battery voltage (4.0V - 6.0V). The STM32F072 ADC pins have an absolute maximum rating of VDDA + 0.3V ≈ 3.6V. Connecting sensors to battery voltage will permanently destroy the microcontroller.
2. **`GND`**: Connect to circuit ground.
3. **`SIGNAL` (ADC Input)**:
   - **RH (Right Horizontal)**: `PA0` (ADC Channel 0)
   - **RV (Right Vertical)**: `PA1` (ADC Channel 1)
   - **LV (Left Vertical)**: `PA2` (ADC Channel 2)
   - **LH (Left Horizontal)**: `PA3` (ADC Channel 3)

---

## 3. Firmware Configuration

### Radio Setup Toggle
In the radio menu:
1. Long-press **[OK]** -> Navigate to **`RADIO SETUP`**.
2. Scroll to **`Gimbals:`**.
3. Press **[OK]** to toggle between:
   - **`POT`** (Standard potentiometers: 0 count deadband, optimized for resistive sweeps)
   - **`HALL`** (Hall effect sensors: 3-count central magnetic ripple suppression deadband, adapted calibration limits)

> [!TIP]
> **Can you use `HALL` mode on standard potentiometer gimbals?**
> Yes. The setting is purely a software processing toggle on the ADC readings and causes no hardware or electrical issues. Running `HALL` on stock potentiometers simply adds the small ±3-count (~0.2%) center deadband, which can actually be helpful on older or worn potentiometers to eliminate resting stick jitter around center. Both `POT` and `HALL` modes retain full -1000..+1000 throw.

### Calibration Routine
After installing Hall effect gimbals:
1. Navigate to **`CALIBRATION`** in the main menu.
2. **Step 1 (Neutral):** Center all sticks (including Throttle to 50%) and press **[OK]**.
3. **Step 2 (Extents):** Move all sticks to their physical extremes in circular and diagonal patterns.
   - In `HALL` mode, minimum required travel per direction is relaxed to 200 counts to accommodate compact magnet swing arcs.
4. Press **[OK]** to save the calibration to flash.

---

## 4. Mechanical Mounting

- Mini Hall gimbals (RadioMaster TX12/Zorro/Pocket size) require a 3D printed adapter plate to align with the transmitter outer shell screw posts.
- Ensure the stick gimbal wiring harness has sufficient slack throughout the entire stick throw range to avoid tension or wire pinching.
