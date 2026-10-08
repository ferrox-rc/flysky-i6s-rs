//! Settings and Diagnostics Menu Subsystem for FlySky FS-i6X.
//!
//! Provides navigation, 20-model memory management, channel reversing,
//! 5/9-point throttle curve editing with real-time spline visualization,
//! configuration editing with Flash persistence, live channel monitoring,
//! raw ADC diagnostics, and native ExpressLRS / CRSF configuration.

pub use crate::ui::format;
pub use crate::ui::widgets;
pub mod screens;

use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;

use crate::adc;
use crate::buzzer::Buzzer;
use crate::display::St7567;
use crate::storage::RadioStorage;
use crate::trim::TrimController;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MenuState {
    Closed,
    MainMenu,
    ModelSelect,
    ModelSetup,
    DualRateExpo,
    ThrottleCurve,
    WingMixer,
    MixerLineEdit,
    AuxChannels,
    ChannelReverse,
    RadioSetup,
    RxSetup,
    ElrsSetup,
    ChannelMonitor,
    DiagAnas,
    SystemInfo,
}

#[derive(Copy, Clone, Debug)]
pub struct NavKeys {
    pub ok: bool,
    pub cancel: bool,
    pub up: bool,
    pub down: bool,
    pub bind: bool,
    pub sw_change: Option<u8>,
    pub dr_change: Option<u8>,
    pub pot_change: Option<u8>,
}

pub struct MenuController {
    pub state: MenuState,
    pub return_state: MenuState,
    pub selected_item: usize,
    pub scroll_offset: usize,
    pub main_menu_selected: usize,
    pub main_menu_scroll: usize,
    pub page_idx: usize,
    pub sub_idx: usize,
    pub edit_val: i32,
    pub editing: bool,
    pub request_calibration: bool,
    pub request_bind: bool,
    prev_keys: u16,
    pub waiting_release: bool,
    up_hold_ms: u16,
    down_hold_ms: u16,
    repeat_timer_ms: u16,
    prev_switches: crate::input::Switches,
    prev_vr1: i16,
    prev_vr2: i16,
}

impl Default for MenuController {
    fn default() -> Self {
        Self::new()
    }
}

impl MenuController {
    pub const fn new() -> Self {
        Self {
            state: MenuState::Closed,
            return_state: MenuState::Closed,
            selected_item: 0,
            scroll_offset: 0,
            main_menu_selected: 0,
            main_menu_scroll: 0,
            page_idx: 0,
            sub_idx: 0,
            edit_val: 0,
            editing: false,
            request_calibration: false,
            request_bind: false,
            prev_keys: 0xFFFF,
            waiting_release: false,
            up_hold_ms: 0,
            down_hold_ms: 0,
            repeat_timer_ms: 0,
            prev_switches: crate::input::Switches::new(),
            prev_vr1: 0,
            prev_vr2: 0,
        }
    }

    /// Open the main settings menu.
    pub fn open(&mut self, buzzer: &mut Buzzer) {
        self.state = MenuState::MainMenu;
        self.selected_item = 0;
        self.scroll_offset = 0;
        self.main_menu_selected = 0;
        self.main_menu_scroll = 0;
        self.page_idx = 0;
        self.sub_idx = 0;
        self.edit_val = 0;
        self.editing = false;
        self.request_calibration = false;
        self.request_bind = false;
        self.waiting_release = true;
        self.prev_keys = 0xFFFF;
        self.up_hold_ms = 0;
        self.down_hold_ms = 0;
        self.repeat_timer_ms = 0;
        buzzer.click();
    }

    /// Return safely to Main Menu, preserving cursor index and scroll offset.
    pub fn return_to_main_menu(&mut self) {
        self.state = MenuState::MainMenu;
        self.selected_item = self.main_menu_selected;
        self.scroll_offset = self.main_menu_scroll;
        self.editing = false;
        self.waiting_release = true;
    }

    /// Returns true if any menu or diagnostic screen is active.
    pub fn is_active(&self) -> bool {
        self.state != MenuState::Closed
    }

    /// Process navigation keys, update menu state, and render display.
    #[inline(never)]
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        lcd: &mut St7567,
        keys: u16,
        switches: &crate::input::Switches,
        pots: &crate::input::Pots,
        storage: &mut RadioStorage,
        trims: &mut TrimController,
        raw_adc: &[u16; adc::NUM_CHANNELS],
        rf_chs: &[u16; crate::mixer::NUM_CHANNELS],
        buzzer: &mut Buzzer,
    ) {
        // Initialize jog wheel tracking positions on first update after menu opened
        if self.prev_keys == 0xFFFF {
            self.prev_vr1 = pots.vr1;
            self.prev_vr2 = pots.vr2;
        }

        // Key release tracking (bit 10: OK, bit 11: Cancel, bit 9: Up, bit 8: Down, bit 12: Bind)
        if self.waiting_release
            && (keys & ((1 << 8) | (1 << 9) | (1 << 10) | (1 << 11) | (1 << 12))) == 0
        {
            self.waiting_release = false;
        }

        let newly_pressed = if self.waiting_release {
            0
        } else {
            keys & !self.prev_keys
        };
        self.prev_keys = keys;

        let raw_down = (keys & (1 << 8)) != 0 && !self.waiting_release;
        let raw_up = (keys & (1 << 9)) != 0 && !self.waiting_release;

        let mut down_pressed = (newly_pressed & (1 << 8)) != 0;
        let mut up_pressed = (newly_pressed & (1 << 9)) != 0;

        // Auto-repeat when UP or DOWN is held (quick traversal of lists, values, and characters)
        if raw_down {
            self.down_hold_ms = self.down_hold_ms.saturating_add(20);
            if self.down_hold_ms >= 300 {
                self.repeat_timer_ms = self.repeat_timer_ms.saturating_add(20);
                if self.repeat_timer_ms >= 70 {
                    down_pressed = true;
                    self.repeat_timer_ms = 0;
                }
            }
        } else {
            self.down_hold_ms = 0;
        }

        if raw_up {
            self.up_hold_ms = self.up_hold_ms.saturating_add(20);
            if self.up_hold_ms >= 300 {
                self.repeat_timer_ms = self.repeat_timer_ms.saturating_add(20);
                if self.repeat_timer_ms >= 70 {
                    up_pressed = true;
                    self.repeat_timer_ms = 0;
                }
            }
        } else {
            self.up_hold_ms = 0;
        }

        if !raw_down && !raw_up {
            self.repeat_timer_ms = 0;
        }

        // Jog Wheels (VRA / VRB) menu list navigation and auto-assignment on change
        let d_vr1 = pots.vr1 - self.prev_vr1;
        let d_vr2 = pots.vr2 - self.prev_vr2;

        let mut pot_change = None;
        if d_vr1.abs() >= 120 {
            pot_change = Some(5); // MixSource::Vra
            self.prev_vr1 = pots.vr1;
        } else if d_vr2.abs() >= 120 {
            pot_change = Some(6); // MixSource::Vrb
            self.prev_vr2 = pots.vr2;
        }

        if !self.editing {
            if d_vr1 >= 60 || d_vr2 >= 60 {
                down_pressed = true;
                if d_vr1 >= 60 {
                    self.prev_vr1 = pots.vr1;
                }
                if d_vr2 >= 60 {
                    self.prev_vr2 = pots.vr2;
                }
            } else if d_vr1 <= -60 || d_vr2 <= -60 {
                up_pressed = true;
                if d_vr1 <= -60 {
                    self.prev_vr1 = pots.vr1;
                }
                if d_vr2 <= -60 {
                    self.prev_vr2 = pots.vr2;
                }
            }
        } else {
            if d_vr1.abs() >= 60 {
                self.prev_vr1 = pots.vr1;
            }
            if d_vr2.abs() >= 60 {
                self.prev_vr2 = pots.vr2;
            }
        }

        let sw_change = switches.detect_condition_change(&self.prev_switches);
        let dr_change = switches.detect_dr_switch_change(&self.prev_switches);
        self.prev_switches = *switches;

        let nav_keys = NavKeys {
            ok: (newly_pressed & (1 << 10)) != 0,
            cancel: (newly_pressed & (1 << 11)) != 0,
            up: up_pressed,
            down: down_pressed,
            bind: (newly_pressed & (1 << 12)) != 0,
            sw_change,
            dr_change,
            pot_change,
        };

        if self.state == MenuState::Closed {
            return;
        }

        lcd.clear(BinaryColor::Off).ok();

        match self.state {
            MenuState::Closed => {}
            MenuState::MainMenu => {
                screens::main_menu::update(self, lcd, &nav_keys, storage, buzzer);
            }
            MenuState::ModelSelect => {
                screens::model::update_select(self, lcd, &nav_keys, storage, trims, buzzer);
            }
            MenuState::ModelSetup => {
                screens::model::update_setup(self, lcd, &nav_keys, storage, buzzer);
            }
            MenuState::DualRateExpo => {
                screens::mixer::update_dual_rate(self, lcd, &nav_keys, storage, buzzer);
            }
            MenuState::ThrottleCurve => {
                screens::mixer::update_throttle_curve(self, lcd, &nav_keys, storage, buzzer);
            }
            MenuState::WingMixer => {
                screens::mixer::update_wing_mixer(self, lcd, &nav_keys, storage, buzzer);
            }
            MenuState::MixerLineEdit => {
                screens::mixer::update_mixer_line_edit(self, lcd, &nav_keys, storage, buzzer);
            }
            MenuState::AuxChannels => {
                screens::channels::update_aux_channels(self, lcd, &nav_keys, storage, buzzer);
            }
            MenuState::ChannelReverse => {
                screens::channels::update_channel_reverse(self, lcd, &nav_keys, storage, buzzer);
            }
            MenuState::RadioSetup => {
                screens::setup::update_radio_setup(self, lcd, &nav_keys, storage, trims, buzzer);
            }
            MenuState::RxSetup => {
                screens::setup::update_rx_setup(self, lcd, &nav_keys, storage, buzzer);
            }
            MenuState::ElrsSetup => {
                screens::elrs::update(self, lcd, &nav_keys, buzzer);
            }
            MenuState::ChannelMonitor => {
                screens::channels::update_channel_monitor(self, lcd, &nav_keys, storage, rf_chs, buzzer);
            }
            MenuState::DiagAnas => {
                screens::diag::update_diag_anas(self, lcd, &nav_keys, raw_adc, buzzer);
            }
            MenuState::SystemInfo => {
                screens::diag::update_system_info(self, lcd, &nav_keys, buzzer);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_menu_return_to_main_menu_preserves_position() {
        let mut ctrl = MenuController::new();
        let mut buzzer = Buzzer::new();

        ctrl.open(&mut buzzer);
        assert_eq!(ctrl.state, MenuState::MainMenu);
        assert_eq!(ctrl.selected_item, 0);
        assert_eq!(ctrl.scroll_offset, 0);

        // Simulate navigating to item 7 ("Radio Setup")
        ctrl.selected_item = 7;
        ctrl.scroll_offset = 5;
        ctrl.main_menu_selected = 7;
        ctrl.main_menu_scroll = 5;

        // Enter sub-menu: indices reset
        ctrl.state = MenuState::RadioSetup;
        ctrl.selected_item = 0;
        ctrl.scroll_offset = 0;
        ctrl.editing = true;

        // Return to main menu
        ctrl.return_to_main_menu();

        assert_eq!(ctrl.state, MenuState::MainMenu);
        assert_eq!(ctrl.selected_item, 7);
        assert_eq!(ctrl.scroll_offset, 5);
        assert!(!ctrl.editing);
        assert!(ctrl.waiting_release);
    }

    #[test]
    fn test_jog_wheel_menu_navigation() {
        let mut ctrl = MenuController::new();
        let mut buzzer = Buzzer::new();
        let mut lcd = St7567::new();
        let mut storage = RadioStorage::default_factory();
        let mut trims = TrimController::new();
        let switches = crate::input::Switches::new();
        let raw_adc = [2048u16; adc::NUM_CHANNELS];
        let rf_chs = [1500u16; crate::mixer::NUM_CHANNELS];

        ctrl.open(&mut buzzer);
        assert_eq!(ctrl.selected_item, 0);

        // First frame initializes prev_vr1 and prev_vr2
        let mut pots = crate::input::Pots { vr1: 0, vr2: 0 };
        ctrl.update(
            &mut lcd,
            0,
            &switches,
            &pots,
            &mut storage,
            &mut trims,
            &raw_adc,
            &rf_chs,
            &mut buzzer,
        );
        assert_eq!(ctrl.selected_item, 0);

        // Rotating VR1 clockwise by +80 counts triggers DOWN
        pots.vr1 = 80;
        ctrl.update(
            &mut lcd,
            0,
            &switches,
            &pots,
            &mut storage,
            &mut trims,
            &raw_adc,
            &rf_chs,
            &mut buzzer,
        );
        assert_eq!(ctrl.selected_item, 1);

        // Rotating VR2 clockwise by +90 counts triggers DOWN again
        pots.vr2 = 90;
        ctrl.update(
            &mut lcd,
            0,
            &switches,
            &pots,
            &mut storage,
            &mut trims,
            &raw_adc,
            &rf_chs,
            &mut buzzer,
        );
        assert_eq!(ctrl.selected_item, 2);

        // Rotating VR1 counter-clockwise by -80 counts triggers UP
        pots.vr1 = 0;
        ctrl.update(
            &mut lcd,
            0,
            &switches,
            &pots,
            &mut storage,
            &mut trims,
            &raw_adc,
            &rf_chs,
            &mut buzzer,
        );
        assert_eq!(ctrl.selected_item, 1);
    }

    #[test]
    fn test_jog_wheel_auto_assignment_in_aux_channels() {
        let mut ctrl = MenuController::new();
        let mut buzzer = Buzzer::new();
        let mut lcd = St7567::new();
        let mut storage = RadioStorage::default_factory();
        let mut trims = TrimController::new();
        let switches = crate::input::Switches::new();
        let raw_adc = [2048u16; adc::NUM_CHANNELS];
        let rf_chs = [1500u16; crate::mixer::NUM_CHANNELS];

        ctrl.state = MenuState::AuxChannels;
        ctrl.selected_item = 0; // CH5
        ctrl.editing = true;

        let mut pots = crate::input::Pots { vr1: 0, vr2: 0 };
        ctrl.update(
            &mut lcd,
            0,
            &switches,
            &pots,
            &mut storage,
            &mut trims,
            &raw_adc,
            &rf_chs,
            &mut buzzer,
        );

        // Rotate VR1 by +150 counts -> auto-assigns MixSource::Vra (5)
        pots.vr1 = 150;
        ctrl.update(
            &mut lcd,
            0,
            &switches,
            &pots,
            &mut storage,
            &mut trims,
            &raw_adc,
            &rf_chs,
            &mut buzzer,
        );
        assert_eq!(storage.models[0].aux_channels[0], 5);

        // Rotate VR2 by +150 counts -> auto-assigns MixSource::Vrb (6)
        pots.vr2 = 150;
        ctrl.update(
            &mut lcd,
            0,
            &switches,
            &pots,
            &mut storage,
            &mut trims,
            &raw_adc,
            &rf_chs,
            &mut buzzer,
        );
        assert_eq!(storage.models[0].aux_channels[0], 6);
    }
}
