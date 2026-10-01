//! Interactive endpoint calibration subprogram.
//!
//! Guides the user through a 2-step wizard:
//! 1. Center all sticks and pots (Throttle centered to 50%) -> captures neutral points.
//! 2. Move sticks and pots to physical limits -> captures min/max extents.
//!
//! Applies OpenTX-style margins (~2%) and saves to Flash.

use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{Line, PrimitiveStyle, Rectangle},
    text::Text,
};

use crate::adc;
use crate::buzzer::Buzzer;
use crate::display::St7567;
use crate::input;
use crate::storage::{self, ChannelCalib};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CalibStep {
    Inactive,
    Center,
    Limits,
    Complete,
}

pub struct CalibWizard {
    pub step: CalibStep,
    centers: [u16; 10], // 0..3: Sticks; 4..5: Stock Pots; 6..9: Ext Pots (P7)
    mins: [u16; 10],
    maxs: [u16; 10],
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
            centers: [2048; 10],
            mins: [4095; 10],
            maxs: [0; 10],
            prev_keys: 0xFFFF,
            waiting_release: false,
            timer_ms: 0,
        }
    }

    /// Start the calibration wizard.
    pub fn start(&mut self, buzzer: &mut Buzzer) {
        self.step = CalibStep::Center;
        self.centers = [2048; 10];
        self.mins = [4095; 10];
        self.maxs = [0; 10];
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
        if self.waiting_release
            && (keys & (1 << 10)) == 0 {
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

        let text_style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
        let border_style = PrimitiveStyle::with_stroke(BinaryColor::On, 1);

        // Map raw ADC to wizard channels:
        // [0] PA0: Roll
        // [1] PA1: Pitch
        // [2] PA2: Throttle
        // [3] PA3: Yaw
        // [4] PA6: VRA
        // [5] PA7: VRB
        // [6] PC2: VRC (AD12 on Header P7)
        // [7] PC3: VRD (AD13 on Header P7)
        // [8] PC4: VRE (AD14 on Header P7)
        // [9] PC5: VRF (AD15 on Header P7)
        let current_raw = [
            raw_adc[0],
            raw_adc[1],
            raw_adc[2],
            raw_adc[3],
            raw_adc[6],
            raw_adc[7],
            raw_adc[11],
            raw_adc[12],
            raw_adc[13],
            raw_adc[14],
        ];

        match self.step {
            CalibStep::Inactive => {}

            CalibStep::Center => {
                if cancel_pressed {
                    self.step = CalibStep::Inactive;
                    buzzer.click();
                    return;
                }

                if ok_pressed {
                    self.centers.copy_from_slice(&current_raw);
                    self.mins.copy_from_slice(&current_raw);
                    self.maxs.copy_from_slice(&current_raw);
                    self.step = CalibStep::Limits;
                    self.waiting_release = true; // Must release OK before accepting save in step 2!
                    buzzer.play_tone(2400, 50);
                    return;
                }

                // Render Step 1
                lcd.clear(BinaryColor::Off).ok();
                Text::new("CALIBRATION (1/2)", Point::new(12, 9), text_style).draw(lcd).ok();
                Line::new(Point::new(0, 11), Point::new(127, 11)).into_styled(border_style).draw(lcd).ok();

                Text::new("1. Center all sticks", Point::new(2, 22), text_style).draw(lcd).ok();
                Text::new("   & rotary pots.", Point::new(2, 32), text_style).draw(lcd).ok();
                Text::new("2. Move THR to middle!", Point::new(2, 43), text_style).draw(lcd).ok();

                Line::new(Point::new(0, 48), Point::new(127, 48)).into_styled(border_style).draw(lcd).ok();
                if self.waiting_release {
                    Text::new("Release [OK] key...", Point::new(4, 59), text_style).draw(lcd).ok();
                } else {
                    Text::new("[OK] Next  [ESC] Exit", Point::new(2, 59), text_style).draw(lcd).ok();
                }
            }

            CalibStep::Limits => {
                if cancel_pressed {
                    self.step = CalibStep::Inactive;
                    buzzer.click();
                    return;
                }

                // Continuously track minimum and maximum reached
                for (i, &raw) in current_raw.iter().enumerate() {
                    if raw < self.mins[i] {
                        self.mins[i] = raw;
                    }
                    if raw > self.maxs[i] {
                        self.maxs[i] = raw;
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
                            let span_neg = ((center.saturating_sub(self.mins[i]) as u32 * 63) / 64) as u16;
                            let span_pos = ((self.maxs[i].saturating_sub(center) as u32 * 63) / 64) as u16;
                            storage.radio.sticks[i] = ChannelCalib::new(
                                center.saturating_sub(span_neg),
                                center,
                                center.saturating_add(span_pos),
                            );
                        }

                        // Pots: if moved by >= 400 counts total span, update; otherwise preserve
                        for i in 0..2 {
                            let p_idx = 4 + i;
                            let span = self.maxs[p_idx].saturating_sub(self.mins[p_idx]);
                            if span >= 400 {
                                let center = self.centers[p_idx];
                                let span_neg = ((center.saturating_sub(self.mins[p_idx]) as u32 * 63) / 64) as u16;
                                let span_pos = ((self.maxs[p_idx].saturating_sub(center) as u32 * 63) / 64) as u16;
                                storage.radio.pots[i] = ChannelCalib::new(
                                    center.saturating_sub(span_neg),
                                    center,
                                    center.saturating_add(span_pos),
                                );
                            }
                        }

                        // Ext Pots: if ext_adc is active, update any ext pot moved by >= 400 counts
                        if storage.radio.ext_adc != 0 {
                            for i in 0..4 {
                                let p_idx = 6 + i;
                                let span = self.maxs[p_idx].saturating_sub(self.mins[p_idx]);
                                if span >= 400 {
                                    let center = self.centers[p_idx];
                                    let span_neg = ((center.saturating_sub(self.mins[p_idx]) as u32 * 63) / 64) as u16;
                                    let span_pos = ((self.maxs[p_idx].saturating_sub(center) as u32 * 63) / 64) as u16;
                                    storage.radio.ext_pots[i] = ChannelCalib::new(
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
                lcd.clear(BinaryColor::Off).ok();
                Text::new("CALIBRATION (2/2)", Point::new(12, 9), text_style).draw(lcd).ok();
                Line::new(Point::new(0, 11), Point::new(127, 11)).into_styled(border_style).draw(lcd).ok();

                let labels = ["A", "E", "T", "R"];
                for i in 0..4 {
                    let y = 13 + (i as i32 * 8);
                    Text::new(labels[i], Point::new(2, y + 6), text_style).draw(lcd).ok();

                    // Outline gauge box (width = 63, from 10 to 72; inner x = 11..71)
                    Rectangle::new(Point::new(10, y), Size::new(63, 7))
                        .into_styled(border_style)
                        .draw(lcd)
                        .ok();

                    // Center tick mark at 41 (left inner: 11..40 = 30px, right inner: 42..71 = 30px)
                    Line::new(Point::new(41, y), Point::new(41, y + 6))
                        .into_styled(border_style)
                        .draw(lcd)
                        .ok();

                    // Display extent covered from center
                    let center = self.centers[i];
                    let span_neg = center.saturating_sub(self.mins[i]);
                    let span_pos = self.maxs[i].saturating_sub(center);

                    // Target nominal span for full gauge travel:
                    // Horizontal (A, R) travel is ~1600 counts; Vertical (E, T) is ~1450 counts.
                    // Using 1350 (H) / 1250 (V) ensures every physical axis reaches 100% of the box edges.
                    let stick_target = if i == 0 || i == 3 { 1350u32 } else { 1250u32 };

                    // Left fill: up to 30 pixels left (reaches x = 11)
                    let left_w = ((span_neg as u32 * 30) / stick_target).min(30) as i32;
                    if left_w > 0 {
                        Line::new(Point::new(41 - left_w, y + 3), Point::new(41, y + 3))
                            .into_styled(border_style)
                            .draw(lcd)
                            .ok();
                    }

                    // Right fill: up to 30 pixels right (reaches x = 71)
                    let right_w = ((span_pos as u32 * 30) / stick_target).min(30) as i32;
                    if right_w > 0 {
                        Line::new(Point::new(41, y + 3), Point::new(41 + right_w, y + 3))
                            .into_styled(border_style)
                            .draw(lcd)
                            .ok();
                    }

                    // Current stick position tick mark
                    let cur = current_raw[i];
                    let delta = cur as i32 - center as i32;
                    let cur_x = if delta < 0 {
                        41 - ((delta.unsigned_abs() * 30) / stick_target).min(30) as i32
                    } else {
                        41 + ((delta as u32 * 30) / stick_target).min(30) as i32
                    };
                    Line::new(Point::new(cur_x, y + 1), Point::new(cur_x, y + 5))
                        .into_styled(border_style)
                        .draw(lcd)
                        .ok();

                    // Stick status text
                    if stick_ready[i] {
                        Text::new("OK", Point::new(75, y + 6), text_style).draw(lcd).ok();
                    } else {
                        Text::new("--", Point::new(75, y + 6), text_style).draw(lcd).ok();
                    }
                }

                // Pot scaling: pots swing ~800..1300 counts around center. 900 counts fills 16 pixels.
                let pot_target = 900u32;

                draw_pot_gauge(
                    lcd, "V1 OK", "V1 --", 19, 21,
                    self.centers[4], self.mins[4], self.maxs[4], current_raw[4],
                    pot_target, text_style, border_style,
                );
                draw_pot_gauge(
                    lcd, "V2 OK", "V2 --", 35, 37,
                    self.centers[5], self.mins[5], self.maxs[5], current_raw[5],
                    pot_target, text_style, border_style,
                );

                Line::new(Point::new(0, 48), Point::new(127, 48)).into_styled(border_style).draw(lcd).ok();
                if self.waiting_release {
                    Text::new("Release [OK] key...", Point::new(4, 59), text_style).draw(lcd).ok();
                } else if all_ready {
                    Text::new("[OK] Save  [ESC] Exit", Point::new(2, 59), text_style).draw(lcd).ok();
                } else {
                    Text::new("Stir sticks & pots", Point::new(2, 59), text_style).draw(lcd).ok();
                }
            }

            CalibStep::Complete => {
                lcd.clear(BinaryColor::Off).ok();
                Text::new("CALIBRATION SAVED!", Point::new(10, 24), text_style).draw(lcd).ok();
                Text::new("Flash updated OK", Point::new(14, 38), text_style).draw(lcd).ok();

                if self.timer_ms > dt_ms {
                    self.timer_ms -= dt_ms;
                } else {
                    self.step = CalibStep::Inactive;
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_pot_gauge(
    lcd: &mut St7567,
    label_ok: &'static str,
    label_wait: &'static str,
    text_y: i32,
    box_y: i32,
    center: u16,
    min: u16,
    max: u16,
    current: u16,
    pot_target: u32,
    text_style: MonoTextStyle<'_, BinaryColor>,
    border_style: PrimitiveStyle<BinaryColor>,
) {
    let span_neg = center.saturating_sub(min);
    let span_pos = max.saturating_sub(center);
    let moved = (span_neg + span_pos) >= 400;
    let txt = if moved { label_ok } else { label_wait };
    Text::new(txt, Point::new(92, text_y), text_style).draw(lcd).ok();
    Rectangle::new(Point::new(92, box_y), Size::new(35, 7))
        .into_styled(border_style)
        .draw(lcd)
        .ok();
    Line::new(Point::new(109, box_y), Point::new(109, box_y + 6))
        .into_styled(border_style)
        .draw(lcd)
        .ok();

    let left_w = ((span_neg as u32 * 16) / pot_target).min(16) as i32;
    if left_w > 0 {
        Line::new(Point::new(109 - left_w, box_y + 3), Point::new(109, box_y + 3))
            .into_styled(border_style)
            .draw(lcd)
            .ok();
    }
    let right_w = ((span_pos as u32 * 16) / pot_target).min(16) as i32;
    if right_w > 0 {
        Line::new(Point::new(109, box_y + 3), Point::new(109 + right_w, box_y + 3))
            .into_styled(border_style)
            .draw(lcd)
            .ok();
    }

    let delta = current as i32 - center as i32;
    let mark_x = if delta < 0 {
        109 - ((delta.unsigned_abs() * 16) / pot_target).min(16) as i32
    } else {
        109 + ((delta as u32 * 16) / pot_target).min(16) as i32
    };
    Line::new(Point::new(mark_x, box_y + 1), Point::new(mark_x, box_y + 5))
        .into_styled(border_style)
        .draw(lcd)
        .ok();
}
