use crate::buzzer::Buzzer;
use crate::display::St7567;
use crate::menu::widgets;
use crate::menu::{MenuController, MenuState, NavKeys};
use crate::storage::{RadioStorage, NUM_MODELS};
use crate::ui::glyphs::{draw_glyph_12x12, MENU_GLYPHS};

fn draw_menu_icon(lcd: &mut St7567, idx: usize, x: i32, y: i32, on: bool) {
    if idx < MENU_GLYPHS.len() {
        draw_glyph_12x12(lcd, x, y, MENU_GLYPHS[idx], on);
    }
}

#[inline(never)]
pub fn update(
    ctrl: &mut MenuController,
    lcd: &mut St7567,
    keys: &NavKeys,
    storage: &mut RadioStorage,
    buzzer: &mut Buzzer,
) {
    const ITEM_COUNT: usize = 13;
    let items = [
        "Model Select",
        "Model Setup",
        "Dual Rate/Expo",
        "Throttle Curve",
        "Wing / Mixer",
        "Aux Channels",
        "Channel Reverse",
        "Radio Setup",
        "Protocol Setup",
        "Channel Monitor",
        "Calibration",
        "Analog Diag",
        "System Info",
    ];

    if keys.cancel {
        ctrl.state = MenuState::Closed;
        buzzer.click();
        return;
    }

    widgets::navigate_3slot_list(
        &mut ctrl.selected_item,
        &mut ctrl.scroll_offset,
        ITEM_COUNT,
        keys.up,
        keys.down,
        buzzer,
    );

    if keys.ok {
        buzzer.click();
        ctrl.waiting_release = true;
        ctrl.main_menu_selected = ctrl.selected_item;
        ctrl.main_menu_scroll = ctrl.scroll_offset;
        match ctrl.selected_item {
            0 => {
                ctrl.state = MenuState::ModelSelect;
                ctrl.selected_item = storage.radio.active_model as usize;
                ctrl.scroll_offset = ctrl.selected_item.saturating_sub(2).min(NUM_MODELS.saturating_sub(4));
            }
            1 => {
                ctrl.state = MenuState::ModelSetup;
                ctrl.selected_item = 0;
                ctrl.scroll_offset = 0;
                ctrl.sub_idx = 0;
            }
            2 => {
                ctrl.state = MenuState::DualRateExpo;
                ctrl.selected_item = 0;
                ctrl.scroll_offset = 0;
                ctrl.sub_idx = 0;
            }
            3 => {
                ctrl.state = MenuState::ThrottleCurve;
                ctrl.selected_item = 0;
                ctrl.scroll_offset = 0;
            }
            4 => {
                ctrl.state = MenuState::WingMixer;
                ctrl.selected_item = 0;
                ctrl.scroll_offset = 0;
            }
            5 => {
                ctrl.state = MenuState::AuxChannels;
                ctrl.selected_item = 0;
                ctrl.scroll_offset = 0;
            }
            6 => {
                ctrl.state = MenuState::ChannelReverse;
                ctrl.selected_item = 0;
                ctrl.scroll_offset = 0;
            }
            7 => {
                ctrl.state = MenuState::RadioSetup;
                ctrl.selected_item = 0;
                ctrl.scroll_offset = 0;
            }
            8 => {
                ctrl.state = MenuState::RxSetup;
                ctrl.selected_item = 0;
                ctrl.scroll_offset = 0;
            }
            9 => {
                ctrl.state = MenuState::ChannelMonitor;
                ctrl.page_idx = 0;
            }
            10 => {
                ctrl.request_calibration = true;
                ctrl.state = MenuState::Closed;
                return;
            }
            11 => {
                ctrl.state = MenuState::DiagAnas;
                ctrl.selected_item = 0;
                ctrl.scroll_offset = 0;
            }
            12 => {
                ctrl.state = MenuState::SystemInfo;
                ctrl.selected_item = 0;
                ctrl.scroll_offset = 0;
            }
            _ => {}
        }
        return;
    }

    // Render Header
    widgets::draw_header(lcd, "SETTINGS MENU");

    // Render 3 visible items
    for slot in 0..3 {
        let idx = ctrl.scroll_offset + slot;
        if idx >= ITEM_COUNT {
            break;
        }
        let is_selected = idx == ctrl.selected_item;
        widgets::draw_icon_row(
            lcd,
            slot,
            is_selected,
            Some(|lcd: &mut St7567, x, y, on| draw_menu_icon(lcd, idx, x, y, on)),
            items[idx],
            None,
            0,
        );
    }

    // Render Scrollbar on right edge
    widgets::draw_scrollbar(lcd, ctrl.selected_item, ITEM_COUNT, 13, 40);

    // Format footer with position indicator (e.g. "[OK] Select   1/13")
    let mut pos_buf = [0u8; 6];
    let pos_str = {
        let cur = ctrl.selected_item + 1;
        let mut i = 0;
        if cur >= 10 {
            pos_buf[i] = b'0' + (cur / 10) as u8;
            i += 1;
        }
        pos_buf[i] = b'0' + (cur % 10) as u8;
        i += 1;
        pos_buf[i] = b'/';
        i += 1;
        pos_buf[i] = b'1';
        i += 1;
        pos_buf[i] = b'3';
        i += 1;
        core::str::from_utf8(&pos_buf[..i]).unwrap_or("")
    };

    widgets::draw_footer_split(lcd, "[OK] Select   [ESC] Exit", pos_str);
}
