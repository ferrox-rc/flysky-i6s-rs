//! Live Flight Dashboard subsystem for FlySky FS-i6X.
//!
//! Provides the primary 4-page flight display:
//! - Page 0 (P1/4): Primary Gimbals, Trims, Switches, Pots
//! - Page 1 (P2/4): 14-Channel Dual-Column Live Monitor
//! - Page 2 (P3/4): Model Dashboard, Type, RX ID, Throttle Curve
//! - Page 3 (P4/4): Telemetry & RF Diagnostics (CRSF Link Diag / AFHDS 2A)

pub mod pages;
pub mod status_bar;

use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;

use crate::buzzer::Buzzer;
use crate::display::St7567;
use crate::input::InputState;
use crate::rf::afhds2a::TelemetryData;
use crate::storage::RadioStorage;
use crate::trim::TrimController;

pub struct DashboardController {
    pub page: usize,
    pub blink_phase: u8,
    pub vbat_alarm_timer: u32,
    pub rssi_alarm_timer: u32,
    pub min_rssi: u8,
    pub min_rx_v_mv: u16,
    pub telem_seen: bool,
}

impl Default for DashboardController {
    fn default() -> Self {
        Self::new()
    }
}

impl DashboardController {
    pub fn new() -> Self {
        Self {
            page: 0,
            blink_phase: 0,
            vbat_alarm_timer: 7000,
            rssi_alarm_timer: 0,
            min_rssi: 100,
            min_rx_v_mv: 0xFFFF,
            telem_seen: false,
        }
    }

    /// Advance to the next dashboard page.
    pub fn next_page(&mut self, buzzer: &mut Buzzer) {
        self.page = (self.page + 1) % 4;
        buzzer.play_tone(2200, 30);
    }

    /// Return to the previous dashboard page.
    #[allow(dead_code)]
    pub fn prev_page(&mut self, buzzer: &mut Buzzer) {
        self.page = if self.page == 0 { 3 } else { self.page - 1 };
        buzzer.play_tone(2200, 30);
    }

    /// Handle UP/DOWN key presses to cycle dashboard pages (0..3).
    #[allow(dead_code)]
    pub fn handle_keys(&mut self, newly_pressed: u16, buzzer: &mut Buzzer) {
        if newly_pressed & (1 << 9) != 0 {
            // Up
            self.prev_page(buzzer);
        } else if newly_pressed & (1 << 8) != 0 {
            // Down
            self.next_page(buzzer);
        }
    }

    /// Render the current dashboard page onto the LCD.
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        lcd: &mut St7567,
        state: &InputState,
        storage: &RadioStorage,
        trims: &TrimController,
        rf_chs: &[u16; 14],
        rf_ok: bool,
        is_binding: bool,
        telem: &TelemetryData,
        timer_str: Option<&str>,
        timer_expired: bool,
        buzzer: &mut Buzzer,
    ) {
        lcd.clear(BinaryColor::Off).ok();

        status_bar::render(
            lcd,
            storage,
            state.battery_mv,
            rf_ok,
            is_binding,
            telem,
            buzzer,
            self.blink_phase,
            &mut self.vbat_alarm_timer,
            &mut self.rssi_alarm_timer,
            &mut self.min_rssi,
            &mut self.min_rx_v_mv,
            &mut self.telem_seen,
        );

        let blink_on = (self.blink_phase & 0x08) != 0;
        self.blink_phase = self.blink_phase.wrapping_add(1);

        match self.page {
            0 => pages::gimbals::render(
                lcd,
                state,
                storage,
                trims,
                is_binding,
                timer_str,
                timer_expired,
                blink_on,
            ),
            1 => pages::channels::render(lcd, rf_chs, is_binding),
            2 => pages::model::render(lcd, storage, telem, is_binding),
            _ => {
                let is_crsf = storage.active_model().rf_protocol == 1;
                pages::telemetry::render(
                    lcd,
                    is_crsf,
                    telem,
                    state.battery_mv,
                    self.min_rssi,
                    self.min_rx_v_mv,
                    self.telem_seen,
                    is_binding,
                );
            }
        }
    }
}
