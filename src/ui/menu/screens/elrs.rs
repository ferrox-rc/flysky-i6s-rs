//! Native ExpressLRS / TBS CRSF configuration menu screen.

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

pub fn update(ctrl: &mut MenuController, lcd: &mut St7567, keys: &NavKeys, buzzer: &mut Buzzer) {
    let engine = crsf::get_config_engine();

    // Intercept keys if a confirmation dialog is active
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
    } else if keys.cancel {
        if ctrl.return_state == MenuState::MainMenu {
            ctrl.state = MenuState::MainMenu;
            ctrl.selected_item = 8;
            ctrl.scroll_offset = 5;
        } else {
            ctrl.state = MenuState::RxSetup;
            ctrl.selected_item = 2;
        }
        ctrl.waiting_release = true;
        buzzer.click();
        return;
    }

    let text_style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
    let text_style_small = MonoTextStyle::new(&FONT_4X6, BinaryColor::On);
    let fill_style = PrimitiveStyle::with_fill(BinaryColor::On);

    match engine.state {
        ElrsConfigState::Idle | ElrsConfigState::Discovering => {
            if keys.ok {
                ctrl.waiting_release = true;
                crsf::start_config();
                buzzer.click();
            }

            widgets::draw_header(lcd, "ELRS CONFIG");
            Text::new("Connecting to module...", Point::new(4, 26), text_style)
                .draw(lcd)
                .ok();
            Text::new(
                "Waiting for ping response",
                Point::new(4, 38),
                text_style_small,
            )
            .draw(lcd)
            .ok();

            widgets::draw_footer(lcd, "[OK] Retry   [ESC] Back");
        }
        ElrsConfigState::Connected | ElrsConfigState::LoadingParam(_) => {
            widgets::draw_header(lcd, "ELRS CONFIG");
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

            widgets::draw_footer(lcd, "Please wait...  [ESC] Back");
        }
        ElrsConfigState::Ready => {
            let count = engine.params_len;
            if count == 0 {
                if keys.ok {
                    ctrl.waiting_release = true;
                    crsf::start_config();
                    buzzer.click();
                }
                widgets::draw_header(lcd, "ELRS CONFIG");
                Text::new("No parameters found", Point::new(8, 28), text_style)
                    .draw(lcd)
                    .ok();
                widgets::draw_footer(lcd, "[OK] Retry   [ESC] Back");
            } else {
                let in_confirm = matches!(
                    engine.active_cmd,
                    crsf::ActiveCommandState::WaitingConfirm { .. }
                );

                if !in_confirm {
                    widgets::navigate_4slot_list(
                        &mut ctrl.selected_item,
                        &mut ctrl.scroll_offset,
                        count,
                        keys.up,
                        keys.down,
                        buzzer,
                    );

                    if keys.ok {
                        let p = &engine.params[ctrl.selected_item];
                        if p.param_type == crsf::protocol::CRSF_TYPE_SELECT {
                            crsf::cycle_param(ctrl.selected_item);
                            buzzer.click();
                        } else if p.param_type == crsf::protocol::CRSF_TYPE_COMMAND {
                            crsf::trigger_command(ctrl.selected_item);
                            buzzer.play_tone(2400, 80);
                        }
                    }
                }

                // Header: device name if available
                let dev_name = if engine.device_name_len > 0 {
                    core::str::from_utf8(&engine.device_name[..engine.device_name_len as usize])
                        .unwrap_or("ELRS SETUP")
                } else {
                    "ELRS SETUP"
                };
                widgets::draw_header(lcd, dev_name);

                // Render up to 4 parameters
                for slot in 0..4 {
                    let idx = ctrl.scroll_offset + slot;
                    if idx >= count {
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

                    let p = &engine.params[idx];
                    let name =
                        core::str::from_utf8(&p.name[..p.name_len as usize]).unwrap_or("Param");

                    if p.param_type == crsf::protocol::CRSF_TYPE_SELECT {
                        Text::new(name, Point::new(4, y + 7), style).draw(lcd).ok();
                        let mut opt_buf = [0u8; 16];
                        let opt = p.current_option_str(&mut opt_buf);
                        Text::new(opt, Point::new(72, y + 7), style).draw(lcd).ok();
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
                } else {
                    widgets::draw_footer(lcd, "[OK] Cycle/Run  [ESC] Back");
                }
            }
        }
    }
}
