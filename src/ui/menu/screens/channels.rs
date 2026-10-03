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

/// Format dynamic channel label into a fixed buffer, e.g. "1:AIL", "2:L.ELV", "6:R.AIL", "10:VR1", "18:None".
pub fn format_channel_label<'a>(
    ch: usize,
    model: &crate::storage::ModelConfig,
    buf: &'a mut [u8; 10],
) -> &'a str {
    let mut len = 0;
    let ch_num = ch + 1;
    if ch_num >= 10 {
        buf[len] = b'0' + (ch_num / 10) as u8;
        len += 1;
        buf[len] = b'0' + (ch_num % 10) as u8;
        len += 1;
    } else {
        buf[len] = b'0' + ch_num as u8;
        len += 1;
    }
    buf[len] = b':';
    len += 1;

    let name = match ch {
        0 => match model.wing_tail_mix {
            1 => "L.ELV",
            3 => "L.AIL",
            _ => "AIL",
        },
        1 => match model.wing_tail_mix {
            1 => "R.ELV",
            2 => "L.VTL",
            _ => "ELE",
        },
        2 => "THR",
        3 => match model.wing_tail_mix {
            2 => "R.VTL",
            _ => "RUD",
        },
        5 if model.wing_tail_mix == 3 => "R.AIL",
        4..=17 => {
            let aux_src = model.aux_channels[ch - 4] as usize;
            if aux_src < SOURCE_NAMES.len() {
                SOURCE_NAMES[aux_src]
            } else {
                "None"
            }
        }
        _ => "CH",
    };

    for &b in name.as_bytes() {
        if len < buf.len() {
            buf[len] = b;
            len += 1;
        }
    }

    ascii_as_str(&buf[..len])
}

#[inline(never)]
pub fn update_channel_monitor(
    ctrl: &mut MenuController,
    lcd: &mut St7567,
    keys: &NavKeys,
    storage: &RadioStorage,
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

    let start_ch: usize = match ctrl.page_idx {
        0 => 0,
        1 => 6,
        _ => 12,
    };

    let text_style_small = MonoTextStyle::new(&FONT_4X6, BinaryColor::On);
    let active_model = storage.active_model();

    for i in 0..6 {
        let ch = start_ch + i;
        let y = 12 + (i as i32 * 7);

        let mut name_buf = [0u8; 10];
        let name = format_channel_label(ch, active_model, &mut name_buf);

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::ModelConfig;

    #[test]
    fn test_format_channel_label_normal_and_aux() {
        let mut model = ModelConfig::default_for_index(0);
        model.wing_tail_mix = 0; // Normal
        model.aux_channels[0] = 7; // CH5 -> SA
        model.aux_channels[1] = 8; // CH6 -> SB
        model.aux_channels[5] = 10; // CH10 -> SD
        model.aux_channels[13] = 0; // CH18 -> None

        let mut buf = [0u8; 10];
        assert_eq!(format_channel_label(0, &model, &mut buf), "1:AIL");
        assert_eq!(format_channel_label(1, &model, &mut buf), "2:ELE");
        assert_eq!(format_channel_label(2, &model, &mut buf), "3:THR");
        assert_eq!(format_channel_label(3, &model, &mut buf), "4:RUD");
        assert_eq!(format_channel_label(4, &model, &mut buf), "5:SA");
        assert_eq!(format_channel_label(5, &model, &mut buf), "6:SB");
        assert_eq!(format_channel_label(9, &model, &mut buf), "10:SD");
        assert_eq!(format_channel_label(17, &model, &mut buf), "18:None");
    }

    #[test]
    fn test_format_channel_label_wing_tail_templates() {
        let mut model = ModelConfig::default_for_index(0);
        let mut buf = [0u8; 10];

        // Elevon
        model.wing_tail_mix = 1;
        assert_eq!(format_channel_label(0, &model, &mut buf), "1:L.ELV");
        assert_eq!(format_channel_label(1, &model, &mut buf), "2:R.ELV");
        assert_eq!(format_channel_label(2, &model, &mut buf), "3:THR");
        assert_eq!(format_channel_label(3, &model, &mut buf), "4:RUD");

        // V-Tail
        model.wing_tail_mix = 2;
        assert_eq!(format_channel_label(0, &model, &mut buf), "1:AIL");
        assert_eq!(format_channel_label(1, &model, &mut buf), "2:L.VTL");
        assert_eq!(format_channel_label(2, &model, &mut buf), "3:THR");
        assert_eq!(format_channel_label(3, &model, &mut buf), "4:R.VTL");

        // Flaperon
        model.wing_tail_mix = 3;
        assert_eq!(format_channel_label(0, &model, &mut buf), "1:L.AIL");
        assert_eq!(format_channel_label(1, &model, &mut buf), "2:ELE");
        assert_eq!(format_channel_label(2, &model, &mut buf), "3:THR");
        assert_eq!(format_channel_label(3, &model, &mut buf), "4:RUD");
        assert_eq!(format_channel_label(5, &model, &mut buf), "6:R.AIL");
    }
}

