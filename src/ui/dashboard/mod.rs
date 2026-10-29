//! Live Flight Dashboard subsystem for FlySky FS-i6X.
//!
//! Provides the primary 4-page flight display:
//! - Page 0 (P1/5): Primary Gimbals, Trims, Switches, Pots
//! - Page 1 (P2/5): 18-Channel Dual-Column Live Monitor
//! - Page 2 (P3/5): 18-Channel Dual-Column Live Monitor cont.
//! - Page 3 (P4/5): Model Dashboard, Type, RX ID, Throttle Curve
//! - Page 4 (P5/5): Telemetry & RF Diagnostics (CRSF Link Diag / AFHDS 2A)

pub mod pages;
pub mod status_bar;

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
        self.page = (self.page + 1) % 5;
        buzzer.play_tone(2200, 30);
    }

    /// Return to the previous dashboard page.
    #[allow(dead_code)]
    pub fn prev_page(&mut self, buzzer: &mut Buzzer) {
        self.page = if self.page == 0 { 4 } else { self.page - 1 };
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
        rf_chs: &[u16; crate::mixer::NUM_CHANNELS],
        rf_ok: bool,
        is_binding: bool,
        telem: &TelemetryData,
        timer_str: Option<&str>,
        timer_expired: bool,
        buzzer: &mut Buzzer,
        now_ms: u32,
    ) {
        lcd.clear_buffer();

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
            1 => pages::channels::render(lcd, rf_chs, is_binding, 0),
            2 => pages::channels::render(lcd, rf_chs, is_binding, 1),
            3 => pages::model::render(lcd, storage, telem, is_binding),
            _ => {
                let is_crsf = storage.active_model().rf_protocol == 1;
                pages::telemetry::render(
                    lcd,
                    storage,
                    is_crsf,
                    telem,
                    state.battery_mv,
                    self.min_rssi,
                    self.min_rx_v_mv,
                    self.telem_seen,
                    is_binding,
                    timer_str,
                    now_ms,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dashboard_5page_cycling() {
        let mut dash = DashboardController::new();
        let mut buzzer = Buzzer::new();

        assert_eq!(dash.page, 0);

        // Next page advances through all 5 pages and wraps
        dash.next_page(&mut buzzer);
        assert_eq!(dash.page, 1);
        dash.next_page(&mut buzzer);
        assert_eq!(dash.page, 2);
        dash.next_page(&mut buzzer);
        assert_eq!(dash.page, 3);
        dash.next_page(&mut buzzer);
        assert_eq!(dash.page, 4);
        dash.next_page(&mut buzzer);
        assert_eq!(dash.page, 0);

        // Prev page wraps backwards from 0 to 4
        dash.prev_page(&mut buzzer);
        assert_eq!(dash.page, 4);
        dash.prev_page(&mut buzzer);
        assert_eq!(dash.page, 3);
        dash.prev_page(&mut buzzer);
        assert_eq!(dash.page, 2);
        dash.prev_page(&mut buzzer);
        assert_eq!(dash.page, 1);
        dash.prev_page(&mut buzzer);
        assert_eq!(dash.page, 0);
    }

    #[test]
    fn test_dashboard_custom_telemetry_render() {
        let mut lcd = St7567::new();
        let mut storage = RadioStorage::default_factory();
        let telem = TelemetryData::default();

        storage.active_model.telem_slots = [4, 5, 6, 11]; // Custom slots: CAPACITY, TX POWER, SNR, TIMER

        pages::telemetry::render(
            &mut lcd,
            &storage,
            false,
            &telem,
            4100,
            0,
            0,
            false,
            false,
            Some("05:32"),
            10000,
        );
    }

    #[test]
    fn test_dashboard_stale_telemetry_indicator() {
        let mut lcd = St7567::new();
        let mut storage = RadioStorage::default_factory();
        // Custom slots: RPM (15), TEMP (16), VSPD (12), ALT (13)
        storage.active_model.telem_slots = [15, 16, 12, 13];

        let mut telem = TelemetryData::default();
        telem.connected = true;
        telem.rpm = 4500;
        telem.last_rpm_ms = 1000;
        telem.temp_c_x10 = 350; // 35.0 C
        telem.last_temp_ms = 1000;
        telem.vspeed_cms = 150;
        telem.last_vspd_ms = 1000;
        telem.alt_m = 120;
        telem.last_alt_ms = 1000;

        // Render at 2000ms: only 1000ms elapsed (< 2500ms) -> Live, no stale indicator
        pages::telemetry::render(
            &mut lcd, &storage, false, &telem, 4000, 100, 4000, true, false, None, 2000,
        );

        // Render at 5000ms: 4000ms elapsed (>= 2500ms) -> Stale, displays last known values with *
        pages::telemetry::render(
            &mut lcd, &storage, false, &telem, 4000, 100, 4000, true, false, None, 5000,
        );
    }
}
