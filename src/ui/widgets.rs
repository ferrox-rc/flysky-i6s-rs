//! Reusable UI widgets and layout helpers for FlySky FS-i6X menu displays.

use crate::buzzer::Buzzer;
use crate::display::St7567;

/// Draw a standardized top header banner with title and underline divider.
pub fn draw_header(lcd: &mut St7567, title: &str) {
    let x = ((128i32 - title.len() as i32 * 6) / 2).max(2);
    lcd.draw_str_6x10(x, 2, title, false);
    lcd.draw_hline(0, 11, 128, true);
}

/// Draw a standardized bottom footer with small font (4x6) and divider line at y = 55,
/// matching the flight pages' 8-pixel footer height and baseline at y = 62 (top at y = 57).
pub fn draw_footer(lcd: &mut St7567, text: &str) {
    lcd.draw_hline(0, 55, 128, true);
    lcd.draw_str_4x6(2, 57, text, false);
}

/// Draw a standardized bottom footer with left-aligned and right-aligned text (4x6)
/// and divider line at y = 55.
pub fn draw_footer_split(lcd: &mut St7567, left: &str, right: &str) {
    lcd.draw_hline(0, 55, 128, true);
    lcd.draw_str_4x6(2, 57, left, false);
    let right_x = (126i32 - right.len() as i32 * 4).max(2);
    lcd.draw_str_4x6(right_x, 57, right, false);
}

/// Draw a standardized bottom footer with left-aligned, center-aligned, and right-aligned text (4x6)
/// and divider line at y = 55. If center_inverted is true, draws an inverted solid background behind center text.
pub fn draw_footer_three(
    lcd: &mut St7567,
    left: &str,
    center: &str,
    right: &str,
    center_inverted: bool,
) {
    lcd.draw_hline(0, 55, 128, true);
    lcd.draw_str_4x6(2, 57, left, false);
    if !center.is_empty() {
        let center_x = ((128i32 - center.len() as i32 * 4) / 2).max(2);
        if center_inverted {
            lcd.fill_rect(center_x - 2, 56, center.len() as u32 * 4 + 3, 7, true);
            lcd.draw_str_4x6(center_x, 57, center, true);
        } else {
            lcd.draw_str_4x6(center_x, 57, center, false);
        }
    }
    let right_x = (126i32 - right.len() as i32 * 4).max(2);
    lcd.draw_str_4x6(right_x, 57, right, false);
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

/// Draw a horizontal calibration extent gauge with center reference, min/max extent fill,
/// and a live cursor position marker.
#[allow(clippy::too_many_arguments)]
pub fn draw_calib_gauge(
    lcd: &mut St7567,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    center: u16,
    min: u16,
    max: u16,
    current: u16,
    target_span: u32,
) {
    if width < 6 || height < 3 {
        return;
    }
    lcd.draw_rect(x, y, width, height, true);

    let center_x = x + (width as i32 / 2);
    lcd.draw_vline(center_x, y, height, true);

    let half_w = (width as i32 / 2).saturating_sub(1);
    if half_w <= 0 || target_span == 0 {
        return;
    }

    let span_neg = center.saturating_sub(min);
    let span_pos = max.saturating_sub(center);
    let mid_y = y + (height as i32 / 2);

    // Negative span fill (leftwards from center)
    let left_w = (((span_neg as u32) * (half_w as u32)) / target_span).min(half_w as u32) as i32;
    if left_w > 0 {
        lcd.draw_hline(center_x - left_w, mid_y, left_w as u32 + 1, true);
    }

    // Positive span fill (rightwards from center)
    let right_w = (((span_pos as u32) * (half_w as u32)) / target_span).min(half_w as u32) as i32;
    if right_w > 0 {
        lcd.draw_hline(center_x, mid_y, right_w as u32 + 1, true);
    }

    // Current live position cursor
    let delta = current as i32 - center as i32;
    let cur_x = if delta < 0 {
        center_x - (((delta.unsigned_abs() * (half_w as u32)) / target_span).min(half_w as u32) as i32)
    } else {
        center_x + (((delta as u32 * (half_w as u32)) / target_span).min(half_w as u32) as i32)
    };
    let marker_h = height.saturating_sub(2);
    if marker_h > 0 {
        lcd.draw_vline(cur_x, y + 1, marker_h, true);
    }
}

/// Draw the 4 primary flight stick calibration gauges with labels, travel extents, and readiness indicators.
pub fn draw_calib_sticks(
    lcd: &mut St7567,
    labels: &[&str; 4],
    centers: &[u16],
    mins: &[u16],
    maxs: &[u16],
    current: &[u16],
    stick_ready: &[bool; 4],
) {
    for i in 0..4 {
        let y = 13 + (i as i32 * 8);
        lcd.draw_str_6x10(2, y - 1, labels[i], false);

        // Horizontal axes (Aileron 0, Rudder 3) travel ~1350 counts; Vertical axes (Elevator 1, Throttle 2) ~1250 counts
        let stick_target = if i == 0 || i == 3 { 1350u32 } else { 1250u32 };
        draw_calib_gauge(
            lcd,
            10,
            y,
            63,
            7,
            centers[i],
            mins[i],
            maxs[i],
            current[i],
            stick_target,
        );

        let status = if stick_ready[i] { "OK" } else { "--" };
        lcd.draw_str_6x10(75, y - 1, status, false);
    }
}

/// Draw a compact horizontal dual split bar for rotary pots (VRa and VRb).
/// - Dimensions: width x 7 px. Top lane is VRa, bottom lane is VRb.
/// - Each lane features a center tick and sliding 3px cursor (-1000..+1000).
pub fn draw_split_pot_bar(lcd: &mut St7567, x: i32, y: i32, width: u32, vr1: i16, vr2: i16) {
    if width < 8 {
        return;
    }
    // Top border, middle divider, and bottom border
    lcd.draw_hline(x, y, width, true);
    lcd.draw_hline(x, y + 3, width, true);
    lcd.draw_hline(x, y + 6, width, true);

    // Left and right end caps
    lcd.draw_vline(x, y, 7, true);
    lcd.draw_vline(x + width as i32 - 1, y, 7, true);

    // Center ticks for both top and bottom lanes
    let center_x = x + (width as i32 / 2);
    lcd.set_pixel(center_x, y + 1, true);
    lcd.set_pixel(center_x, y + 5, true);

    // Travel range for sliding cursors
    let min_pos = x + 2;
    let max_pos = x + width as i32 - 3;
    let travel = (max_pos - min_pos).max(1);

    // Top cursor: VRa (y + 1..y + 2, 3px wide)
    let c1 = min_pos + (((vr1 as i32 + 1000) * travel + 1000) / 2000);
    lcd.fill_rect(c1 - 1, y + 1, 3, 2, true);

    // Bottom cursor: VRb (y + 4..y + 5, 3px wide)
    let c2 = min_pos + (((vr2 as i32 + 1000) * travel + 1000) / 2000);
    lcd.fill_rect(c2 - 1, y + 4, 3, 2, true);
}

/// Draw a single horizontal pot bar (width x 7 px) with center tick and sliding cursor.
pub fn draw_single_pot_bar(lcd: &mut St7567, x: i32, y: i32, width: u32, val: i16) {
    if width < 8 {
        return;
    }
    lcd.draw_rect(x, y, width, 7, true);
    let center_x = x + (width as i32 / 2);
    lcd.draw_vline(center_x, y + 1, 5, true);

    let min_pos = x + 2;
    let max_pos = x + width as i32 - 3;
    let travel = (max_pos - min_pos).max(1);
    let c = min_pos + (((val as i32 + 1000) * travel + 1000) / 2000);
    lcd.fill_rect(c - 1, y + 1, 3, 5, true);
}

/// Draw a single borderless pot lane with a baseline track, center tick, and sliding cursor.
/// Height is 2..3 px, saving vertical space so lanes can be stacked without bounding box overhead.
pub fn draw_pot_track(lcd: &mut St7567, x: i32, y: i32, width: u32, val: i16) {
    if width < 6 {
        return;
    }
    // Baseline track
    lcd.draw_hline(x, y + 1, width, true);

    // Sliding cursor (2px wide, centered on track at y..y+2)
    let min_pos = x + 1;
    let max_pos = x + width as i32 - 2;
    let travel = (max_pos - min_pos).max(1);
    let c = min_pos + (((val as i32 + 1000) * travel + 1000) / 2000);
    lcd.fill_rect(c - 1, y, 2, 3, true);
}

/// Draw a cluster of 3 to 10 pots using borderless horizontal lanes:
/// - 3 pots: 1 column of 3 stacked full-width tracks (y, y+3, y+6).
/// - 4 pots: 2 columns of 2 stacked half-width tracks (Left: 2, Right: 2).
/// - 5 pots: 2 columns (Left: 3 stacked tracks, Right: 2 stacked tracks).
/// - 6 pots: 2 columns (Left: 3 stacked tracks, Right: 3 stacked tracks).
/// - 7+ pots: 3 columns of stacked tracks.
pub fn draw_multi_pot_bar(lcd: &mut St7567, x: i32, y: i32, width: u32, pots: &[i16]) {
    let count = pots.len();
    if count == 0 || width < 12 {
        return;
    }

    if count == 3 {
        draw_pot_track(lcd, x, y, width, pots[0]);
        draw_pot_track(lcd, x, y + 3, width, pots[1]);
        draw_pot_track(lcd, x, y + 6, width, pots[2]);
    } else if count <= 6 {
        let col_gap = 2;
        let col_w = (width.saturating_sub(col_gap) / 2).max(6);
        let right_x = x + col_w as i32 + col_gap as i32;

        let left_count = if count == 4 { 2 } else { 3 };
        let right_count = count - left_count;

        for (row, &val) in pots.iter().take(left_count).enumerate() {
            let ry = if left_count == 2 {
                y + 1 + (row as i32 * 4)
            } else {
                y + (row as i32 * 3)
            };
            draw_pot_track(lcd, x, ry, col_w, val);
        }

        for (row, &val) in pots[left_count..].iter().take(right_count).enumerate() {
            let ry = if right_count == 2 {
                y + 1 + (row as i32 * 4)
            } else {
                y + (row as i32 * 3)
            };
            draw_pot_track(lcd, right_x, ry, col_w, val);
        }
    } else {
        let col_gap = 2;
        let col_w = (width.saturating_sub(col_gap * 2) / 3).max(6);
        let rows_per_col = count.div_ceil(3);

        for (i, &val) in pots.iter().enumerate() {
            let col = i / rows_per_col;
            let row = i % rows_per_col;
            let cx = x + (col as i32 * (col_w as i32 + col_gap as i32));
            let ry = y + (row as i32 * 3);
            draw_pot_track(lcd, cx, ry, col_w, val);
        }
    }
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

    let max_fill = (width - 2) as i32;
    let normalized = (val as i32 + 1000).clamp(0, 2000);
    let fill_len = ((normalized * max_fill) / 2000) as u32;

    if fill_len > 0 {
        lcd.fill_rect(x + 1, y + 1, fill_len, height.saturating_sub(2), true);
    }

    if trim != 0 {
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
pub fn draw_scrollbar(lcd: &mut St7567, selected: usize, count: usize, top_y: i32, height: u32) {
    if count <= 1 {
        return;
    }
    lcd.draw_vline(126, top_y, height, true);

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
    F: FnOnce(&mut St7567, i32, i32, bool),
{
    let y = 13 + (slot as i32 * 14);
    let icon_on = !is_selected;
    if is_selected {
        lcd.fill_rect(2, y, 121, 13, true);
    }

    let text_x = if let Some(draw_fn) = draw_icon {
        draw_fn(lcd, 4, y + 1, icon_on);
        19
    } else {
        4
    };

    lcd.draw_str_6x10(text_x, y + 3, label, is_selected);
    if let Some(val) = value {
        lcd.draw_str_6x10(value_x, y + 3, val, is_selected);
    }
}

/// Render a single list item row within a 4-slot view with highlight.
/// If `value_x <= 0`, the value string is automatically right-aligned against the row right edge.
pub fn draw_list_row(
    lcd: &mut St7567,
    slot: usize,
    is_selected: bool,
    label: &str,
    value: Option<&str>,
    value_x: i32,
) {
    let y = 14 + (slot as i32 * 9);
    if is_selected {
        lcd.fill_rect(2, y, 124, 9, true);
    }

    lcd.draw_str_6x10(4, y, label, is_selected);
    if let Some(val) = value {
        let vx = if value_x <= 0 {
            124 - (val.len() as i32 * 6)
        } else {
            value_x
        };
        lcd.draw_str_6x10(vx, y, val, is_selected);
    }
}

/// Render a single list item row within a 4-slot view with highlight, right-aligning the value.
pub fn draw_list_row_right(
    lcd: &mut St7567,
    slot: usize,
    is_selected: bool,
    label: &str,
    value: Option<&str>,
) {
    draw_list_row(lcd, slot, is_selected, label, value, 0);
}

/// Draw a framed bar gauge meter with an inner filled level.
pub fn draw_bar_gauge(lcd: &mut St7567, x: i32, y: i32, width: u32, height: u32, fill_width: u32) {
    lcd.draw_rect(x, y, width, height, true);
    if fill_width > 0 {
        let inner_p_x = x + 1;
        let inner_p_y = y + 1;
        let inner_h = height.saturating_sub(2);
        lcd.fill_rect(inner_p_x, inner_p_y, fill_width, inner_h, true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_draw_list_row_right_alignment() {
        let mut lcd = St7567::new();
        draw_list_row_right(&mut lcd, 0, false, "Thr Trim:", Some("IDLE"));
        draw_list_row_right(&mut lcd, 1, true, "Beeper:", Some("ENABLED"));
        draw_list_row_right(&mut lcd, 2, false, "[Configure Module]", None);
    }
}