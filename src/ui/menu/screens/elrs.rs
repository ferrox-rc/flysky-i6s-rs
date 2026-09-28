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
        ctrl.state = MenuState::MainMenu;
        ctrl.selected_item = 8;
        ctrl.scroll_offset = 5;
    } else {
        ctrl.state = MenuState::RxSetup;
        ctrl.selected_item = 2;
    }
    ctrl.editing = false;
    ctrl.page_idx = 0;
    ctrl.waiting_release = true;
    buzzer.click();
}

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
                // One or more devices discovered! Present TBS-Agent device picker
                if ctrl.selected_item >= engine.devices_len && engine.devices_len > 0 {
                    ctrl.selected_item = engine.devices_len - 1;
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

                // Render up to 4 discovered devices
                for slot in 0..4 {
                    let idx = ctrl.scroll_offset + slot;
                    if idx >= engine.devices_len {
                        break;
                    }
                    let y = 14 + (slot as i32 * 9);
                    let is_sel = idx == ctrl.selected_item;
                    let style = if is_sel {
                        Rectangle::new(Point::new(2, y), Size::new(124, 9))
                            .into_styled(fill_style)
                            .draw(lcd)
                            .ok();
                        MonoTextStyle::new(&FONT_6X10, BinaryColor::Off)
                    } else {
                        text_style
                    };

                    let dev = &engine.devices[idx];
                    let d_name = core::str::from_utf8(&dev.name[..dev.name_len as usize])
                        .unwrap_or("Device");
                    let role = crsf::protocol::device_role_str(dev.address);

                    // Left: Device Name
                    Text::new(d_name, Point::new(4, y + 7), style).draw(lcd).ok();

                    // Right: [Role] Tag (e.g. [TX], [RX], [FC])
                    let mut tag_buf = [b' '; 5];
                    tag_buf[0] = b'[';
                    let r_bytes = role.as_bytes();
                    let r_len = r_bytes.len().min(2);
                    tag_buf[1..1 + r_len].copy_from_slice(&r_bytes[..r_len]);
                    tag_buf[1 + r_len] = b']';
                    let tag_str = core::str::from_utf8(&tag_buf[..2 + r_len]).unwrap_or("[--]");
                    Text::new(tag_str, Point::new(102, y + 7), style).draw(lcd).ok();
                }

                widgets::draw_footer(lcd, "[OK] Select   [ESC] Back");
            }
        }

        // --- State: Loading Parameters for Selected Device ---
        ElrsConfigState::Connected | ElrsConfigState::LoadingParam(_) => {
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
            Text::new("Loading parameters...", Point::new(4, 26), text_style)
                .draw(lcd)
                .ok();

            if let ElrsConfigState::LoadingParam(id) = engine.state {
                let mut pbuf = [b' '; 18];
                pbuf[..6].copy_from_slice(b"Param ");
                pbuf[6] = b'0' + ((id / 10) % 10);
                pbuf[7] = b'0' + (id % 10);
                pbuf[8..12].copy_from_slice(b" of ");
                pbuf[12] = b'0' + ((engine.param_count / 10) % 10);
                pbuf[13] = b'0' + (engine.param_count % 10);
                let p_str = core::str::from_utf8(&pbuf[..14]).unwrap_or("Loading...");
                Text::new(p_str, Point::new(4, 38), text_style_small)
                    .draw(lcd)
                    .ok();
            }

            widgets::draw_footer(lcd, "Please wait...  [ESC] Abort");
        }

        // --- State: Ready / Interactive Parameter Navigation ---
        ElrsConfigState::Ready => {
            let current_folder = ctrl.page_idx as u8;
            let mut folder_indices = [0usize; crsf::MAX_PARAMS];
            let count = crsf::get_folder_params(current_folder, &mut folder_indices);

            if count == 0 {
                if engine.params_len == 0 {
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
                        ctrl.page_idx = crsf::get_parent_folder(current_folder) as usize;
                        ctrl.selected_item = 0;
                        ctrl.scroll_offset = 0;
                        buzzer.click();
                        return;
                    }
                    let mut fbuf = [0u8; 16];
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
                        let actual_idx = folder_indices[ctrl.selected_item];
                        let p = &engine.params[actual_idx];

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
                            crsf::set_param_value(actual_idx, ctrl.sub_idx as u8);
                            ctrl.editing = false;
                            buzzer.play_tone(2400, 80);
                        } else if keys.cancel {
                            // Cancel edit: revert without transmitting
                            ctrl.editing = false;
                            buzzer.click();
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
                            let actual_idx = folder_indices[ctrl.selected_item];
                            let p = &engine.params[actual_idx];

                            if p.param_type == crsf::protocol::CRSF_TYPE_FOLDER {
                                // Drill down into subfolder
                                ctrl.page_idx = p.id as usize;
                                ctrl.selected_item = 0;
                                ctrl.scroll_offset = 0;
                                buzzer.click();
                            } else if p.param_type == crsf::protocol::CRSF_TYPE_SELECT {
                                // Enter modal edit mode on SELECT
                                ctrl.editing = true;
                                ctrl.sub_idx = p.value as usize;
                                buzzer.click();
                            } else if p.param_type == crsf::protocol::CRSF_TYPE_COMMAND {
                                crsf::trigger_command(actual_idx);
                                buzzer.play_tone(2400, 80);
                            }
                        } else if keys.cancel {
                            if current_folder > 0 {
                                // Step back to parent folder
                                ctrl.page_idx = crsf::get_parent_folder(current_folder) as usize;
                                ctrl.selected_item = 0;
                                ctrl.scroll_offset = 0;
                                buzzer.click();
                                return;
                            } else {
                                // Step back from root folder to Device List
                                crsf::return_to_device_list();
                                ctrl.selected_item = 0;
                                ctrl.scroll_offset = 0;
                                ctrl.editing = false;
                                buzzer.click();
                                return;
                            }
                        }
                    }
                }

                // Header title: subfolder name or device name
                if current_folder > 0 {
                    let mut fbuf = [0u8; 16];
                    let fname = crsf::get_folder_name(current_folder, &mut fbuf);
                    widgets::draw_header(lcd, fname);
                } else {
                    let dev_name = if engine.device_name_len > 0 {
                        core::str::from_utf8(
                            &engine.device_name[..engine.device_name_len as usize],
                        )
                        .unwrap_or("CRSF SETUP")
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
                    let actual_idx = folder_indices[slot_idx];
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
                    let name =
                        core::str::from_utf8(&p.name[..p.name_len as usize]).unwrap_or("Param");

                    if p.param_type == crsf::protocol::CRSF_TYPE_FOLDER {
                        // Folder row with trailing chevron
                        Text::new(name, Point::new(4, y + 7), style).draw(lcd).ok();
                        Text::new(">", Point::new(118, y + 7), style).draw(lcd).ok();
                    } else if p.param_type == crsf::protocol::CRSF_TYPE_SELECT {
                        Text::new(name, Point::new(4, y + 7), style).draw(lcd).ok();
                        let mut opt_buf = [0u8; 16];

                        if is_sel && ctrl.editing {
                            // Modal edit display: bracketed with '<' and '>'
                            let opt = p.option_str_for_val(ctrl.sub_idx as u8, &mut opt_buf);
                            let mut ed_buf = [b' '; 18];
                            ed_buf[0] = b'<';
                            let o_len = opt.len().min(14);
                            ed_buf[1..1 + o_len].copy_from_slice(&opt.as_bytes()[..o_len]);
                            ed_buf[1 + o_len] = b'>';
                            let ed_str =
                                core::str::from_utf8(&ed_buf[..2 + o_len]).unwrap_or(opt);
                            let x_pos = 122i32.saturating_sub((ed_str.len() as i32) * 6);
                            Text::new(ed_str, Point::new(x_pos.max(56), y + 7), style)
                                .draw(lcd)
                                .ok();
                        } else {
                            let opt = p.current_option_str(&mut opt_buf);
                            let x_pos = 122i32.saturating_sub((opt.len() as i32) * 6);
                            Text::new(opt, Point::new(x_pos.max(64), y + 7), style)
                                .draw(lcd)
                                .ok();
                        }
                    } else if p.param_type == crsf::protocol::CRSF_TYPE_COMMAND {
                        let is_running = match engine.active_cmd {
                            crsf::ActiveCommandState::Starting { param_id, .. }
                            | crsf::ActiveCommandState::Running { param_id, .. } => {
                                param_id == p.id
                            }
                            _ => false,
                        };
                        if is_running {
                            Text::new("[Executing...]", Point::new(14, y + 7), style)
                                .draw(lcd)
                                .ok();
                        } else {
                            let mut cmd_buf = [b' '; 20];
                            cmd_buf[0] = b'[';
                            let n_len = (p.name_len as usize).min(16);
                            cmd_buf[1..1 + n_len].copy_from_slice(&p.name[..n_len]);
                            cmd_buf[1 + n_len] = b']';
                            let c_str =
                                core::str::from_utf8(&cmd_buf[..2 + n_len]).unwrap_or("[Cmd]");
                            Text::new(c_str, Point::new(14, y + 7), style)
                                .draw(lcd)
                                .ok();
                        }
                    } else {
                        Text::new(name, Point::new(4, y + 7), style).draw(lcd).ok();
                    }
                }

                // If confirmation modal is needed, draw centered overlay box
                if let crsf::ActiveCommandState::WaitingConfirm { param_id } = engine.active_cmd {
                    let p_name = engine
                        .params
                        .iter()
                        .find(|p| p.id == param_id)
                        .map(|p| {
                            core::str::from_utf8(&p.name[..p.name_len as usize]).unwrap_or("Action")
                        })
                        .unwrap_or("Action");

                    Rectangle::new(Point::new(12, 18), Size::new(104, 28))
                        .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
                        .draw(lcd)
                        .ok();
                    Rectangle::new(Point::new(12, 18), Size::new(104, 28))
                        .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
                        .draw(lcd)
                        .ok();

                    let mut q_buf = [b' '; 18];
                    q_buf[..6].copy_from_slice(b"Run: [");
                    let n_len = p_name.len().min(8);
                    q_buf[6..6 + n_len].copy_from_slice(&p_name.as_bytes()[..n_len]);
                    q_buf[6 + n_len] = b']';
                    q_buf[7 + n_len] = b'?';
                    let q_str = core::str::from_utf8(&q_buf[..8 + n_len]).unwrap_or("Execute?");
                    Text::new(q_str, Point::new(18, 28), text_style)
                        .draw(lcd)
                        .ok();
                    Text::new("[OK] Yes  [ESC] No", Point::new(18, 40), text_style_small)
                        .draw(lcd)
                        .ok();

                    widgets::draw_footer(lcd, "[OK] Confirm   [ESC] Cancel");
                } else if ctrl.editing {
                    widgets::draw_footer(lcd, "[OK] Confirm   [ESC] Cancel");
                } else if current_folder > 0 {
                    widgets::draw_footer(lcd, "[OK] Select   [ESC] Up");
                } else {
                    widgets::draw_footer(lcd, "[OK] Select   [ESC] Devices");
                }
            }
        }
    }
}
