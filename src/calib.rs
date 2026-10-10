//! Interactive endpoint calibration subprogram.
//!
//! Guides the user through a 2-step wizard:
//! 1. Center all sticks and pots (Throttle centered to 50%) -> captures neutral points.
//! 2. Move sticks and pots to physical limits -> captures min/max extents.
//!
//! Reuses the widgets library and dynamic analog input discovery from gimbals.
//! Applies OpenTX-style margins (~2%) and saves to Flash.

use crate::adc;
use crate::buzzer::Buzzer;
use crate::display::St7567;
use crate::input;
use crate::storage::{self, ChannelCalib};
use crate::ui::dashboard::pages::gimbals::{self, collect_configured_pots, ConfiguredPot};
use crate::ui::widgets;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CalibStep {
    Inactive,
    Center,
    Limits,
    Complete,
}

pub struct CalibWizard {
    pub step: CalibStep,
    centers: [u16; 6], // 0..3: Sticks; 4..5: Aux channels
    mins: [u16; 6],
    maxs: [u16; 6],
    prev_keys: u16,
    waiting_release: bool,
    timer_ms: u16,
}

impl Default for CalibWizard {
    fn default() -> Self {
        Self::new()
    }
}

impl CalibWizard {
    pub const fn new() -> Self {
        Self {
            step: CalibStep::Inactive,
            centers: [2048; 6],
            mins: [4095; 6],
            maxs: [0; 6],
            prev_keys: 0xFFFF,
            waiting_release: false,
            timer_ms: 0,
        }
    }

    /// Start the calibration wizard.
    pub fn start(&mut self, buzzer: &mut Buzzer) {
        self.step = CalibStep::Center;
        self.centers = [2048; 6];
        self.mins = [4095; 6];
        self.maxs = [0; 6];
        self.prev_keys = 0xFFFF; // Block any immediate edge trigger
        self.waiting_release = true; // User must release OK before proceeding
        self.timer_ms = 0;
        buzzer.chime_calib_start();
    }

    /// Returns true if the wizard is currently active.
    pub fn is_active(&self) -> bool {
        self.step != CalibStep::Inactive
    }

    /// Update wizard state and render screen.
    pub fn update(
        &mut self,
        lcd: &mut St7567,
        storage: &mut crate::storage::RadioStorage,
        raw_adc: &[u16; adc::NUM_CHANNELS],
        keys: u16,
        dt_ms: u16,
        buzzer: &mut Buzzer,
    ) {
        // Debounce / release tracking for OK button (bit 10)
        if self.waiting_release && (keys & (1 << 10)) == 0 {
            self.waiting_release = false;
        }

        let newly_pressed = if self.waiting_release {
            0
        } else {
            keys & !self.prev_keys
        };
        self.prev_keys = keys;

        let ok_pressed = (newly_pressed & (1 << 10)) != 0;
        let cancel_pressed = (newly_pressed & (1 << 11)) != 0;

        // Map primary stick ADC readings (PA0..PA3)
        let current_sticks = [raw_adc[0], raw_adc[1], raw_adc[2], raw_adc[3]];

        // Dynamically inspect all configured analog potentiometer inputs
        let (configured_pots, pot_count) = collect_configured_pots(storage);

        match self.step {
            CalibStep::Inactive => {}

            CalibStep::Center => {
                if cancel_pressed {
                    self.step = CalibStep::Inactive;
                    buzzer.click();
                    return;
                }

                if ok_pressed {
                    // Capture center positions for the 4 sticks
                    self.centers[0..4].copy_from_slice(&current_sticks);
                    self.mins[0..4].copy_from_slice(&current_sticks);
                    self.maxs[0..4].copy_from_slice(&current_sticks);

                    // Capture center positions for all active pots
                    for pot in &configured_pots[..pot_count] {
                        let ch = pot.ch;
                        let val = gimbals::aux_channel_raw_adc(raw_adc, ch);
                        self.centers[ch] = val;
                        self.mins[ch] = val;
                        self.maxs[ch] = val;
                    }

                    self.step = CalibStep::Limits;
                    self.waiting_release = true; // Must release OK before accepting save in step 2!
                    buzzer.play_tone(2400, 50);
                    return;
                }

                // Render Step 1 using standard header & footer widgets
                lcd.clear_buffer();
                widgets::draw_header(lcd, "CALIBRATION (1/2)");

                lcd.draw_str_6x10(2, 15, "1. Center all sticks", false);
                lcd.draw_str_6x10(2, 25, "   & rotary pots.", false);
                lcd.draw_str_6x10(2, 36, "2. Move THR to middle!", false);

                if self.waiting_release {
                    widgets::draw_footer(lcd, "Release [OK] key...");
                } else {
                    widgets::draw_footer_split(lcd, "[OK] Next", "[ESC] Exit");
                }
            }

            CalibStep::Limits => {
                if cancel_pressed {
                    self.step = CalibStep::Inactive;
                    buzzer.click();
                    return;
                }

                // Track physical limits for sticks
                for (i, &raw) in current_sticks.iter().enumerate() {
                    if raw < self.mins[i] {
                        self.mins[i] = raw;
                    }
                    if raw > self.maxs[i] {
                        self.maxs[i] = raw;
                    }
                }

                // Track physical limits for all active pots
                for pot in &configured_pots[..pot_count] {
                    let ch = pot.ch;
                    let raw = gimbals::aux_channel_raw_adc(raw_adc, ch);
                    if raw < self.mins[ch] {
                        self.mins[ch] = raw;
                    }
                    if raw > self.maxs[ch] {
                        self.maxs[ch] = raw;
                    }
                }

                // Check readiness of sticks (each stick must move at least 250 counts in each direction)
                let mut ready_count = 0u8;
                let mut stick_ready = [false; 4];
                for (i, ready) in stick_ready.iter_mut().enumerate() {
                    let span_neg = self.centers[i].saturating_sub(self.mins[i]);
                    let span_pos = self.maxs[i].saturating_sub(self.centers[i]);
                    if span_neg >= 250 && span_pos >= 250 {
                        *ready = true;
                        ready_count += 1;
                    }
                }

                let all_ready = ready_count == 4;

                if ok_pressed {
                    if all_ready {
                        // Apply OpenTX STICK_TOLERANCE 64 margin (62/64 = ~96.8%)
                        storage.radio.magic = storage::FLASH_MAGIC;
                        storage.radio.version = storage::CONFIG_VERSION;

                        for i in 0..4 {
                            let center = self.centers[i];
                            let span_neg =
                                ((center.saturating_sub(self.mins[i]) as u32 * 63) / 64) as u16;
                            let span_pos =
                                ((self.maxs[i].saturating_sub(center) as u32 * 63) / 64) as u16;
                            storage.radio.sticks[i] = ChannelCalib::new(
                                center.saturating_sub(span_neg),
                                center,
                                center.saturating_add(span_pos),
                            );
                        }

                        // Save calibrated pot ranges for any active pot moved by >= 400 counts
                        for pot in &configured_pots[..pot_count] {
                            let ch = pot.ch;
                            let span = self.maxs[ch].saturating_sub(self.mins[ch]);
                            if span >= 400 {
                                let center = self.centers[ch];
                                let span_neg =
                                    ((center.saturating_sub(self.mins[ch]) as u32 * 63) / 64) as u16;
                                let span_pos =
                                    ((self.maxs[ch].saturating_sub(center) as u32 * 63) / 64) as u16;
                                storage.radio.aux_pots[ch] = storage::PotCalib::new(
                                    center.saturating_sub(span_neg),
                                    center,
                                    center.saturating_add(span_pos),
                                );

                                // Backward compatibility for legacy pots (VRA = ch 4, VRB = ch 5)
                                if ch == 4 {
                                    storage.radio.pots[0] = ChannelCalib::new(
                                        center.saturating_sub(span_neg),
                                        center,
                                        center.saturating_add(span_pos),
                                    );
                                } else if ch == 5 {
                                    storage.radio.pots[1] = ChannelCalib::new(
                                        center.saturating_sub(span_neg),
                                        center,
                                        center.saturating_add(span_pos),
                                    );
                                }
                            }
                        }

                        input::apply_calibration(&storage.radio);
                        storage::save_radio_config(storage);

                        buzzer.chime_calib_success();
                        self.step = CalibStep::Complete;
                        self.timer_ms = 1200; // Display success banner for 1.2s
                    } else {
                        // Warning buzz if not all sticks have been moved to limits
                        buzzer.play_tone(1100, 100);
                    }
                    return;
                }

                // Render Step 2
                lcd.clear_buffer();
                widgets::draw_header(lcd, "CALIBRATION (2/2)");

                let is_general = storage.active_model().model_type().is_general();
                let stick_labels = if is_general {
                    ["1", "2", "3", "4"]
                } else {
                    ["A", "E", "T", "R"]
                };

                // Render flight sticks using the dedicated widget
                widgets::draw_calib_sticks(
                    lcd,
                    &stick_labels,
                    &self.centers[0..4],
                    &self.mins[0..4],
                    &self.maxs[0..4],
                    &current_sticks,
                    &stick_ready,
                );

                // Dynamically render all configured potentiometers
                draw_calib_pots(
                    lcd,
                    &configured_pots[..pot_count],
                    &self.centers,
                    &self.mins,
                    &self.maxs,
                    raw_adc,
                );

                if self.waiting_release {
                    widgets::draw_footer(lcd, "Release [OK] key...");
                } else if all_ready {
                    widgets::draw_footer_split(lcd, "[OK] Save", "[ESC] Exit");
                } else {
                    widgets::draw_footer_split(lcd, "Stir sticks & pots", "[ESC] Exit");
                }
            }

            CalibStep::Complete => {
                lcd.clear_buffer();
                widgets::draw_header(lcd, "CALIBRATION (OK)");
                lcd.draw_str_6x10(10, 19, "CALIBRATION SAVED!", false);
                lcd.draw_str_6x10(14, 33, "Flash updated OK", false);
                widgets::draw_footer(lcd, "Ready");

                if self.timer_ms > dt_ms {
                    self.timer_ms -= dt_ms;
                } else {
                    self.step = CalibStep::Inactive;
                }
            }
        }
    }
}

/// Dynamically render active potentiometer calibration gauges on the right side of the screen.
fn draw_calib_pots(
    lcd: &mut St7567,
    configured_pots: &[ConfiguredPot],
    centers: &[u16; 6],
    mins: &[u16; 6],
    maxs: &[u16; 6],
    raw_adc: &[u16; adc::NUM_CHANNELS],
) {
    let count = configured_pots.len();
    if count == 0 {
        return;
    }

    let pot_target = 900u32;

    if count <= 2 {
        for (i, pot) in configured_pots.iter().enumerate() {
            let ch = pot.ch;
            let center = centers[ch];
            let min = mins[ch];
            let max = maxs[ch];
            let cur = gimbals::aux_channel_raw_adc(raw_adc, ch);
            let moved = max.saturating_sub(min) >= 400;

            let (text_y, box_y) = if count == 1 {
                (18, 27)
            } else if i == 0 {
                (12, 21)
            } else {
                (28, 37)
            };

            lcd.draw_str_6x10(92, text_y, pot.name, false);
            let status = if moved { "OK" } else { "--" };
            lcd.draw_str_6x10(110, text_y, status, false);

            widgets::draw_calib_gauge(lcd, 92, box_y, 35, 7, center, min, max, cur, pot_target);
        }
    } else if count <= 4 {
        for (i, pot) in configured_pots.iter().enumerate() {
            let ch = pot.ch;
            let center = centers[ch];
            let min = mins[ch];
            let max = maxs[ch];
            let cur = gimbals::aux_channel_raw_adc(raw_adc, ch);
            let moved = max.saturating_sub(min) >= 400;

            let y = 13 + (i as i32 * 8);

            lcd.draw_str_4x6(89, y + 1, pot.name, false);
            let status = if moved { "OK" } else { "--" };
            lcd.draw_str_4x6(99, y + 1, status, false);

            widgets::draw_calib_gauge(lcd, 110, y, 17, 7, center, min, max, cur, pot_target);
        }
    } else {
        let col_w = 18;
        let left_x = 89;
        let right_x = 108;
        let rows_per_col = count.div_ceil(2);

        for (i, pot) in configured_pots.iter().enumerate() {
            let ch = pot.ch;
            let center = centers[ch];
            let min = mins[ch];
            let max = maxs[ch];
            let cur = gimbals::aux_channel_raw_adc(raw_adc, ch);
            let moved = max.saturating_sub(min) >= 400;

            let col = i / rows_per_col;
            let row = i % rows_per_col;
            let x = if col == 0 { left_x } else { right_x };
            let y = 13 + (row as i32 * 7);

            lcd.draw_str_4x6(x, y, pot.name, false);
            if moved {
                lcd.draw_rect(x + 9, y, col_w - 9, 6, true);
            }
            widgets::draw_calib_gauge(
                lcd,
                x + 9,
                y,
                col_w - 9,
                6,
                center,
                min,
                max,
                cur,
                pot_target,
            );
        }
    }
}
