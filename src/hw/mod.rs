//! Hardware Abstraction Layer (HAL/PAC) for FlySky FS-i6S.
//!
//! Isolates all FS-i6S board-specific peripherals, pin mappings, power latch,
//! capacitive touchscreen (FT6236), rear buttons, LEDs, and setup rows.

pub mod adc;
pub mod keys;
pub mod power;
pub mod touch;
pub mod uart;

use crate::buzzer::Buzzer;
use crate::display::St7567;
use crate::input::{ InputState, SwitchPos };
use crate::storage::{ AdcInputMode, RadioConfig, RadioStorage };
use crate::trim::TrimController;
use crate::ui::glyphs::draw_switch_arrow;
use crate::ui::menu::{ MenuController, MenuState };
use crate::ui::widgets;

pub const FIRMWARE_ID: &str = "flysky-i6s-rs";
pub const DEFAULT_SB_MODE: AdcInputMode = AdcInputMode::ThreePos;
pub const HAS_JOG_WHEELS: bool = true;

pub const USB_PRODUCT_JOYSTICK: &str = "FS-i6S Joystick";
pub const USB_SERIAL_JOYSTICK: &str = "FS-I6S-SIM";
pub const USB_PRODUCT_SERIAL: &str = "FS-i6S Serial";
pub const USB_SERIAL_SERIAL: &str = "FS-I6S-VCP";
pub const USB_PRODUCT_COMPOSITE: &str = "FS-i6S Radio";
pub const USB_SERIAL_COMPOSITE: &str = "FS-I6S-COMP";

pub const CLI_PROMPT: &[u8] = b"i6s> ";
pub const CLI_PAUSE_PROMPT: &[u8] = b"\r\n[Stream paused]\r\ni6s> ";
pub const STATUS_BANNER: &[u8] = concat!(
    "FlySky FS-i6S Rust Firmware ",
    env!("FIRMWARE_VERSION"),
    " (",
    env!("GIT_HASH"),
    ")\r\n"
)
.as_bytes();
pub const CRSF_UART_STATUS: &[u8] = b"Protocol: CRSF / ExpressLRS (USART1 PB6/PB7 active)\r\n";

pub const AUX_NAMES: [&str; 10] = [
    "SA (PA4):",
    "SB (PA5):",
    "SC (PB0):",
    "SD (PB1):",
    "VRA (PA6):",
    "VRB (PA7):",
    "VRC (PC1):",
    "VRD (PC2):",
    "VRE (PC3):",
    "VRF (PC4):",
];

pub const ADAPTIVE_CH_NAMES: [&str; 10] = ["A", "B", "C", "D", "1", "2", "I", "J", "K", "L"];

pub const AUX_SOURCES: &[u8] = &[
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42,
];

pub const SETUP_ITEMS: usize = 20;
pub const INPUT_SETUP_RETURN: (usize, usize) = (15, 12);
pub const TELEM_SETUP_RETURN: (usize, usize) = (17, 14);
pub const PPM_OUT_SETUP_IDX: usize = 18;

/// Early board initialization (power latch on FS-i6S).
#[inline]
pub fn pre_init() {
    power::init();
}

/// Initialize capacitive touchscreen controller on FS-i6S.
#[inline]
pub fn init_touch() {
    touch::ft6236::init();
}

/// Initialize board-specific switch/button pins after loading config.
#[inline]
pub fn init_board_keys(radio: &RadioConfig) {
    keys::init_j4_keys(radio.j4_sg_sh_en != 0);
}

/// Poll navigation keys (combines physical buttons and FT6236 touch gestures on FS-i6S).
#[inline]
pub fn poll_keys(storage: &RadioStorage, menu_active: bool) -> u16 {
    let touch_sample = if storage.radio.touch_disabled == 0 && touch::ft6236::is_touch_int_asserted() {
        touch::ft6236::read_touch()
    } else {
        None
    };
    let current_sticks = crate::input::read().gimbals;
    touch::nav::update_inputs(
        touch_sample.as_ref(),
        &current_sticks,
        menu_active,
        storage.radio.rear_left_func,
        storage.radio.rear_right_func,
        storage.radio.throttle_trim != 0,
    )
}

/// Update dynamic flight switches (rear buttons SWE/SWF and J4 SG/SH on FS-i6S).
#[inline]
pub fn update_flight_switches(
    state: &mut InputState,
    storage: &RadioStorage,
    menu_active: bool,
) {
    let (left_btn, right_btn) = keys::read_front_buttons();
    if !menu_active && storage.radio.rear_left_func == 0 && left_btn {
        state.switches.se = SwitchPos::Down;
    } else {
        state.switches.se = SwitchPos::Up;
    }
    if !menu_active && storage.radio.rear_right_func == 0 && right_btn {
        state.switches.sf = SwitchPos::Down;
    } else {
        state.switches.sf = SwitchPos::Up;
    }

    if storage.radio.j4_sg_sh_en != 0 {
        let (sg_pressed, sh_pressed) = keys::read_j4_buttons();
        state.switches.sg = if sg_pressed { SwitchPos::Down } else { SwitchPos::Up };
        state.switches.sh = if sh_pressed { SwitchPos::Down } else { SwitchPos::Up };
    } else {
        state.switches.sg = SwitchPos::Up;
        state.switches.sh = SwitchPos::Up;
    }
}

/// Calculate battery voltage in millivolts using the FlySky FS-i6S 10k/5.1k resistor divider:
/// Vbat = Vadc * ((10000 + 5100) / 5100) = Vadc * 2.9608
/// Vadc = (raw * 3300) / 4095
#[inline]
pub fn calculate_battery_mv(raw: u16) -> u16 {
    let vbat_mv = (raw as u32 * 977064) / 409500;
    vbat_mv as u16
}

/// Decode board-specific switches (SE..SWL) from raw ADC and GPIO states.
#[inline]
pub fn decode_board_switches(
    raw: &[u16; adc::NUM_CHANNELS],
    _ext_switches: bool,
    ext_adc: bool,
    adc_modes: &[u8; 10],
) -> (
    SwitchPos,
    SwitchPos,
    SwitchPos,
    SwitchPos,
    SwitchPos,
    SwitchPos,
    SwitchPos,
    SwitchPos,
) {
    let (swi, swj, swk, swl) = if ext_adc {
        let swi = match AdcInputMode::resolve(6, adc_modes[6]) {
            AdcInputMode::ThreePos => crate::input::decode_switch_3pos(raw[11]),
            AdcInputMode::TwoPos => crate::input::decode_switch_2pos(raw[11]),
            _ => SwitchPos::Up,
        };
        let swj = match AdcInputMode::resolve(7, adc_modes[7]) {
            AdcInputMode::ThreePos => crate::input::decode_switch_3pos(raw[12]),
            AdcInputMode::TwoPos => crate::input::decode_switch_2pos(raw[12]),
            _ => SwitchPos::Up,
        };
        let swk = match AdcInputMode::resolve(8, adc_modes[8]) {
            AdcInputMode::ThreePos => crate::input::decode_switch_3pos(raw[13]),
            AdcInputMode::TwoPos => crate::input::decode_switch_2pos(raw[13]),
            _ => SwitchPos::Up,
        };
        let swl = match AdcInputMode::resolve(9, adc_modes[9]) {
            AdcInputMode::ThreePos => crate::input::decode_switch_3pos(raw[14]),
            AdcInputMode::TwoPos => crate::input::decode_switch_2pos(raw[14]),
            _ => SwitchPos::Up,
        };
        (swi, swj, swk, swl)
    } else {
        (SwitchPos::Up, SwitchPos::Up, SwitchPos::Up, SwitchPos::Up)
    };

    (
        SwitchPos::Up,
        SwitchPos::Up,
        SwitchPos::Up,
        SwitchPos::Up,
        swi,
        swj,
        swk,
        swl,
    )
}

/// Number of visible items in Input Setup menu.
#[inline]
pub fn input_setup_items(storage: &RadioStorage) -> usize {
    if storage.radio.ext_adc != 0 {
        10
    } else {
        6
    }
}

/// Handle OK key press for board-specific Radio Setup items (indices >= 8).
pub fn handle_board_setup_ok(
    idx: usize,
    ctrl: &mut MenuController,
    storage: &mut RadioStorage,
    _trims: &mut TrimController,
    buzzer: &mut Buzzer,
) {
    match idx {
        8 => {
            buzzer.click();
            storage.radio.ext_module_pwr = if storage.radio.ext_module_pwr == 0 { 1 } else { 0 };
            crate::crsf::set_power_polarity(storage.radio.ext_module_pwr == 0);
            crate::storage::save_radio_config(storage);
        }
        9 => {
            buzzer.click();
            storage.radio.j4_sg_sh_en = if storage.radio.j4_sg_sh_en == 0 { 1 } else { 0 };
            keys::init_j4_keys(storage.radio.j4_sg_sh_en != 0);
            crate::storage::save_radio_config(storage);
        }
        10 => {
            buzzer.click();
            storage.radio.rear_left_func = (storage.radio.rear_left_func + 1) % 3;
            crate::storage::save_radio_config(storage);
        }
        11 => {
            buzzer.click();
            storage.radio.rear_right_func = (storage.radio.rear_right_func + 1) % 3;
            crate::storage::save_radio_config(storage);
        }
        12 => {
            buzzer.click();
            storage.radio.touch_disabled = if storage.radio.touch_disabled == 0 { 1 } else { 0 };
            crate::storage::save_radio_config(storage);
        }
        13 => {
            buzzer.click();
            storage.radio.stick_mode = (storage.radio.stick_mode + 1) % 4;
            crate::storage::save_radio_config(storage);
        }
        14 => {
            buzzer.click();
            storage.radio.switch_beep = if storage.radio.switch_beep == 0 { 1 } else { 0 };
            crate::storage::save_radio_config(storage);
        }
        15 => {
            buzzer.click();
            ctrl.state = MenuState::InputSetup;
            ctrl.selected_item = 0;
            ctrl.scroll_offset = 0;
        }
        16 => {
            buzzer.click();
            storage.radio.ext_adc = if storage.radio.ext_adc == 0 { 1 } else { 0 };
            adc::set_ext_adc_enabled(storage.radio.ext_adc != 0);
            crate::input::set_ext_adc_enabled(storage.radio.ext_adc != 0);
            crate::input::apply_calibration(&storage.radio);
            crate::storage::save_radio_config(storage);
        }
        17 => {
            buzzer.click();
            ctrl.state = MenuState::TelemSetup;
            ctrl.selected_item = 0;
            ctrl.scroll_offset = 0;
        }
        18 => {
            buzzer.click();
            let en = !storage.radio.is_ppm_out_enabled();
            storage.radio.set_ppm_out_enabled(en);
            crate::trainer::set_ppm_out(en);
            crate::storage::save_radio_config(storage);
        }
        19 => {
            buzzer.click();
            let en = !storage.radio.is_ppm_in_enabled();
            storage.radio.set_ppm_in_enabled(en);
            crate::trainer::set_ppm_in(en);
            crate::storage::save_radio_config(storage);
        }
        _ => {}
    }
}

/// Render board-specific Radio Setup rows (indices >= 8).
pub fn draw_board_setup_row(
    lcd: &mut St7567,
    slot: usize,
    idx: usize,
    is_sel: bool,
    storage: &RadioStorage,
) {
    match idx {
        8 => {
            let pwr_str = if storage.radio.ext_module_pwr == 0 {
                "HIGH (N)"
            } else {
                "LOW (P)"
            };
            widgets::draw_list_row_right(lcd, slot, is_sel, "H1 PF6 Pwr:", Some(pwr_str));
        }
        9 => {
            let j4_str = if storage.radio.j4_sg_sh_en != 0 {
                "SG/SH BTN"
            } else {
                "SWD (DBG)"
            };
            widgets::draw_list_row_right(lcd, slot, is_sel, "J4 Mode:", Some(j4_str));
        }
        10 => {
            let left_str = match storage.radio.rear_left_func {
                1 => "TIMER RESET",
                2 => "TRIMS MOD",
                _ => "RC SW (SWE)",
            };
            widgets::draw_list_row_right(lcd, slot, is_sel, "Rear L:", Some(left_str));
        }
        11 => {
            let right_str = match storage.radio.rear_right_func {
                1 => "INSTANT TRIM",
                2 => "TRIMS MOD",
                _ => "RC SW (SWF)",
            };
            widgets::draw_list_row_right(lcd, slot, is_sel, "Rear R:", Some(right_str));
        }
        12 => {
            let touch_str = if storage.radio.touch_disabled == 0 {
                "ENABLED"
            } else {
                "DISABLED"
            };
            widgets::draw_list_row_right(lcd, slot, is_sel, "Touch:", Some(touch_str));
        }
        13 => {
            let mode_str = match storage.radio.stick_mode {
                0 => "MODE 1",
                1 => "MODE 2",
                2 => "MODE 3",
                3 => "MODE 4",
                _ => "MODE 2",
            };
            widgets::draw_list_row_right(lcd, slot, is_sel, "Stick Mode:", Some(mode_str));
        }
        14 => {
            let sw_snd_str = if storage.radio.switch_beep != 0 {
                "ENABLED"
            } else {
                "MUTED"
            };
            widgets::draw_list_row_right(lcd, slot, is_sel, "Sw Sound:", Some(sw_snd_str));
        }
        15 => {
            widgets::draw_list_row_right(lcd, slot, is_sel, "Inputs:", Some("[SETUP]"));
        }
        16 => {
            let ext_str = if storage.radio.ext_adc != 0 {
                "ENABLED"
            } else {
                "DISABLED"
            };
            widgets::draw_list_row_right(lcd, slot, is_sel, "Ext ADC (PC):", Some(ext_str));
        }
        17 => {
            widgets::draw_list_row_right(lcd, slot, is_sel, "Telem Page:", Some("[SETUP]"));
        }
        18 => {
            let ppm_str = if storage.radio.is_ppm_out_enabled() {
                "ENABLED"
            } else {
                "OFF"
            };
            widgets::draw_list_row_right(lcd, slot, is_sel, "PPM Out:", Some(ppm_str));
        }
        19 => {
            let in_str = if storage.radio.is_ppm_in_enabled() {
                "ENABLED"
            } else {
                "OFF"
            };
            widgets::draw_list_row_right(lcd, slot, is_sel, "Trainer In:", Some(in_str));
        }
        _ => {}
    }
}

/// Render the default switch and dual pot layout (y = 46..53) on FS-i6S.
pub fn render_default_layout(lcd: &mut St7567, state: &InputState, storage: &RadioStorage) {
    let sg_sh_active = storage.radio.j4_sg_sh_en != 0;

    if sg_sh_active {
        let sw_list = [
            ("A", state.switches.sa),
            ("B", state.switches.sb),
            ("C", state.switches.sc),
            ("D", state.switches.sd),
            ("E", state.switches.se),
            ("F", state.switches.sf),
            ("G", state.switches.sg),
            ("H", state.switches.sh),
        ];
        let mut x = 2;
        for (lbl, pos) in sw_list.iter() {
            lcd.draw_str_4x6(x, 47, lbl, false);
            draw_switch_arrow(lcd, x + 5, 46, *pos);
            x += 12;
        }

        widgets::draw_split_pot_bar(lcd, 100, 46, 26, state.pots.vr1, state.pots.vr2);
    } else {
        let sw_list = [
            ("A", state.switches.sa),
            ("B", state.switches.sb),
            ("C", state.switches.sc),
            ("D", state.switches.sd),
            ("E", state.switches.se),
            ("F", state.switches.sf),
        ];
        let mut x = 2;
        for (lbl, pos) in sw_list.iter() {
            lcd.draw_str_4x6(x, 47, lbl, false);
            draw_switch_arrow(lcd, x + 6, 46, *pos);
            x += 14;
        }

        lcd.draw_str_4x6(86, 47, "VR", false);
        widgets::draw_split_pot_bar(lcd, 96, 46, 30, state.pots.vr1, state.pots.vr2);
    }
}

/// Append board-specific digital switches (E, F, G, H) to the adaptive dashboard layout list.
pub fn append_extra_switches(
    state: &InputState,
    storage: &RadioStorage,
    sw_list: &mut [(&'static str, SwitchPos, bool, u8); 16],
    sw_count: &mut usize,
) {
    if *sw_count < sw_list.len() {
        sw_list[*sw_count] = ("E", state.switches.se, false, 0);
        *sw_count += 1;
    }
    if *sw_count < sw_list.len() {
        sw_list[*sw_count] = ("F", state.switches.sf, false, 0);
        *sw_count += 1;
    }
    if storage.radio.j4_sg_sh_en != 0 {
        if *sw_count < sw_list.len() {
            sw_list[*sw_count] = ("G", state.switches.sg, false, 0);
            *sw_count += 1;
        }
        if *sw_count < sw_list.len() {
            sw_list[*sw_count] = ("H", state.switches.sh, false, 0);
            *sw_count += 1;
        }
    }
}

/// Title for Analog Diagnostics pages 2 and 3.
pub fn diag_page_title(page_idx: usize) -> &'static str {
    match page_idx {
        0 => "ANALOG (1-6)",
        1 => "ANALOG (7-11)",
        2 => "SWITCHES (E/F/G/H)",
        _ => "EXT ANALOG (PC1-4)",
    }
}

/// Render Analog Diagnostics pages 2 and 3 on FS-i6S.
pub fn draw_diag_ext_page(
    lcd: &mut St7567,
    page_idx: usize,
    raw_adc: &[u16; adc::NUM_CHANNELS],
    switches: &crate::input::Switches,
) {
    use crate::ui::format::{ ascii_as_str, u16_to_dec_4 };

    if page_idx == 2 {
        let items = [
            ("SW:SE", switches.se),
            ("SW:SF", switches.sf),
            ("SW:SG", switches.sg),
            ("SW:SH", switches.sh),
        ];
        for (i, &(label, pos)) in items.iter().enumerate() {
            let y = 13 + (i as i32 * 9);
            lcd.draw_str_4x6(2, y, label, false);
            let state_str = if pos == SwitchPos::Down {
                "DN (LOW)"
            } else {
                "UP (HIGH)"
            };
            let fill = if pos == SwitchPos::Down { 30 } else { 0 };
            widgets::draw_bar_gauge(lcd, 32, y, 30, 6, fill);
            lcd.draw_str_4x6(66, y, state_str, false);
        }
    } else {
        let ext_active = crate::input::is_ext_adc_enabled();
        let cfg_label = if ext_active {
            "PC ADC: ENABLED (15-CH)"
        } else {
            "PC ADC: DISABLED (OFF)"
        };
        lcd.draw_str_4x6(2, 13, cfg_label, false);

        let names: [&str; 4] = ["VRC:PC1 ", "VRD:PC2 ", "VRE:PC3 ", "VRF:PC4 "];
        for (i, &name) in names.iter().enumerate() {
            let adc_idx = 11 + i;
            let y = 25 + (i as i32 * 7);
            lcd.draw_str_4x6(2, y, name, false);

            let raw = raw_adc[adc_idx].min(4095);
            let fill_w = if ext_active {
                ((raw as u32 * 38) / 4095).min(38)
            } else {
                0
            };
            widgets::draw_bar_gauge(lcd, 44, y + 1, 40, 5, fill_w);

            let mut val_buf = [0u8; 4];
            if ext_active {
                u16_to_dec_4(raw, &mut val_buf);
            } else {
                val_buf = *b" OFF";
            }
            let val_str = ascii_as_str(&val_buf);
            lcd.draw_str_4x6(90, y, val_str, false);
        }
    }
}

/// Board-specific background idle state for FS-i6S (PowerManager, rear button edges, touch hold, LEDs).
pub struct BoardIdleState {
    pub power_manager: power::PowerManager,
    prev_left_btn: bool,
    prev_right_btn: bool,
    latched_keys: u16,
    last_center_touch_ms: u32,
    menu_led_phase: bool,
    menu_led_timer_ms: u16,
}

impl Default for BoardIdleState {
    fn default() -> Self {
        Self::new()
    }
}

impl BoardIdleState {
    pub const fn new() -> Self {
        Self {
            power_manager: power::PowerManager::new(),
            prev_left_btn: false,
            prev_right_btn: false,
            latched_keys: 0,
            last_center_touch_ms: 0,
            menu_led_phase: true,
            menu_led_timer_ms: 0,
        }
    }

    /// Update board-specific background tasks and return `(ok_held_for_menu, timer_reset_clicked, instant_trim_clicked)`.
    pub fn update(
        &mut self,
        now: u32,
        dt_ms: u16,
        keys: u16,
        menu_active: bool,
        storage: &mut RadioStorage,
        buzzer: &mut Buzzer,
    ) -> (bool, bool, bool) {
        self.power_manager.update(dt_ms, storage, buzzer);
        self.latched_keys |= keys;

        // Front Button LED Backlight Handling
        if menu_active {
            self.menu_led_timer_ms = self.menu_led_timer_ms.saturating_add(dt_ms);
            if self.menu_led_timer_ms >= 250 {
                self.menu_led_timer_ms = 0;
                self.menu_led_phase = !self.menu_led_phase;
                power::set_leds(self.menu_led_phase);
            }
        } else if !self.menu_led_phase {
            self.menu_led_phase = true;
            self.menu_led_timer_ms = 0;
            power::set_leds(true);
        }

        let (left_btn, right_btn) = keys::read_front_buttons();
        let left_clicked = left_btn && !self.prev_left_btn;
        let right_clicked = right_btn && !self.prev_right_btn;
        self.prev_left_btn = left_btn;
        self.prev_right_btn = right_btn;

        let ok_held = if !menu_active {
            let center_touch = (keys & touch::nav::KEY_MENU_OPEN) != 0;
            if center_touch {
                self.last_center_touch_ms = now;
            }
            let touch_active = self.last_center_touch_ms != 0
                && now.wrapping_sub(self.last_center_touch_ms) < 150;
            touch_active || right_btn
        } else {
            self.last_center_touch_ms = 0;
            false
        };

        let timer_reset_clicked = !menu_active && left_clicked && storage.radio.rear_left_func == 1;
        let instant_trim_clicked =
            !menu_active && right_clicked && storage.radio.rear_right_func == 1;

        (ok_held, timer_reset_clicked, instant_trim_clicked)
    }

    pub fn clear_menu_open_state(&mut self) {
        self.last_center_touch_ms = 0;
    }

    pub fn take_frame_keys(&mut self, _current_keys: u16, menu_or_calib_active: bool) -> u16 {
        let mut frame_keys = self.latched_keys;
        self.latched_keys = 0;
        if menu_or_calib_active && (frame_keys & ((1 << 8) | (1 << 9))) != 0 {
            frame_keys &= !(1 << 10);
        }
        frame_keys
    }

    pub fn render_overlay(&self, lcd: &mut St7567) {
        let progress_pct = self.power_manager.shutdown_progress_pct();
        if progress_pct > 0 {
            lcd.fill_rect(18, 19, 92, 26, false);
            lcd.draw_rect(18, 19, 92, 26, true);
            lcd.draw_str_6x10(28, 21, "SHUTDOWN...", false);
            lcd.draw_rect(27, 33, 74, 6, true);
            let fill_w = ((progress_pct as u32 * 70) / 100).min(70);
            if fill_w > 0 {
                lcd.fill_rect(29, 35, fill_w, 2, true);
            }
        }
    }
}
