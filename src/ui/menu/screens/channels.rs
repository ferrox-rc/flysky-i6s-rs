//! Auxiliary channels assignment, Channel reversing, and Channel monitor screens.

use embedded_graphics::{
    mono_font::{ascii::FONT_4X6, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::Rectangle,
    text::Text,
};

use crate::buzzer::Buzzer;
use crate::display::St7567;
use crate::menu::format::{ascii_as_str, u16_to_dec_4, SOURCE_NAMES};
use crate::menu::widgets;
use crate::menu::{MenuController, NavKeys};
use crate::mixer::{CHANNEL_MAX_US, CHANNEL_MIN_US, CHANNEL_SPAN_US, NUM_CHANNELS};
use crate::storage::{self, RadioStorage};

const CH_NAMES: [&str; 18] = [
    "CH1 (AIL)",
    "CH2 (ELE)",
    "CH3 (THR)",
    "CH4 (RUD)",
    "CH5 (SA) ",
    "CH6 (SB) ",
    "CH7 (VR1)",
    "CH8 (VR2)",
    "CH9 (SC) ",
    "CH10(SD) ",
    "CH11     ",
    "CH12     ",
    "CH13     ",
    "CH14     ",
    "CH15     ",
    "CH16     ",
    "CH17     ",
    "CH18     ",
];

#[inline(never)]
pub fn update_aux_channels(
    ctrl: &mut MenuController,
    lcd: &mut St7567,
    keys: &NavKeys,
    storage: &mut RadioStorage,
    buzzer: &mut Buzzer,
) {
    let active_idx = storage.radio.active_model as usize;
    const AUX_COUNT: usize = 14;

    if keys.cancel {
        if ctrl.editing {
            ctrl.editing = false;
        } else {
            storage::save_active_model(storage);
            ctrl.return_to_main_menu();
            buzzer.click();
            return;
        }
    }

    if !ctrl.editing {
        widgets::navigate_4slot_list(
            &mut ctrl.selected_item,
            &mut ctrl.scroll_offset,
            AUX_COUNT,
            keys.up,
            keys.down,
            buzzer,
        );

        if keys.ok {
            ctrl.editing = true;
            buzzer.click();
        }
    } else {
        let cur = storage.models[active_idx].aux_channels[ctrl.selected_item];
        if keys.up {
            storage.models[active_idx].aux_channels[ctrl.selected_item] = (cur + 1) % 11;
            buzzer.play_tone(2200, 20);
        } else if keys.down {
            storage.models[active_idx].aux_channels[ctrl.selected_item] =
                if cur == 0 { 10 } else { cur - 1 };
            buzzer.play_tone(2200, 20);
        }
        if keys.ok {
            ctrl.editing = false;
            buzzer.click();
        }
    }

    widgets::draw_header(lcd, "AUX CHANNELS");

    for slot in 0..4 {
        let idx = ctrl.scroll_offset + slot;
        if idx >= AUX_COUNT {
            break;
        }
        let is_sel = idx == ctrl.selected_item;

        let ch_num = 5 + idx;
        let mut ch_buf = *b"CH00: ";
        if ch_num >= 10 {
            ch_buf[2] = b'0' + (ch_num / 10) as u8;
            ch_buf[3] = b'0' + (ch_num % 10) as u8;
        } else {
            ch_buf[2] = b'0' + ch_num as u8;
            ch_buf[3] = b' ';
        }
        let ch_label = ascii_as_str(&ch_buf);

        let src_idx = (storage.models[active_idx].aux_channels[idx] as usize).min(10);
        widgets::draw_list_row(lcd, slot, is_sel, ch_label, Some(SOURCE_NAMES[src_idx]), 48);
    }

    let footer = if ctrl.editing {
        "[OK] Done   [UP/DN] Source"
    } else {
        "[OK] Edit   [ESC] Exit"
    };
    widgets::draw_footer(lcd, footer);
}

#[inline(never)]
pub fn update_channel_reverse(
    ctrl: &mut MenuController,
    lcd: &mut St7567,
    keys: &NavKeys,
    storage: &mut RadioStorage,
    buzzer: &mut Buzzer,
) {
    const CH_COUNT: usize = 18;
    let active_idx = storage.radio.active_model as usize;

    if keys.cancel {
        storage::save_active_model(storage);
        ctrl.return_to_main_menu();
        buzzer.click();
        return;
    }

    widgets::navigate_4slot_list(
        &mut ctrl.selected_item,
        &mut ctrl.scroll_offset,
        CH_COUNT,
        keys.up,
        keys.down,
        buzzer,
    );

    if keys.ok {
        storage.models[active_idx].channel_reverse ^= 1 << ctrl.selected_item;
        buzzer.click();
    }

    widgets::draw_header(lcd, "CHANNEL REVERSE");

    for slot in 0..4 {
        let ch = ctrl.scroll_offset + slot;
        if ch >= CH_COUNT {
            break;
        }
        let is_sel = ch == ctrl.selected_item;
        let is_rev = (storage.models[active_idx].channel_reverse & (1 << ch)) != 0;
        let status_str = if is_rev { "REVERSE" } else { "NORMAL " };

        widgets::draw_list_row(lcd, slot, is_sel, CH_NAMES[ch], Some(status_str), 76);
    }

    widgets::draw_footer(lcd, "[OK] Toggle   [ESC] Back");
}

#[inline(never)]
pub fn update_channel_monitor(
    ctrl: &mut MenuController,
    lcd: &mut St7567,
    keys: &NavKeys,
    rf_chs: &[u16; NUM_CHANNELS],
    buzzer: &mut Buzzer,
) {
    if keys.cancel {
        ctrl.return_to_main_menu();
        buzzer.click();
        return;
    }

    if keys.up {
        ctrl.page_idx = (ctrl.page_idx + 1) % 3;
        buzzer.play_tone(2200, 30);
    } else if keys.down {
        ctrl.page_idx = if ctrl.page_idx == 0 {
            2
        } else {
            ctrl.page_idx - 1
        };
        buzzer.play_tone(2200, 30);
    }

    let title = match ctrl.page_idx {
        0 => "CHANNELS (1-6)",
        1 => "CHANNELS (7-12)",
        _ => "CHANNELS (13-18)",
    };
    widgets::draw_header(lcd, title);

    let (start_ch, ch_names): (usize, [&str; 6]) = match ctrl.page_idx {
        0 => (0, ["1:ROL", "2:PIT", "3:THR", "4:YAW", "5:SwA", "6:SwB"]),
        1 => (6, ["7:VR1", "8:VR2", "9:SC ", "10:SD", "11:CH", "12:CH"]),
        _ => (12, ["13:CH", "14:CH", "15:CH", "16:CH", "17:CH", "18:CH"]),
    };

    let text_style_small = MonoTextStyle::new(&FONT_4X6, BinaryColor::On);

    for (i, name) in ch_names.iter().enumerate() {
        let ch = start_ch + i;
        let y = 12 + (i as i32 * 7);
        Text::new(name, Point::new(2, y + 5), text_style_small)
            .draw(lcd)
            .ok();

        let us = rf_chs[ch].clamp(CHANNEL_MIN_US, CHANNEL_MAX_US);
        let fill_w = (((us - CHANNEL_MIN_US) as u32 * 38) / CHANNEL_SPAN_US).min(38);
        widgets::draw_bar_gauge(
            lcd,
            Rectangle::new(Point::new(44, y + 1), Size::new(40, 5)),
            fill_w,
        );

        let mut val_buf = [0u8; 4];
        u16_to_dec_4(us, &mut val_buf);
        let val_str = ascii_as_str(&val_buf);
        Text::new(val_str, Point::new(90, y + 5), text_style_small)
            .draw(lcd)
            .ok();
    }

    widgets::draw_footer(lcd, "[UP/DN] Page  [ESC] Back");
}
