//! Reusable UI widgets and layout helpers for FlySky FS-i6X menu displays.

use embedded_graphics::{
    mono_font::{ascii::FONT_4X6, ascii::FONT_6X10, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::Rectangle,
    text::Text,
};

use crate::buzzer::Buzzer;
use crate::display::St7567;

pub const STYLE_TEXT_ON: MonoTextStyle<'static, BinaryColor> = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
pub const STYLE_TEXT_INV: MonoTextStyle<'static, BinaryColor> = MonoTextStyle::new(&FONT_6X10, BinaryColor::Off);
pub const STYLE_SMALL_ON: MonoTextStyle<'static, BinaryColor> = MonoTextStyle::new(&FONT_4X6, BinaryColor::On);
pub const STYLE_SMALL_INV: MonoTextStyle<'static, BinaryColor> = MonoTextStyle::new(&FONT_4X6, BinaryColor::Off);

/// Draw a standardized top header banner with title and underline divider.
pub fn draw_header(lcd: &mut St7567, title: &str) {
    let x = ((128i32 - title.len() as i32 * 6) / 2).max(2);
    Text::new(title, Point::new(x, 9), STYLE_TEXT_ON).draw(lcd).ok();
    lcd.draw_hline(0, 11, 128, true);
}

/// Draw a standardized bottom footer with small font (FONT_4X6) and divider line at y = 55,
/// matching the flight pages' 8-pixel footer height and baseline at y = 62.
pub fn draw_footer(lcd: &mut St7567, text: &str) {
    lcd.draw_hline(0, 55, 128, true);
    Text::new(text, Point::new(2, 62), STYLE_SMALL_ON).draw(lcd).ok();
}

/// Draw a standardized bottom footer with left-aligned and right-aligned text (FONT_4X6)
/// and divider line at y = 55.
pub fn draw_footer_split(lcd: &mut St7567, left: &str, right: &str) {
    lcd.draw_hline(0, 55, 128, true);
    Text::new(left, Point::new(2, 62), STYLE_SMALL_ON).draw(lcd).ok();
    let right_x = (126i32 - right.len() as i32 * 4).max(2);
    Text::new(right, Point::new(right_x, 62), STYLE_SMALL_ON).draw(lcd).ok();
}

/// Draw a standardized bottom footer with left-aligned, center-aligned, and right-aligned text (FONT_4X6)
/// and divider line at y = 55. If center_inverted is true, draws an inverted solid background behind center text.
pub fn draw_footer_three(lcd: &mut St7567, left: &str, center: &str, right: &str, center_inverted: bool) {
    lcd.draw_hline(0, 55, 128, true);
    Text::new(left, Point::new(2, 62), STYLE_SMALL_ON).draw(lcd).ok();
    if !center.is_empty() {
        let center_x = ((128i32 - center.len() as i32 * 4) / 2).max(2);
        if center_inverted {
            lcd.fill_rect(center_x - 2, 56, center.len() as u32 * 4 + 3, 7, true);
            Text::new(center, Point::new(center_x, 62), STYLE_SMALL_INV).draw(lcd).ok();
        } else {
            Text::new(center, Point::new(center_x, 62), STYLE_SMALL_ON).draw(lcd).ok();
        }
    }
    let right_x = (126i32 - right.len() as i32 * 4).max(2);
    Text::new(right, Point::new(right_x, 62), STYLE_SMALL_ON).draw(lcd).ok();
}

/// Draw a horizontal channel gauge (-1000..+1000) with center ticks, trim marker, and a sliding 3px cursor.
pub fn draw_channel_gauge(
    lcd: &mut St7567,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    val: i16,
    trim: i8,
) {
    lcd.draw_rect(x, y, width, height, true);

    let center_x = x + (width as i32 / 2);

    // Center tick line (top and bottom 2 pixels)
    lcd.draw_vline(center_x, y, 2, true);
    lcd.draw_vline(center_x, y + height as i32 - 2, 2, true);

    let min_pos = x + 2;
    let max_pos = x + width as i32 - 3;
    let travel = max_pos - min_pos;
    let cursor_center = min_pos + (((val as i32 + 1000) * travel + 1000) / 2000);

    // If trim is non-zero, draw a 1-pixel trim indicator tick
    if trim != 0 {
        let trim_x = center_x + ((trim as i32 * (travel / 2)) / 25);
        lcd.draw_vline(trim_x, y + 2, height.saturating_sub(4), true);
    }

    lcd.fill_rect(cursor_center - 1, y + 1, 3, height.saturating_sub(2), true);
}

/// Draw a left-to-right throttle progress bar (-1000 is 0%, +1000 is 100%).
pub fn draw_progress_bar(
    lcd: &mut St7567,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    val: i16,
    trim: i8,
) {
    lcd.draw_rect(x, y, width, height, true);

    // Map -1000..+1000 to 0..max_fill
    let max_fill = (width - 2) as i32;
    let normalized = (val as i32 + 1000).clamp(0, 2000);
    let fill_len = ((normalized * max_fill) / 2000) as u32;

    if fill_len > 0 {
        lcd.fill_rect(x + 1, y + 1, fill_len, height.saturating_sub(2), true);
    }

    if trim != 0 {
        // Trim tick: -25..+25 steps maps to ±10% (±100 µs) of full travel (2000 counts)
        let trim_offset = (trim as i32 * max_fill) / 250;
        let trim_x = (x + 1 + trim_offset.max(0)).min(x + max_fill);
        let tick_on = (trim_x - (x + 1)) >= fill_len as i32;
        lcd.draw_vline(trim_x, y + 1, height.saturating_sub(2), tick_on);
    }
}

/// Handle 4-slot circular scrolling list navigation with tone feedback.
pub fn navigate_4slot_list(
    selected: &mut usize,
    scroll_offset: &mut usize,
    count: usize,
    up_pressed: bool,
    down_pressed: bool,
    buzzer: &mut Buzzer,
) {
    if count == 0 {
        return;
    }
    if down_pressed {
        if *selected + 1 < count {
            *selected += 1;
            if *selected >= *scroll_offset + 4 {
                *scroll_offset = *selected - 3;
            }
        } else {
            *selected = 0;
            *scroll_offset = 0;
        }
        buzzer.play_tone(2200, 30);
    }
    if up_pressed {
        if *selected > 0 {
            *selected -= 1;
            if *selected < *scroll_offset {
                *scroll_offset = *selected;
            }
        } else {
            *selected = count - 1;
            *scroll_offset = count.saturating_sub(4);
        }
        buzzer.play_tone(2200, 30);
    }
}

/// Handle 3-slot circular scrolling list navigation with tone feedback.
pub fn navigate_3slot_list(
    selected: &mut usize,
    scroll_offset: &mut usize,
    count: usize,
    up_pressed: bool,
    down_pressed: bool,
    buzzer: &mut Buzzer,
) {
    if count == 0 {
        return;
    }
    if down_pressed {
        if *selected + 1 < count {
            *selected += 1;
            if *selected >= *scroll_offset + 3 {
                *scroll_offset = *selected - 2;
            }
        } else {
            *selected = 0;
            *scroll_offset = 0;
        }
        buzzer.play_tone(2200, 30);
    }
    if up_pressed {
        if *selected > 0 {
            *selected -= 1;
            if *selected < *scroll_offset {
                *scroll_offset = *selected;
            }
        } else {
            *selected = count - 1;
            *scroll_offset = count.saturating_sub(3);
        }
        buzzer.play_tone(2200, 30);
    }
}

/// Draw a vertical scrollbar on the right edge of the screen (x = 125..127).
pub fn draw_scrollbar(
    lcd: &mut St7567,
    selected: usize,
    count: usize,
    top_y: i32,
    height: u32,
) {
    if count <= 1 {
        return;
    }
    // 1px track line on x = 126
    lcd.draw_vline(126, top_y, height, true);

    // Thumb height: proportional or minimum 6px
    let thumb_h = ((height * 3) / count as u32).clamp(6, height);
    let travel = height.saturating_sub(thumb_h);
    let thumb_y = top_y + ((selected as u32 * travel) / (count as u32 - 1)) as i32;

    lcd.fill_rect(125, thumb_y, 3, thumb_h, true);
}

/// Render a single list item row within a 3-slot view (14px row height) with optional 12x12 icon.
pub fn draw_icon_row<F>(
    lcd: &mut St7567,
    slot: usize,
    is_selected: bool,
    draw_icon: Option<F>,
    label: &str,
    value: Option<&str>,
    value_x: i32,
) where
    F: FnOnce(&mut St7567, Point, BinaryColor),
{
    let y = 13 + (slot as i32 * 14);
    let (text_color, icon_color) = if is_selected {
        lcd.fill_rect(2, y, 121, 13, true);
        (BinaryColor::Off, BinaryColor::Off)
    } else {
        (BinaryColor::On, BinaryColor::On)
    };

    let text_style = MonoTextStyle::new(&FONT_6X10, text_color);

    let text_x = if let Some(draw_fn) = draw_icon {
        draw_fn(lcd, Point::new(4, y + 1), icon_color);
        19
    } else {
        4
    };

    Text::new(label, Point::new(text_x, y + 10), text_style).draw(lcd).ok();
    if let Some(val) = value {
        Text::new(val, Point::new(value_x, y + 10), text_style).draw(lcd).ok();
    }
}

/// Render a single list item row within a 4-slot view with highlight.
pub fn draw_list_row(
    lcd: &mut St7567,
    slot: usize,
    is_selected: bool,
    label: &str,
    value: Option<&str>,
    value_x: i32,
) {
    let y = 14 + (slot as i32 * 9);
    let style = if is_selected {
        lcd.fill_rect(2, y, 124, 9, true);
        STYLE_TEXT_INV
    } else {
        STYLE_TEXT_ON
    };

    Text::new(label, Point::new(4, y + 7), style).draw(lcd).ok();
    if let Some(val) = value {
        Text::new(val, Point::new(value_x, y + 7), style).draw(lcd).ok();
    }
}

/// Draw a framed bar gauge meter with an inner filled level.
pub fn draw_bar_gauge(
    lcd: &mut St7567,
    box_rect: Rectangle,
    fill_width: u32,
) {
    lcd.draw_rect(box_rect.top_left.x, box_rect.top_left.y, box_rect.size.width, box_rect.size.height, true);
    if fill_width > 0 {
        let inner_p_x = box_rect.top_left.x + 1;
        let inner_p_y = box_rect.top_left.y + 1;
        let inner_h = box_rect.size.height.saturating_sub(2);
        lcd.fill_rect(inner_p_x, inner_p_y, fill_width, inner_h, true);
    }
}
