//! Native ExpressLRS / TBS CRSF configuration menu screen.
//!
//! Features TBS-Agent style auto-discovery of all bus devices (TX, RX, FC),
//! hierarchical folder drill-down and back navigation, and in-place modal option editing.

use embedded_graphics::{
    mono_font::{ascii::FONT_4X6, ascii::FONT_6X10, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
    text::Text,
};

use crate::buzzer::Buzzer;
use crate::crsf::{self, ElrsConfigState};
use crate::display::St7567;
use crate::menu::widgets;
use crate::menu::{MenuController, MenuState, NavKeys};

/// Helper to exit back to the previous radio setup menu.
fn exit_to_parent_menu(ctrl: &mut MenuController, buzzer: &mut Buzzer) {
    if ctrl.return_state == MenuState::MainMenu {
        ctrl.return_to_main_menu();
    } else {
        ctrl.state = MenuState::RxSetup;
        ctrl.selected_item = 2;
    }
    ctrl.editing = false;
    ctrl.page_idx = 0;
    ctrl.waiting_release = true;
    buzzer.click();
}

#[inline(never)]
pub fn update(ctrl: &mut MenuController, lcd: &mut St7567, keys: &NavKeys, buzzer: &mut Buzzer) {
    let engine = crsf::get_config_engine();

    // 1. Intercept keys if a command confirmation dialog is active
    if let crsf::ActiveCommandState::WaitingConfirm { .. } = engine.active_cmd {
        if keys.cancel {
            crsf::confirm_command(false);
            buzzer.click();
            return;
        }
        if keys.ok {
            crsf::confirm_command(true);
            buzzer.play_tone(2400, 80);
            return;
        }
    }

    // 2. Intercept keys if a command has completed (allow immediate dismissal)
    if let crsf::ActiveCommandState::Completed { .. } = engine.active_cmd {
        if keys.ok || keys.cancel {
            crsf::dismiss_command();
            buzzer.click();
            return;
        }
    }

    let text_style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
    let text_style_small = MonoTextStyle::new(&FONT_4X6, BinaryColor::On);
    let fill_style = PrimitiveStyle::with_fill(BinaryColor::On);

    match engine.state {
        // --- State: Idle / Device Discovery (TBS-Agent Device List) ---
        ElrsConfigState::Idle | ElrsConfigState::Discovering => {
            if engine.devices_len == 0 {
                // No devices responded to discovery pings yet
                if keys.ok {
                    ctrl.waiting_release = true;
                    crsf::start_config();
                    buzzer.click();
                } else if keys.cancel {
                    exit_to_parent_menu(ctrl, buzzer);
                    return;
                }

                widgets::draw_header(lcd, "CRSF DEVICES");
                Text::new("Searching for devices...", Point::new(4, 26), text_style)
                    .draw(lcd)
                    .ok();
                Text::new(
                    "Listening for pings (1Hz)",
                    Point::new(4, 38),
                    text_style_small,
                )
                .draw(lcd)
                .ok();

                widgets::draw_footer(lcd, "[OK] Scan   [ESC] Back");
            } else {
                if ctrl.selected_item >= engine.devices_len && engine.devices_len > 0 {
                    ctrl.selected_item = engine.devices_len - 1;
                }
                if ctrl.scroll_offset >= engine.devices_len && engine.devices_len > 0 {
                    ctrl.scroll_offset = engine.devices_len.saturating_sub(4);
                }
                widgets::navigate_4slot_list(
                    &mut ctrl.selected_item,
                    &mut ctrl.scroll_offset,
                    engine.devices_len,
                    keys.up,
                    keys.down,
                    buzzer,
                );

                if keys.ok {
                    let sel = ctrl.selected_item;
                    if crsf::select_device(sel) {
                        ctrl.selected_item = 0;
                        ctrl.scroll_offset = 0;
                        ctrl.page_idx = 0; // Root folder (parent = 0)
                        ctrl.editing = false;
                        ctrl.waiting_release = true;
                        buzzer.click();
                        return;
                    }
                } else if keys.cancel {
                    exit_to_parent_menu(ctrl, buzzer);
                    return;
                }

                widgets::draw_header(lcd, "CRSF DEVICES");

                let has_scroll = engine.devices_len > 4;
                let row_w: u32 = if has_scroll { 121 } else { 124 };

                // Render up to 4 discovered devices
                for slot in 0..4 {
                    let idx = ctrl.scroll_offset + slot;
                    if idx >= engine.devices_len {
                        break;
                    }
                    let y = 14 + (slot as i32 * 9);
                    let is_sel = idx == ctrl.selected_item;
                    let style = if is_sel {
                        Rectangle::new(Point::new(2, y), Size::new(row_w, 9))
                            .into_styled(fill_style)
                            .draw(lcd)
                            .ok();
                        MonoTextStyle::new(&FONT_6X10, BinaryColor::Off)
                    } else {
                        text_style
                    };

                    let dev = &engine.devices[idx];
                    let d_name =
                        crate::ui::format::ascii_as_str(&dev.name[..dev.name_len as usize]);

                    // Left: Device Name
                    Text::new(d_name, Point::new(4, y + 7), style)
                        .draw(lcd)
                        .ok();

                    // Right: [Role] Tag (e.g. [TX], [RX], [FC], [VTX], [WIFI], [ESC1])
                    // Or raw hex fallback for uncommon devices (e.g. [0x55])
                    let tag_style = if is_sel {
                        MonoTextStyle::new(&FONT_4X6, BinaryColor::Off)
                    } else {
                        text_style_small
                    };

                    let mut tag_buf = [b' '; 9];
                    tag_buf[0] = b'[';
                    let tag_len = match crsf::protocol::device_role_str(dev.address) {
                        Some(role) => {
                            let r_bytes = role.as_bytes();
                            let r_len = r_bytes.len().min(4);
                            tag_buf[1..1 + r_len].copy_from_slice(&r_bytes[..r_len]);
                            tag_buf[1 + r_len] = b']';
                            r_len + 2
                        }
                        None => {
                            tag_buf[1] = b'0';
                            tag_buf[2] = b'x';
                            const HEX: &[u8; 16] = b"0123456789ABCDEF";
                            tag_buf[3] = HEX[((dev.address >> 4) & 0x0F) as usize];
                            tag_buf[4] = HEX[(dev.address & 0x0F) as usize];
                            tag_buf[5] = b']';
                            6
                        }
                    };

                    let tag_str = crate::ui::format::ascii_as_str(&tag_buf[..tag_len]);
                    let tag_x = (row_w as i32) - (tag_len as i32 * 4);
                    Text::new(tag_str, Point::new(tag_x, y + 7), tag_style)
                        .draw(lcd)
                        .ok();
                }

                if has_scroll {
                    widgets::draw_scrollbar(lcd, ctrl.selected_item, engine.devices_len, 14, 36);
                }

                widgets::draw_footer(lcd, "[OK] Select   [ESC] Back");
            }
        }

        // --- State: Loading Parameters for Selected Device (Initial Connection) ---
        ElrsConfigState::Connected | ElrsConfigState::LoadingParam(_) if !engine.folder_loading => {
            if keys.cancel {
                // Abort loading and return to device list
                crsf::return_to_device_list();
                ctrl.selected_item = 0;
                ctrl.scroll_offset = 0;
                ctrl.page_idx = 0;
                ctrl.editing = false;
                buzzer.click();
                return;
            }

            widgets::draw_header(lcd, "CRSF CONFIG");
            Text::new("Loading...", Point::new(12, 28), text_style)
                .draw(lcd)
                .ok();

            widgets::draw_footer(lcd, "Please wait...  [ESC] Abort");
        }

        // --- State: Ready / Interactive Parameter Navigation & On-Demand Folder Paging ---
        ElrsConfigState::Ready | ElrsConfigState::LoadingParam(_) | ElrsConfigState::Connected => {
            let current_folder = engine.current_folder;
            let mut folder_indices = [0u8; crsf::MAX_FOLDER_ITEMS];
            let count = crsf::get_folder_params(current_folder, &mut folder_indices);

            if count == 0 {
                if engine.params_len == 0 {
                    if engine.folder_loading {
                        widgets::draw_header(lcd, "[Loading...]");
                        Text::new("Loading items...", Point::new(12, 28), text_style)
                            .draw(lcd)
                            .ok();
                        widgets::draw_footer(lcd, "Loading...    [ESC] Back");
                        if keys.cancel {
                            if crsf::exit_current_folder() {
                                ctrl.page_idx = engine.current_folder as usize;
                                ctrl.selected_item = 0;
                                ctrl.scroll_offset = 0;
                                buzzer.click();
                                return;
                            } else {
                                crsf::return_to_device_list();
                                ctrl.page_idx = 0;
                                ctrl.selected_item = 0;
                                ctrl.scroll_offset = 0;
                                buzzer.click();
                                return;
                            }
                        }
                        return;
                    }
                    // No parameters found for this device at all
                    if keys.ok {
                        ctrl.waiting_release = true;
                        crsf::select_device(engine.selected_device_idx);
                        buzzer.click();
                    } else if keys.cancel {
                        crsf::return_to_device_list();
                        ctrl.selected_item = 0;
                        ctrl.scroll_offset = 0;
                        buzzer.click();
                        return;
                    }
                    widgets::draw_header(lcd, "CRSF CONFIG");
                    Text::new("No parameters found", Point::new(8, 28), text_style)
                        .draw(lcd)
                        .ok();
                    widgets::draw_footer(lcd, "[OK] Retry   [ESC] Devices");
                } else {
                    // Empty subfolder
                    if keys.cancel {
                        if crsf::exit_current_folder() {
                            ctrl.page_idx = engine.current_folder as usize;
                            ctrl.selected_item = 0;
                            ctrl.scroll_offset = 0;
                            buzzer.click();
                            return;
                        } else {
                            crsf::return_to_device_list();
                            ctrl.page_idx = 0;
                            ctrl.selected_item = 0;
                            ctrl.scroll_offset = 0;
                            buzzer.click();
                            return;
                        }
                    }
                    let mut fbuf = [0u8; crsf::MAX_FOLDER_NAME_LEN];
                    let fname = crsf::get_folder_name(current_folder, &mut fbuf);
                    widgets::draw_header(lcd, fname);
                    Text::new("Empty folder", Point::new(12, 28), text_style)
                        .draw(lcd)
                        .ok();
                    widgets::draw_footer(lcd, "[ESC] Up");
                }
            } else {
                let in_confirm = matches!(
                    engine.active_cmd,
                    crsf::ActiveCommandState::WaitingConfirm { .. }
                );

                if !in_confirm {
                    if ctrl.editing {
                        // In-Place Modal Edit Phase: UP/DOWN cycles tentative option value
                        let actual_idx = folder_indices[ctrl.selected_item] as usize;
                        let p = &engine.params[actual_idx];
                        let p_type = p.clean_type();

                        if p_type == crsf::protocol::CRSF_TYPE_SELECT {
                            if keys.up {
                                if ctrl.sub_idx > 0 {
                                    ctrl.sub_idx -= 1;
                                } else {
                                    ctrl.sub_idx = p.max_value as usize;
                                }
                                buzzer.click();
                            } else if keys.down {
                                if ctrl.sub_idx < p.max_value as usize {
                                    ctrl.sub_idx += 1;
                                } else {
                                    ctrl.sub_idx = 0;
                                }
                                buzzer.click();
                            } else if keys.ok {
                                // Commit change: transmit write frame to module & exit edit mode
                                crsf::set_param_value(actual_idx, ctrl.sub_idx as i32);
                                ctrl.editing = false;
                                buzzer.play_tone(2400, 80);
                            } else if keys.cancel {
                                // Cancel edit: revert without transmitting
                                ctrl.editing = false;
                                buzzer.click();
                            }
                        } else if p.is_integer() {
                            if keys.up {
                                if ctrl.edit_val < p.max_value {
                                    ctrl.edit_val += 1;
                                    buzzer.click();
                                }
                            } else if keys.down {
                                if ctrl.edit_val > p.min_value {
                                    ctrl.edit_val -= 1;
                                    buzzer.click();
                                }
                            } else if keys.ok {
                                crsf::set_param_value(actual_idx, ctrl.edit_val);
                                ctrl.editing = false;
                                buzzer.play_tone(2400, 80);
                            } else if keys.cancel {
                                ctrl.editing = false;
                                buzzer.click();
                            }
                        }
                    } else {
                        // Standard Navigation Phase
                        widgets::navigate_4slot_list(
                            &mut ctrl.selected_item,
                            &mut ctrl.scroll_offset,
                            count,
                            keys.up,
                            keys.down,
                            buzzer,
                        );

                        if keys.ok {
                            let actual_idx = folder_indices[ctrl.selected_item] as usize;
                            let p = &engine.params[actual_idx];
                            let p_type = p.clean_type();

                            if p_type == crsf::protocol::CRSF_TYPE_FOLDER {
                                // Drill down into subfolder
                                let f_name = p.name(&engine.string_pool);
                                crsf::enter_folder(p.id, f_name);
                                ctrl.page_idx = p.id as usize;
                                ctrl.selected_item = 0;
                                ctrl.scroll_offset = 0;
                                buzzer.click();
                            } else if p_type == crsf::protocol::CRSF_TYPE_SELECT {
                                // Enter modal edit mode on SELECT
                                ctrl.editing = true;
                                ctrl.sub_idx = p.value as usize;
                                buzzer.click();
                            } else if p.is_integer() {
                                // Enter modal edit mode on INTEGER
                                ctrl.editing = true;
                                ctrl.edit_val = p.value;
                                buzzer.click();
                            } else if p_type == crsf::protocol::CRSF_TYPE_COMMAND {
                                crsf::trigger_command(actual_idx);
                                buzzer.play_tone(2400, 80);
                            }
                        } else if keys.cancel {
                            if crsf::exit_current_folder() {
                                ctrl.page_idx = engine.current_folder as usize;
                                ctrl.selected_item = 0;
                                ctrl.scroll_offset = 0;
                                buzzer.click();
                                return;
                            } else {
                                // Step back from root folder to Device List
                                crsf::return_to_device_list();
                                ctrl.page_idx = 0;
                                ctrl.selected_item = 0;
                                ctrl.scroll_offset = 0;
                                ctrl.editing = false;
                                buzzer.click();
                                return;
                            }
                        }
                    }
                }

                // Header title: subfolder name, device name, or [Loading...] indicator
                if engine.folder_loading {
                    widgets::draw_header(lcd, "[Loading...]");
                } else if current_folder > 0 {
                    let mut fbuf = [0u8; crsf::MAX_FOLDER_NAME_LEN];
                    let fname = crsf::get_folder_name(current_folder, &mut fbuf);
                    widgets::draw_header(lcd, fname);
                } else {
                    let dev_name = if engine.device_name_len > 0 {
                        crate::ui::format::ascii_as_str(
                            &engine.device_name[..engine.device_name_len as usize],
                        )
                    } else {
                        "CRSF SETUP"
                    };
                    widgets::draw_header(lcd, dev_name);
                }

                // Render up to 4 parameters in current folder
                for slot in 0..4 {
                    let slot_idx = ctrl.scroll_offset + slot;
                    if slot_idx >= count {
                        break;
                    }
                    let actual_idx = folder_indices[slot_idx] as usize;
                    let y = 14 + (slot as i32 * 9);
                    let is_sel = slot_idx == ctrl.selected_item;
                    let style = if is_sel {
                        Rectangle::new(Point::new(2, y), Size::new(124, 9))
                            .into_styled(fill_style)
                            .draw(lcd)
                            .ok();
                        MonoTextStyle::new(&FONT_6X10, BinaryColor::Off)
                    } else {
                        text_style
                    };

                    let p = &engine.params[actual_idx];
                    let name = p.name(&engine.string_pool);
                    let p_type = p.clean_type();

                    if p_type == crsf::protocol::CRSF_TYPE_FOLDER {
                        // Folder row with trailing chevron
                        let max_n_chars = 18usize;
                        let n_disp = &name[..name.len().min(max_n_chars)];
                        Text::new(n_disp, Point::new(4, y + 7), style)
                            .draw(lcd)
                            .ok();
                        Text::new(">", Point::new(118, y + 7), style).draw(lcd).ok();
                    } else if p_type == crsf::protocol::CRSF_TYPE_SELECT {
                        let mut opt_buf = [0u8; 24];
                        let raw_opt = if is_sel && ctrl.editing {
                            p.option_str_for_val(
                                &engine.string_pool,
                                ctrl.sub_idx as u8,
                                &mut opt_buf,
                            )
                        } else {
                            p.current_option_str(&engine.string_pool, &mut opt_buf)
                        };

                        // If option has parenthetical hint e.g. "250Hz(-108dBm)", and total line is cramped,
                        // strip the parenthetical hint so the value is cleanly readable
                        let mut opt_clean_len = raw_opt.len();
                        if let Some(pos) = raw_opt.find('(') {
                            if pos > 0
                                && (name.len() + raw_opt.len() > 18 || is_sel && ctrl.editing)
                            {
                                opt_clean_len = pos;
                            }
                        }
                        let opt_clean = raw_opt[..opt_clean_len].trim_end();

                        if is_sel && ctrl.editing {
                            // Modal edit display: bracketed with '<' and '>'
                            let mut ed_buf = [b' '; 26];
                            ed_buf[0] = b'<';
                            ed_buf[1] = b' ';
                            let o_len = opt_clean.len().min(14);
                            ed_buf[2..2 + o_len].copy_from_slice(&opt_clean.as_bytes()[..o_len]);
                            ed_buf[2 + o_len] = b' ';
                            ed_buf[3 + o_len] = b'>';
                            let ed_str = crate::ui::format::ascii_as_str(&ed_buf[..4 + o_len]);

                            let val_x = 124i32.saturating_sub((ed_str.len() as i32) * 6);
                            let max_name_chars = 20usize.saturating_sub(ed_str.len() + 1).max(5);
                            let name_disp = &name[..name.len().min(max_name_chars)];
                            Text::new(name_disp, Point::new(4, y + 7), style)
                                .draw(lcd)
                                .ok();
                            Text::new(ed_str, Point::new(val_x, y + 7), style)
                                .draw(lcd)
                                .ok();
                        } else {
                            let val_x = 124i32.saturating_sub((opt_clean.len() as i32) * 6);
                            let max_name_chars = 20usize.saturating_sub(opt_clean.len() + 1).max(5);
                            let name_disp = &name[..name.len().min(max_name_chars)];
                            Text::new(name_disp, Point::new(4, y + 7), style)
                                .draw(lcd)
                                .ok();
                            Text::new(opt_clean, Point::new(val_x, y + 7), style)
                                .draw(lcd)
                                .ok();
                        }
                    } else if p_type == crsf::protocol::CRSF_TYPE_COMMAND {
                        let is_active = match engine.active_cmd {
                            crsf::ActiveCommandState::Starting { param_id, .. }
                            | crsf::ActiveCommandState::Running { param_id, .. }
                            | crsf::ActiveCommandState::Completed { param_id, .. } => {
                                param_id == p.id
                            }
                            _ => false,
                        };
                        if is_active {
                            let info_str = engine.cmd_info_str();
                            let text_to_show = if !info_str.is_empty() {
                                info_str
                            } else {
                                match engine.active_cmd {
                                    crsf::ActiveCommandState::Completed { .. } => "OK",
                                    _ => "Executing...",
                                }
                            };
                            let mut cmd_buf = [b' '; 22];
                            cmd_buf[0] = b'[';
                            let t_bytes = text_to_show.as_bytes();
                            let t_len = t_bytes.len().min(18);
                            cmd_buf[1..1 + t_len].copy_from_slice(&t_bytes[..t_len]);
                            cmd_buf[1 + t_len] = b']';
                            let c_str = crate::ui::format::ascii_as_str(&cmd_buf[..2 + t_len]);
                            Text::new(c_str, Point::new(6, y + 7), style).draw(lcd).ok();
                        } else {
                            let mut cmd_buf = [b' '; 22];
                            cmd_buf[0] = b'[';
                            let n_bytes = name.as_bytes();
                            let n_len = n_bytes.len().min(18);
                            cmd_buf[1..1 + n_len].copy_from_slice(&n_bytes[..n_len]);
                            cmd_buf[1 + n_len] = b']';
                            let c_str = crate::ui::format::ascii_as_str(&cmd_buf[..2 + n_len]);
                            Text::new(c_str, Point::new(6, y + 7), style).draw(lcd).ok();
                        }
                    } else if p_type == crsf::protocol::CRSF_TYPE_INFO
                        || p_type == crsf::protocol::CRSF_TYPE_STRING
                    {
                        let info = p.info_str(&engine.string_pool);
                        if info.is_empty() {
                            let max_n_chars = 19usize;
                            let n_disp = &name[..name.len().min(max_n_chars)];
                            Text::new(n_disp, Point::new(4, y + 7), style)
                                .draw(lcd)
                                .ok();
                        } else {
                            let use_small = info.len() > 7 || name.len() + info.len() > 17;
                            if use_small {
                                let tag_style = if is_sel {
                                    MonoTextStyle::new(&FONT_4X6, BinaryColor::Off)
                                } else {
                                    text_style_small
                                };
                                let max_info_chars = 15usize;
                                let info_len = info.len().min(max_info_chars);
                                let info_disp = &info[..info_len];
                                let val_x = 124i32.saturating_sub(info_len as i32 * 4);
                                let max_name_width = (val_x - 6).max(24);
                                let max_name_chars = (max_name_width / 6) as usize;
                                let name_disp = &name[..name.len().min(max_name_chars)];

                                Text::new(name_disp, Point::new(4, y + 7), style)
                                    .draw(lcd)
                                    .ok();
                                Text::new(info_disp, Point::new(val_x, y + 7), tag_style)
                                    .draw(lcd)
                                    .ok();
                            } else {
                                let val_x = 124i32.saturating_sub(info.len() as i32 * 6);
                                let max_name_chars = 20usize.saturating_sub(info.len() + 1).max(5);
                                let name_disp = &name[..name.len().min(max_name_chars)];

                                Text::new(name_disp, Point::new(4, y + 7), style)
                                    .draw(lcd)
                                    .ok();
                                Text::new(info, Point::new(val_x, y + 7), style)
                                    .draw(lcd)
                                    .ok();
                            }
                        }
                    } else if p.is_integer() {
                        let mut num_buf = [0u8; 12];
                        let val_to_show = if is_sel && ctrl.editing {
                            ctrl.edit_val
                        } else {
                            p.value
                        };
                        let num_len = crate::ui::format::i32_to_dec(val_to_show, &mut num_buf);
                        let num_str = crate::ui::format::ascii_as_str(&num_buf[..num_len]);
                        let unit_str = p.unit_str(&engine.string_pool);

                        if is_sel && ctrl.editing {
                            // Modal edit display: bracketed with '<' and '>' e.g. "< 4 >" or "< 4 ch >"
                            let mut ed_buf = [b' '; 26];
                            ed_buf[0] = b'<';
                            ed_buf[1] = b' ';
                            let mut offset = 2;
                            let n_bytes = num_str.as_bytes();
                            let n_len = n_bytes.len().min(12);
                            ed_buf[offset..offset + n_len].copy_from_slice(&n_bytes[..n_len]);
                            offset += n_len;

                            if !unit_str.is_empty() {
                                ed_buf[offset] = b' ';
                                offset += 1;
                                let u_bytes = unit_str.as_bytes();
                                let u_len = u_bytes.len().min(8);
                                ed_buf[offset..offset + u_len].copy_from_slice(&u_bytes[..u_len]);
                                offset += u_len;
                            }

                            ed_buf[offset] = b' ';
                            ed_buf[offset + 1] = b'>';
                            let total_len = offset + 2;
                            let ed_str = crate::ui::format::ascii_as_str(&ed_buf[..total_len]);

                            let val_x = 124i32.saturating_sub((ed_str.len() as i32) * 6);
                            let max_name_chars = 20usize.saturating_sub(ed_str.len() + 1).max(5);
                            let name_disp = &name[..name.len().min(max_name_chars)];
                            Text::new(name_disp, Point::new(4, y + 7), style)
                                .draw(lcd)
                                .ok();
                            Text::new(ed_str, Point::new(val_x, y + 7), style)
                                .draw(lcd)
                                .ok();
                        } else {
                            let mut full_val_buf = [0u8; 24];
                            let mut offset = 0;
                            let n_bytes = num_str.as_bytes();
                            full_val_buf[offset..offset + n_bytes.len()].copy_from_slice(n_bytes);
                            offset += n_bytes.len();
                            if !unit_str.is_empty() {
                                full_val_buf[offset] = b' ';
                                offset += 1;
                                let u_bytes = unit_str.as_bytes();
                                let u_len = u_bytes.len().min(full_val_buf.len() - offset);
                                full_val_buf[offset..offset + u_len].copy_from_slice(&u_bytes[..u_len]);
                                offset += u_len;
                            }
                            let full_val_str = crate::ui::format::ascii_as_str(&full_val_buf[..offset]);
                            let val_x = 124i32.saturating_sub((full_val_str.len() as i32) * 6);
                            let max_name_chars = 20usize.saturating_sub(full_val_str.len() + 1).max(5);
                            let name_disp = &name[..name.len().min(max_name_chars)];
                            Text::new(name_disp, Point::new(4, y + 7), style)
                                .draw(lcd)
                                .ok();
                            Text::new(full_val_str, Point::new(val_x, y + 7), style)
                                .draw(lcd)
                                .ok();
                        }
                    } else {
                        let max_n_chars = 19usize;
                        let n_disp = &name[..name.len().min(max_n_chars)];
                        Text::new(n_disp, Point::new(4, y + 7), style)
                            .draw(lcd)
                            .ok();
                    }
                }

                // If confirmation modal is needed, draw centered overlay box
                if let crsf::ActiveCommandState::WaitingConfirm { param_id } = engine.active_cmd {
                    let p_name = engine.params[..engine.params_len]
                        .iter()
                        .find(|p| p.id == param_id)
                        .map(|p| p.name(&engine.string_pool))
                        .unwrap_or("Action");

                    Rectangle::new(Point::new(10, 16), Size::new(108, 32))
                        .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
                        .draw(lcd)
                        .ok();
                    Rectangle::new(Point::new(10, 16), Size::new(108, 32))
                        .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
                        .draw(lcd)
                        .ok();

                    let info_prompt = engine.cmd_info_str();
                    if !info_prompt.is_empty() {
                        let prompt_len = info_prompt.len().min(17);
                        let prompt_disp = &info_prompt[..prompt_len];
                        let prompt_x = 10 + (108i32.saturating_sub(prompt_len as i32 * 6) / 2);
                        Text::new(prompt_disp, Point::new(prompt_x, 27), text_style)
                            .draw(lcd)
                            .ok();
                    } else {
                        let mut q_buf = [b' '; 18];
                        q_buf[..6].copy_from_slice(b"Run: [");
                        let n_len = p_name.len().min(8);
                        q_buf[6..6 + n_len].copy_from_slice(&p_name.as_bytes()[..n_len]);
                        q_buf[6 + n_len] = b']';
                        q_buf[7 + n_len] = b'?';
                        let q_str = crate::ui::format::ascii_as_str(&q_buf[..8 + n_len]);
                        let q_x = 10 + (108i32.saturating_sub((8 + n_len) as i32 * 6) / 2);
                        Text::new(q_str, Point::new(q_x, 27), text_style)
                            .draw(lcd)
                            .ok();
                    }
                    Text::new("[OK] Yes  [ESC] No", Point::new(20, 42), text_style_small)
                        .draw(lcd)
                        .ok();

                    widgets::draw_footer(lcd, "[OK] Confirm   [ESC] Cancel");
                } else if engine.folder_loading {
                    widgets::draw_footer(lcd, "Loading...    [ESC] Back");
                } else if ctrl.editing {
                    widgets::draw_footer(lcd, "[OK] Confirm   [ESC] Cancel");
                } else {
                    let sel_p = if ctrl.selected_item < count {
                        let actual_idx = folder_indices[ctrl.selected_item] as usize;
                        if actual_idx < engine.params_len {
                            Some(&engine.params[actual_idx])
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    let esc_text = if current_folder > 0 {
                        "[ESC] Up"
                    } else {
                        "[ESC] Devices"
                    };

                    if let Some(p) = sel_p {
                        let p_type = p.clean_type();
                        if p_type == crsf::protocol::CRSF_TYPE_INFO
                            || p_type == crsf::protocol::CRSF_TYPE_STRING
                        {
                            let info = p.info_str(&engine.string_pool);
                            if info.len() > 6 {
                                let max_chars = 20usize;
                                let disp_len = info.len().min(max_chars);
                                widgets::draw_footer_split(lcd, &info[..disp_len], esc_text);
                            } else {
                                widgets::draw_footer(lcd, esc_text);
                            }
                        } else {
                            let ok_text = if p_type == crsf::protocol::CRSF_TYPE_FOLDER {
                                "[OK] Enter    "
                            } else if p_type == crsf::protocol::CRSF_TYPE_SELECT || p.is_integer() {
                                "[OK] Edit     "
                            } else {
                                "[OK] Select   "
                            };
                            let mut fbuf = [b' '; 26];
                            fbuf[..14].copy_from_slice(ok_text.as_bytes());
                            let e_bytes = esc_text.as_bytes();
                            let e_len = e_bytes.len().min(12);
                            fbuf[14..14 + e_len].copy_from_slice(&e_bytes[..e_len]);
                            let f_str = crate::ui::format::ascii_as_str(&fbuf[..14 + e_len]);
                            widgets::draw_footer(lcd, f_str);
                        }
                    } else if current_folder > 0 {
                        widgets::draw_footer(lcd, "[OK] Select   [ESC] Up");
                    } else {
                        widgets::draw_footer(lcd, "[OK] Select   [ESC] Devices");
                    }
                }
            }
        }
    }
}
