//! Main Settings menu screen.

use crate::buzzer::Buzzer;
use crate::display::St7567;
use crate::menu::widgets;
use crate::menu::{MenuController, MenuState, NavKeys};
use crate::storage::{RadioStorage, NUM_MODELS};

pub fn update(
    ctrl: &mut MenuController,
    lcd: &mut St7567,
    keys: &NavKeys,
    storage: &mut RadioStorage,
    buzzer: &mut Buzzer,
) {
    const ITEM_COUNT: usize = 13;
    let items = [
        "1. Model Select",
        "2. Model Setup",
        "3. Dual Rate/Expo",
        "4. Thr Curve",
        "5. Wing/Mixer",
        "6. Aux Channels",
        "7. Ch Reverse",
        "8. Radio Setup",
        "9. Protocol Setup",
        "10. Channel Monitor",
        "11. Calibration",
        "12. Analog Diag",
        "13. System Info",
    ];

    if keys.cancel {
        ctrl.state = MenuState::Closed;
        buzzer.click();
        return;
    }

    widgets::navigate_4slot_list(
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

    // Render 4 visible items
    for slot in 0..4 {
        let idx = ctrl.scroll_offset + slot;
        if idx >= ITEM_COUNT {
            break;
        }
        let is_selected = idx == ctrl.selected_item;
        widgets::draw_list_row(lcd, slot, is_selected, items[idx], None, 0);
    }

    // Render Footer
    widgets::draw_footer(lcd, "[OK] Select   [ESC] Exit");
}
