//! Reusable UI widgets and layout helpers for FlySky FS-i6X menu displays.

use embedded_graphics::{
    mono_font::{ascii::FONT_4X6, ascii::FONT_6X10, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{Line, PrimitiveStyle, Rectangle},
    text::Text,
};

use crate::buzzer::Buzzer;
use crate::display::St7567;

/// Draw a standardized top header banner with title and underline divider.
pub fn draw_header(lcd: &mut St7567, title: &str) {
    let text_style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
    let border_style = PrimitiveStyle::with_stroke(BinaryColor::On, 1);
    let x = ((128i32 - title.len() as i32 * 6) / 2).max(2);
    Text::new(title, Point::new(x, 9), text_style).draw(lcd).ok();
    Line::new(Point::new(0, 11), Point::new(127, 11)).into_styled(border_style).draw(lcd).ok();
}

/// Draw a standardized bottom footer with small font (FONT_4X6) and divider line at y = 55,
/// matching the flight pages' 8-pixel footer height and baseline at y = 62.
pub fn draw_footer(lcd: &mut St7567, text: &str) {
    let text_style = MonoTextStyle::new(&FONT_4X6, BinaryColor::On);
    let border_style = PrimitiveStyle::with_stroke(BinaryColor::On, 1);
    Line::new(Point::new(0, 55), Point::new(127, 55)).into_styled(border_style).draw(lcd).ok();
    Text::new(text, Point::new(2, 62), text_style).draw(lcd).ok();
}

/// Draw a standardized bottom footer with left-aligned and right-aligned text (FONT_4X6)
/// and divider line at y = 55.
pub fn draw_footer_split(lcd: &mut St7567, left: &str, right: &str) {
    let text_style = MonoTextStyle::new(&FONT_4X6, BinaryColor::On);
    let border_style = PrimitiveStyle::with_stroke(BinaryColor::On, 1);
    Line::new(Point::new(0, 55), Point::new(127, 55)).into_styled(border_style).draw(lcd).ok();
    Text::new(left, Point::new(2, 62), text_style).draw(lcd).ok();
    let right_x = (126i32 - right.len() as i32 * 4).max(2);
    Text::new(right, Point::new(right_x, 62), text_style).draw(lcd).ok();
}

/// Draw a standardized bottom footer with left-aligned, center-aligned, and right-aligned text (FONT_4X6)
/// and divider line at y = 55. If center_inverted is true, draws an inverted solid background behind center text.
pub fn draw_footer_three(lcd: &mut St7567, left: &str, center: &str, right: &str, center_inverted: bool) {
    let text_style = MonoTextStyle::new(&FONT_4X6, BinaryColor::On);
    let border_style = PrimitiveStyle::with_stroke(BinaryColor::On, 1);
    Line::new(Point::new(0, 55), Point::new(127, 55)).into_styled(border_style).draw(lcd).ok();
    Text::new(left, Point::new(2, 62), text_style).draw(lcd).ok();
    if !center.is_empty() {
        let center_x = ((128i32 - center.len() as i32 * 4) / 2).max(2);
        if center_inverted {
            let fill_style = PrimitiveStyle::with_fill(BinaryColor::On);
            Rectangle::new(Point::new(center_x - 2, 56), Size::new(center.len() as u32 * 4 + 3, 7))
                .into_styled(fill_style)
                .draw(lcd)
                .ok();
            let inv_style = MonoTextStyle::new(&FONT_4X6, BinaryColor::Off);
            Text::new(center, Point::new(center_x, 62), inv_style).draw(lcd).ok();
        } else {
            Text::new(center, Point::new(center_x, 62), text_style).draw(lcd).ok();
        }
    }
    let right_x = (126i32 - right.len() as i32 * 4).max(2);
    Text::new(right, Point::new(right_x, 62), text_style).draw(lcd).ok();
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
    let border_style = PrimitiveStyle::with_stroke(BinaryColor::On, 1);
    let fill_style = PrimitiveStyle::with_fill(BinaryColor::On);

    // Frame
    Rectangle::new(Point::new(x, y), Size::new(width, height))
        .into_styled(border_style)
        .draw(lcd)
        .ok();

    let center_x = x + (width as i32 / 2);

    // Center tick line (top and bottom 2 pixels)
    Line::new(Point::new(center_x, y), Point::new(center_x, y + 1))
        .into_styled(border_style)
        .draw(lcd)
        .ok();
    Line::new(
        Point::new(center_x, y + height as i32 - 2),
        Point::new(center_x, y + height as i32 - 1),
    )
    .into_styled(border_style)
    .draw(lcd)
    .ok();

    let min_pos = x + 2;
    let max_pos = x + width as i32 - 3;
    let travel = max_pos - min_pos;
    let cursor_center = min_pos + (((val as i32 + 1000) * travel + 1000) / 2000);

    // If trim is non-zero, draw a 1-pixel trim indicator tick
    if trim != 0 {
        let trim_x = center_x + ((trim as i32 * (travel / 2)) / 25);
        Line::new(Point::new(trim_x, y + 2), Point::new(trim_x, y + height as i32 - 3))
            .into_styled(border_style)
            .draw(lcd)
            .ok();
    }

    Rectangle::new(
        Point::new(cursor_center - 1, y + 1),
        Size::new(3, height - 2),
    )
    .into_styled(fill_style)
    .draw(lcd)
    .ok();
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
    let border_style = PrimitiveStyle::with_stroke(BinaryColor::On, 1);
    let fill_style = PrimitiveStyle::with_fill(BinaryColor::On);

    // Frame
    Rectangle::new(Point::new(x, y), Size::new(width, height))
        .into_styled(border_style)
        .draw(lcd)
        .ok();

    // Map -1000..+1000 to 0..max_fill
    let max_fill = (width - 2) as i32;
    let normalized = (val as i32 + 1000).clamp(0, 2000);
    let fill_len = ((normalized * max_fill) / 2000) as u32;

    if fill_len > 0 {
        Rectangle::new(Point::new(x + 1, y + 1), Size::new(fill_len, height - 2))
            .into_styled(fill_style)
            .draw(lcd)
            .ok();
    }

    if trim != 0 {
        // Trim tick: -25..+25 steps maps to ±10% (±100 µs) of full travel (2000 counts)
        let trim_offset = (trim as i32 * max_fill) / 250;
        let trim_x = (x + 1 + trim_offset.max(0)).min(x + max_fill);
        let tick_color = if (trim_x - (x + 1)) < fill_len as i32 {
            BinaryColor::Off
        } else {
            BinaryColor::On
        };
        let tick_style = PrimitiveStyle::with_stroke(tick_color, 1);
        Line::new(Point::new(trim_x, y + 1), Point::new(trim_x, y + height as i32 - 2))
            .into_styled(tick_style)
            .draw(lcd)
            .ok();
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
    let fill_style = PrimitiveStyle::with_fill(BinaryColor::On);
    let style = if is_selected {
        Rectangle::new(Point::new(2, y), Size::new(124, 9))
            .into_styled(fill_style)
            .draw(lcd)
            .ok();
        MonoTextStyle::new(&FONT_6X10, BinaryColor::Off)
    } else {
        MonoTextStyle::new(&FONT_6X10, BinaryColor::On)
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
    let border_style = PrimitiveStyle::with_stroke(BinaryColor::On, 1);
    let fill_style = PrimitiveStyle::with_fill(BinaryColor::On);
    box_rect.into_styled(border_style).draw(lcd).ok();
    if fill_width > 0 {
        let inner_p = Point::new(box_rect.top_left.x + 1, box_rect.top_left.y + 1);
        let inner_h = box_rect.size.height.saturating_sub(2);
        Rectangle::new(inner_p, Size::new(fill_width, inner_h))
            .into_styled(fill_style)
            .draw(lcd)
            .ok();
    }
}
