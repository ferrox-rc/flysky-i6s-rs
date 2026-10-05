#![no_std]
#![no_main]

use stm32f0xx_hal as _;

// Panic loop guard placed in .uninit section so RAM value survives warm resets
#[link_section = ".uninit"]
static mut PANIC_COUNT: u32 = 0;

const PANIC_MAGIC: u32 = 0x5041_4E00; // 'PAN\0'
const PANIC_MAX_CONSECUTIVE: u32 = 3;

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    // Disable interrupts to prevent ISR execution during fault handling
    cortex_m::interrupt::disable();

    unsafe {
        let val = core::ptr::read_volatile(core::ptr::addr_of!(PANIC_COUNT));
        let count = if (val & 0xFFFF_FF00) == PANIC_MAGIC {
            val & 0xFF
        } else {
            0
        };

        if count < PANIC_MAX_CONSECUTIVE {
            core::ptr::write_volatile(core::ptr::addr_of_mut!(PANIC_COUNT), PANIC_MAGIC | (count + 1));
            // Trigger rapid Cortex-M0 warm reset (<20ms recovery vs 2.0s watchdog)
            cortex_m::peripheral::SCB::sys_reset();
        }

        // Exceeded consecutive panic threshold: deterministic crash loop detected.
        // Halt and wait for hardware watchdog or power cycle rather than thrashing in an infinite loop.
        loop {
            cortex_m::asm::wfi();
        }
    }
}

use cortex_m_rt::entry;
use embedded_graphics::{
    mono_font::{ascii::FONT_4X6, ascii::FONT_6X10, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    text::Text,
};

use flysky_i6s_rs::{
    adc, boot, buzzer, calib, chip, crsf, curve, display, input, mixer, power, rf, storage, time, touch, trim,
    ui, usb, watchdog,
};
pub use ui::menu;

use display::St7567;

pub use mixer::SAFE_IDLE_CHANNELS;

/// High-rate flight pipeline state and outputs.
struct FlightSnapshot {
    state: input::InputState,
    rf_chs: [u16; mixer::NUM_CHANNELS],
    telem: rf::afhds2a::TelemetryData,
    is_binding: bool,
    is_armed: bool,
    just_armed: bool,
    just_disarmed: bool,
}

/// Flight pipeline context: manages stick sampling, mixing, and RF/USB publication.
struct FlightPipeline {
    prev_armed: bool,
    prev_active_model: u8,
    telem_reading_idx: u8,
}

impl FlightPipeline {
    fn new(storage: &storage::RadioStorage, init_state: &input::InputState) -> Self {
        let active = storage.active_model();
        let prev_armed = mixer::eval_arm_switch(active.arm_switch, &init_state.switches);
        Self {
            prev_armed,
            prev_active_model: storage.radio.active_model,
            telem_reading_idx: 1,
        }
    }

    /// High-rate flight pipeline execution tick (multi-kHz execution speed).
    /// Polls physical sticks, evaluates throttle curves, computes matrix mixer,
    /// checks arm status, and publishes channels to RF (AFHDS 2A / CRSF) and USB.
    #[inline(never)]
    fn tick(
        &mut self,
        now: u32,
        storage: &storage::RadioStorage,
        trims: &trim::TrimController,
        menu_active: bool,
        buzzer: &mut buzzer::Buzzer,
    ) -> FlightSnapshot {
        // 1. Poll continuous DMA analog and digital inputs (sub-microsecond)
        let state = input::poll();
        let active_model = storage.active_model();

        // 2. Resynchronize arm state when active model changes or when exiting settings menu
        if storage.radio.active_model != self.prev_active_model {
            self.prev_active_model = storage.radio.active_model;
            self.prev_armed = mixer::eval_arm_switch(active_model.arm_switch, &state.switches);
        }

        // 3. Check configured Arm Switch condition and play Armed/Disarmed chimes
        let mut just_armed = false;
        let mut just_disarmed = false;
        let is_armed = mixer::eval_arm_switch(active_model.arm_switch, &state.switches);
        if active_model.arm_switch > 0
            && active_model.arm_switch <= 10
            && is_armed != self.prev_armed
        {
            if is_armed {
                just_armed = true;
                if !menu_active {
                    buzzer.chime_armed();
                }
            } else {
                just_disarmed = true;
                if !menu_active {
                    buzzer.chime_disarmed();
                }
            }
            self.prev_armed = is_armed;
        }

        // 4. Evaluate active model throttle curve (normalized 0..MIXER_MAX)
        let thr_input =
            ((state.sticks.throttle + mixer::MIXER_MAX) / 2).clamp(0, mixer::MIXER_MAX) as u16;
        let thr_curved = curve::evaluate_curve(
            thr_input,
            active_model.thr_curve_pts,
            active_model.thr_curve_smooth != 0,
            &active_model.thr_curve,
        );

        // 5. Compute all 14 channels via 4-stage pipeline (D/R, Expo, Matrix Mixer, Trims, Reversing)
        let rf_chs = mixer::compute_channels(
            state.sticks.roll,
            state.sticks.pitch,
            thr_curved,
            state.sticks.yaw,
            &[state.pots.vr1, state.pots.vr2],
            &state.switches,
            active_model,
            trims,
            storage.radio.throttle_trim,
        );

        // 6. Publish channels to active RF subsystem
        let is_crsf = active_model.rf_protocol == 1;
        let sim_mode = usb::is_sim_mode();

        if is_crsf {
            let crsf_active = !sim_mode;
            crsf::set_enabled(crsf_active, active_model.crsf_baud);
            if crsf_active {
                crsf::update_channels(now, &rf_chs);
                crsf::poll_telemetry(now);
            }
            rf::set_silenced(true);
        } else {
            crsf::set_enabled(false, 0);
            rf::set_silenced(sim_mode);
            if !sim_mode {
                rf::set_channels(&rf_chs);
            }
        }

        // 7. Map telemetry data based on active protocol
        let telem = if is_crsf {
            let ct = crsf::get_telemetry();
            rf::afhds2a::TelemetryData {
                connected: ct.connected,
                rssi: ct.uplink_link_quality,
                rx_voltage_mv: ct.rx_battery_mv,
                packets_sent: 0,
                packets_received: 0,
            }
        } else {
            rf::get_telemetry_local(&mut self.telem_reading_idx)
        };
        let is_binding = !is_crsf && rf::is_binding();

        // 8. Poll USB subsystem (Joystick HID @ 100Hz, Serial CLI / Telemetry)
        usb::poll(now, &rf_chs, &state.switches, &telem, state.battery_mv);

        FlightSnapshot {
            state,
            rf_chs,
            telem,
            is_binding,
            is_armed,
            just_armed,
            just_disarmed,
        }
    }
}

/// Background idle and UI context: tracks inactivity, backlight timers, user input, and screen rendering.
struct BackgroundIdleManager {
    prev_stick_samples: [u16; 6],
    prev_pots: [i16; 2],
    prev_switches: input::Switches,
    prev_bind_key: bool,
    bind_hold_ms: u32,
    bind_was_held: bool,
    ok_hold_ms: u16,
    cancel_hold_ms: u16,
    timer_remaining_secs: u16,
    timer_elapsed_secs: u16,
    timer_ms_acc: u16,
    timer_latched: bool,
    timer_reset_toast_ms: u16,
    timer_reset_cooldown_ms: u16,
    cancel_waiting_release: bool,
    prev_timer_model: u8,
    prev_up_key: bool,
    prev_down_key: bool,
    bl_timer_ms: u32,
    inactivity_timer_ms: u32,
    inactivity_beep_timer: u32,
    last_display_ms: u32,
    menu_was_active: bool,
    pub power_manager: power::PowerManager,
    rx_id_dirty: bool,
    trim_dirty: bool,
    trim_save_cooldown_ms: u16,
}

impl BackgroundIdleManager {
    fn new(init_state: &input::InputState, bind_on_boot: bool) -> Self {
        Self {
            prev_stick_samples: [
                init_state.raw[0],
                init_state.raw[1],
                init_state.raw[2],
                init_state.raw[3],
                init_state.raw[6],
                init_state.raw[7],
            ],
            prev_pots: [init_state.pots.vr1, init_state.pots.vr2],
            prev_switches: init_state.switches,
            prev_bind_key: bind_on_boot,
            bind_hold_ms: 0,
            bind_was_held: false,
            ok_hold_ms: 0,
            cancel_hold_ms: 0,
            timer_remaining_secs: 0,
            timer_elapsed_secs: 0,
            timer_ms_acc: 0,
            timer_latched: false,
            timer_reset_toast_ms: 0,
            timer_reset_cooldown_ms: 0,
            cancel_waiting_release: false,
            prev_timer_model: 0xFF,
            prev_up_key: false,
            prev_down_key: false,
            bl_timer_ms: 30_000,
            inactivity_timer_ms: 0,
            inactivity_beep_timer: 0,
            last_display_ms: 0,
            menu_was_active: false,
            power_manager: power::PowerManager::new(),
            rx_id_dirty: false,
            trim_dirty: false,
            trim_save_cooldown_ms: 0,
        }
    }

    /// Background idle execution tick: manages timers, power save, bind key gestures, Flash saves, and display frames.
    #[inline(never)]
    #[allow(clippy::too_many_arguments)]
    fn tick(
        &mut self,
        now: u32,
        dt_ms: u16,
        keys: u16,
        flight: &FlightSnapshot,
        pipeline: &mut FlightPipeline,
        storage: &mut storage::RadioStorage,
        trims: &mut trim::TrimController,
        lcd: &mut St7567,
        buzzer: &mut buzzer::Buzzer,
        dashboard: &mut ui::dashboard::DashboardController,
        menu_controller: &mut menu::MenuController,
        calib_wizard: &mut calib::CalibWizard,
        rf_ok: bool,
    ) {
        let menu_active = menu_controller.is_active() || calib_wizard.is_active();
        self.power_manager.update(dt_ms, storage, buzzer);

        // 1. Resynchronize arm tracking when exiting settings menu
        if self.menu_was_active && !menu_active {
            let active = storage.active_model();
            pipeline.prev_armed = mixer::eval_arm_switch(active.arm_switch, &flight.state.switches);
        }
        self.menu_was_active = menu_active;

        // Pot Center Crossing Haptic/Audio Feedback (VR1 & VR2)
        // Detect transitions across deadband ±25 from outside (|prev| >= 25 && |curr| < 25)
        for (&curr, prev) in [flight.state.pots.vr1, flight.state.pots.vr2]
            .iter()
            .zip(self.prev_pots.iter_mut())
        {
            if prev.abs() >= 25 && curr.abs() < 25 {
                buzzer.pot_center_click();
            }
            *prev = curr;
        }

        // 2. Physical activity & inactivity tracking
        let stick_moved = (flight.state.raw[0] as i32 - self.prev_stick_samples[0] as i32).abs()
            > 30
            || (flight.state.raw[1] as i32 - self.prev_stick_samples[1] as i32).abs() > 30
            || (flight.state.raw[2] as i32 - self.prev_stick_samples[2] as i32).abs() > 30
            || (flight.state.raw[3] as i32 - self.prev_stick_samples[3] as i32).abs() > 30
            || (flight.state.raw[6] as i32 - self.prev_stick_samples[4] as i32).abs() > 40
            || (flight.state.raw[7] as i32 - self.prev_stick_samples[5] as i32).abs() > 40;
        self.prev_stick_samples[0] = flight.state.raw[0];
        self.prev_stick_samples[1] = flight.state.raw[1];
        self.prev_stick_samples[2] = flight.state.raw[2];
        self.prev_stick_samples[3] = flight.state.raw[3];
        self.prev_stick_samples[4] = flight.state.raw[6];
        self.prev_stick_samples[5] = flight.state.raw[7];

        let sw_changed = flight.state.switches != self.prev_switches;
        self.prev_switches = flight.state.switches;

        let user_active = keys != 0 || stick_moved || sw_changed;

        // 3. Backlight auto-dim timeout tracking
        if user_active {
            let timeout_ms: u32 = match storage.radio.backlight_timeout {
                1 => 15_000,
                2 => 30_000,
                3 => 60_000,
                _ => 0,
            };
            self.bl_timer_ms = timeout_ms;
            lcd.set_backlight_level(storage.radio.backlight_brightness * 10);
        } else if storage.radio.backlight_timeout != 0 && dt_ms > 0 {
            if self.bl_timer_ms > dt_ms as u32 {
                self.bl_timer_ms -= dt_ms as u32;
            } else {
                self.bl_timer_ms = 0;
                lcd.set_backlight_level(0);
            }
        }

        // 4. Radio Inactivity Alarm (10 minutes without physical control activity)
        if user_active {
            self.inactivity_timer_ms = 0;
            self.inactivity_beep_timer = 0;
        } else if dt_ms > 0 {
            self.inactivity_timer_ms = self.inactivity_timer_ms.saturating_add(dt_ms as u32);
            if self.inactivity_timer_ms >= 600_000 {
                if self.inactivity_beep_timer >= 30_000 {
                    self.inactivity_beep_timer = 0;
                    buzzer.warn_inactivity();
                } else {
                    self.inactivity_beep_timer += dt_ms as u32;
                }
            }
        }

        // 5. Keys handling (Bind, Cancel, Up, Down)
        let bind_key_raw = (keys & (1 << 12)) != 0;
        let bind_pressed = bind_key_raw && !self.prev_bind_key;
        self.prev_bind_key = bind_key_raw;

        let up_key_raw = (keys & (1 << 9)) != 0;
        let up_pressed = up_key_raw && !self.prev_up_key;
        self.prev_up_key = up_key_raw;

        let down_key_raw = (keys & (1 << 8)) != 0;
        let down_pressed = down_key_raw && !self.prev_down_key;
        self.prev_down_key = down_key_raw;

        let cancel_key = (keys & (1 << 11)) != 0;

        if flight.is_binding {
            if cancel_key || bind_pressed {
                rf::set_bind_mode(false);
                buzzer.click();
            }
        } else if menu_active {
            self.bind_hold_ms = 0;
            self.bind_was_held = false;
        } else {
            // Dashboard page cycling via UP/DOWN keys
            if up_pressed {
                dashboard.prev_page(buzzer);
            } else if down_pressed {
                dashboard.next_page(buzzer);
            }

            if bind_key_raw {
                self.bind_hold_ms = self.bind_hold_ms.saturating_add(dt_ms as u32);
                if self.bind_hold_ms >= 1000 && !self.bind_was_held {
                    rf::set_bind_mode(true);
                    buzzer.play_tone(2400, 150);
                    self.bind_was_held = true;
                }
            } else {
                if !self.bind_was_held && self.bind_hold_ms >= 40 {
                    dashboard.next_page(buzzer);
                }
                self.bind_hold_ms = 0;
                self.bind_was_held = false;
            }
        }

        // 6. Persist newly bound RX ID safely to Flash outside ISR (inhibit while armed)
        if let Some(new_rx_id) = rf::take_pending_rx_save() {
            if new_rx_id != 0
                && new_rx_id != 0xFFFF_FFFF
                && storage.active_model().rx_id != new_rx_id
            {
                storage.active_model_mut().rx_id = new_rx_id;
                self.rx_id_dirty = true;
            }
        }
        if self.rx_id_dirty && !pipeline.prev_armed && storage::save_active_model(storage) {
            self.rx_id_dirty = false;
            buzzer.play_tone_pattern(2400, 70, 50, 2);
        }

        // 7. Debounced auto-save of active model trims to Flash (inhibit while armed)
        let active_trims = [
            trims.values.roll,
            trims.values.pitch,
            trims.values.throttle,
            trims.values.yaw,
        ];
        if storage.active_model().trims != active_trims {
            storage.active_model_mut().trims = active_trims;
            self.trim_dirty = true;
            self.trim_save_cooldown_ms = 2000;
        }

        if self.trim_save_cooldown_ms > 0 {
            self.trim_save_cooldown_ms = self.trim_save_cooldown_ms.saturating_sub(dt_ms);
        }

        if self.trim_dirty
            && self.trim_save_cooldown_ms == 0
            && !pipeline.prev_armed
            && storage::save_active_model(storage)
        {
            self.trim_dirty = false;
        }

        // 8. Open Settings Menu from flight dashboard:
        // - Immediate open on Center Touch Tap (KEY_MENU_OPEN, bit 13)
        // - Hold OK (1.2s) from physical button or touch hold (bit 10)
        if !menu_active {
            if (keys & (1 << 13)) != 0 {
                menu_controller.open(buzzer);
                self.ok_hold_ms = 0;
            } else if (keys & (1 << 10)) != 0 {
                self.ok_hold_ms = self.ok_hold_ms.saturating_add(dt_ms);
                if self.ok_hold_ms >= 1200 {
                    menu_controller.open(buzzer);
                    self.ok_hold_ms = 0;
                }
            } else {
                self.ok_hold_ms = 0;
            }
        }

        // 8. Flight Timer Engine
        let active_idx = storage.radio.active_model;
        let model = storage.active_model();
        if self.prev_timer_model != active_idx {
            self.prev_timer_model = active_idx;
            self.timer_remaining_secs = model.timer_secs;
            self.timer_elapsed_secs = 0;
            self.timer_ms_acc = 0;
            self.timer_latched = false;
        }

        // Arm Switch transitions: auto-reset upon Arming, unlatch upon Disarming
        if model.arm_switch > 0 && model.arm_switch <= 10 {
            if flight.just_armed {
                self.timer_remaining_secs = model.timer_secs;
                self.timer_elapsed_secs = 0;
                self.timer_ms_acc = 0;
                self.timer_latched = false;
            } else if flight.just_disarmed {
                self.timer_latched = false;
            }
        }

        // Cooldown timer, toast duration, and key release guard for timer reset
        if self.timer_reset_toast_ms > 0 {
            self.timer_reset_toast_ms = self.timer_reset_toast_ms.saturating_sub(dt_ms);
        }
        if self.timer_reset_cooldown_ms > 0 {
            self.timer_reset_cooldown_ms = self.timer_reset_cooldown_ms.saturating_sub(dt_ms);
        }
        if !cancel_key {
            self.cancel_waiting_release = false;
        }

        // Check for manual reset via [CANCEL] held for >= 1.0s on flight dashboard
        if !menu_active
            && cancel_key
            && !self.cancel_waiting_release
            && self.timer_reset_cooldown_ms == 0
        {
            self.cancel_hold_ms = self.cancel_hold_ms.saturating_add(dt_ms);
            if self.cancel_hold_ms >= 1000 {
                self.timer_remaining_secs = model.timer_secs;
                self.timer_elapsed_secs = 0;
                self.timer_ms_acc = 0;
                self.timer_latched = false;
                self.cancel_hold_ms = 0;
                self.cancel_waiting_release = true;
                self.timer_reset_toast_ms = 800;
                self.timer_reset_cooldown_ms = 1500;
                buzzer.play_tone(2400, 100);
            }
        } else {
            self.cancel_hold_ms = 0;
        }

        // Evaluate timer trigger condition
        let is_armed_or_unassigned = if model.arm_switch > 0 && model.arm_switch <= 10 {
            flight.is_armed
        } else {
            true
        };
        let timer_running = mixer::is_timer_active(
            model.timer_source,
            flight.state.sticks.throttle,
            &mut self.timer_latched,
            &flight.state.switches,
            is_armed_or_unassigned,
        );

        if timer_running && dt_ms > 0 {
            self.timer_ms_acc = self.timer_ms_acc.saturating_add(dt_ms);
            while self.timer_ms_acc >= 1000 {
                self.timer_ms_acc -= 1000;
                if self.timer_remaining_secs > 0 {
                    self.timer_remaining_secs -= 1;
                    if self.timer_remaining_secs == 0 {
                        buzzer.timer_elapsed_alarm();
                    } else if self.timer_remaining_secs <= 10 {
                        buzzer.timer_countdown_beep();
                    } else if self.timer_remaining_secs.is_multiple_of(60) {
                        buzzer.timer_minute_beep();
                    }
                } else {
                    self.timer_elapsed_secs = self.timer_elapsed_secs.saturating_add(1);
                }
            }
        }

        // 9. Display Frame Rendering (~30 Hz)
        let run_display = now.wrapping_sub(self.last_display_ms) >= 33;
        if !run_display {
            return;
        }
        self.last_display_ms = now;

        self.render_display_frame(
            lcd,
            keys,
            dt_ms,
            flight,
            storage,
            trims,
            buzzer,
            dashboard,
            menu_controller,
            calib_wizard,
            rf_ok,
        );
    }

    #[inline(never)]
    #[allow(clippy::too_many_arguments)]
    fn render_display_frame(
        &mut self,
        lcd: &mut St7567,
        keys: u16,
        dt_ms: u16,
        flight: &FlightSnapshot,
        storage: &mut storage::RadioStorage,
        trims: &mut trim::TrimController,
        buzzer: &mut buzzer::Buzzer,
        dashboard: &mut ui::dashboard::DashboardController,
        menu_controller: &mut menu::MenuController,
        calib_wizard: &mut calib::CalibWizard,
        rf_ok: bool,
    ) {
        if menu_controller.is_active() {
            menu_controller.update(
                lcd,
                keys,
                &flight.state.switches,
                storage,
                trims,
                &flight.state.raw,
                &flight.rf_chs,
                buzzer,
            );

            if menu_controller.request_calibration {
                calib_wizard.start(buzzer);
                menu_controller.request_calibration = false;
            }

            if menu_controller.request_bind {
                rf::set_bind_mode(true);
                menu_controller.request_bind = false;
                buzzer.play_tone(2400, 150);
            }

            lcd.flush();
        } else if calib_wizard.is_active() {
            calib_wizard.update(lcd, storage, &flight.state.raw, keys, dt_ms.max(20), buzzer);
            lcd.flush();
        } else {
            let model = storage.active_model();
            let mut timer_buf = [0u8; 8];
            let timer_display = if model.timer_source != 0 {
                let (disp_secs, expired) = if model.timer_secs > 0 {
                    if self.timer_remaining_secs > 0 {
                        (self.timer_remaining_secs, false)
                    } else {
                        (self.timer_elapsed_secs, true)
                    }
                } else {
                    (self.timer_elapsed_secs, false)
                };
                let formatted = ui::format::format_timer(disp_secs, expired, &mut timer_buf);
                Some((formatted, expired))
            } else {
                None
            };

            let (timer_str, timer_expired) = match timer_display {
                Some((s, exp)) => (Some(s), exp),
                None => (None, false),
            };

            dashboard.render(
                lcd,
                &flight.state,
                storage,
                trims,
                &flight.rf_chs,
                rf_ok,
                flight.is_binding,
                &flight.telem,
                timer_str,
                timer_expired,
                buzzer,
            );

            if self.timer_reset_toast_ms > 0 {
                draw_timer_reset_modal(lcd, 100, true);
            } else if self.cancel_hold_ms > 80 {
                let pct = ((self.cancel_hold_ms as u32 * 100) / 1000).min(100) as u8;
                draw_timer_reset_modal(lcd, pct, false);
            }
        }

        if self.power_manager.shutdown_progress_pct() > 0 {
            draw_shutdown_modal(lcd, self.power_manager.shutdown_progress_pct());
        }

        lcd.flush();
    }
}

/// Pre-flight safety check: verifies throttle at idle and switches in safe UP positions.
#[inline(never)]
fn run_preflight_check(
    storage: &storage::RadioStorage,
    buzzer: &mut buzzer::Buzzer,
    lcd: &mut St7567,
    calib_wizard: &calib::CalibWizard,
    was_watchdog_reset: bool,
) {
    if calib_wizard.is_active() || was_watchdog_reset {
        return;
    }

    let text_style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
    let text_style_small = MonoTextStyle::new(&FONT_4X6, BinaryColor::On);

    let mut preflight_beep_timer: u32 = 0;
    let mut preflight_last_render: u32 = 0;
    let mut warned = false;

    loop {
        watchdog::feed();

        let now = time::millis();
        let state = input::poll();
        let keys = boot::scan_keys();

        let is_general = storage.active_model().model_type == 4;
        let is_calibrated = storage.radio.sticks[2].min > 200;
        let thr_unsafe = if is_general {
            false
        } else if is_calibrated {
            state.sticks.throttle > -900
        } else {
            state.raw[2] > 1400
        };

        let sa_unsafe = state.switches.sa != input::SwitchPos::Up;
        let sb_unsafe = state.switches.sb != input::SwitchPos::Up;
        let sc_unsafe = state.switches.sc != input::SwitchPos::Up;
        let sd_unsafe = state.switches.sd != input::SwitchPos::Up;
        let sw_unsafe = sa_unsafe || sb_unsafe || sc_unsafe || sd_unsafe;

        let cancel_pressed = (keys & (1 << 11)) != 0;

        if (!thr_unsafe && !sw_unsafe) || cancel_pressed {
            if warned {
                buzzer.play_tone(2200, 40);
            }
            break;
        }
        warned = true;

        // Lock RF transmission to safe idle/failsafe during warning
        rf::set_channels(&SAFE_IDLE_CHANNELS);
        usb::poll(
            now,
            &SAFE_IDLE_CHANNELS,
            &state.switches,
            &rf::get_telemetry(),
            state.battery_mv,
        );

        if now.wrapping_sub(preflight_beep_timer) >= 800 {
            preflight_beep_timer = now;
            buzzer.warn_preflight();
        }

        if now.wrapping_sub(preflight_last_render) >= 33 {
            let dt = (now.wrapping_sub(preflight_last_render)).min(100) as u16;
            preflight_last_render = now;
            buzzer.tick(dt);

            lcd.clear(BinaryColor::Off).ok();
            Text::new("SAFETY WARNING!", Point::new(16, 9), text_style)
                .draw(lcd)
                .ok();
            lcd.draw_hline(0, 11, 128, true);

            if thr_unsafe {
                Text::new("THROTTLE NOT AT IDLE!", Point::new(2, 23), text_style)
                    .draw(lcd)
                    .ok();
            }

            if sw_unsafe {
                Text::new("SWITCH WARNING:", Point::new(2, 34), text_style)
                    .draw(lcd)
                    .ok();
                let mut sw_warn = [b' '; 20];
                let mut col = 0;
                for &(unsafe_flag, label) in &[
                    (sa_unsafe, b"[SA]"),
                    (sb_unsafe, b"[SB]"),
                    (sc_unsafe, b"[SC]"),
                    (sd_unsafe, b"[SD]"),
                ] {
                    if unsafe_flag && col + 4 <= sw_warn.len() {
                        sw_warn[col..col + 4].copy_from_slice(label);
                        col += 4;
                        if col < sw_warn.len() {
                            sw_warn[col] = b' ';
                            col += 1;
                        }
                    }
                }
                let sw_str =
                    ui::format::ascii_as_str(&sw_warn[..col.saturating_sub(1).min(16)]);
                Text::new(sw_str, Point::new(2, 44), text_style)
                    .draw(lcd)
                    .ok();
            }

            lcd.draw_hline(0, 55, 128, true);
            Text::new(
                "Lower Thr/Safe SW  [ESC]Skip",
                Point::new(2, 62),
                text_style_small,
            )
            .draw(lcd)
            .ok();
            lcd.flush();
        }
    }
}

/// Draw centered modal overlay for timer reset hold progress or completion confirmation.
fn draw_timer_reset_modal(lcd: &mut St7567, progress_pct: u8, completed: bool) {
    lcd.fill_rect(18, 19, 92, 26, false);
    lcd.draw_rect(18, 19, 92, 26, true);

    let text_style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);

    if completed {
        Text::new("TIMER RESET!", Point::new(28, 35), text_style)
            .draw(lcd)
            .ok();
    } else {
        Text::new("RESET TIMER", Point::new(31, 29), text_style)
            .draw(lcd)
            .ok();

        // Progress bar container: 74 x 6, positioned at (27, 33)
        lcd.draw_rect(27, 33, 74, 6, true);

        // Progress fill: up to 70px wide
        let fill_w = ((progress_pct as u32 * 70) / 100).min(70);
        if fill_w > 0 {
            lcd.fill_rect(29, 35, fill_w, 2, true);
        }
    }
}

/// Draw centered modal overlay for power button shutdown hold progress.
fn draw_shutdown_modal(lcd: &mut St7567, progress_pct: u8) {
    lcd.fill_rect(18, 19, 92, 26, false);
    lcd.draw_rect(18, 19, 92, 26, true);

    let text_style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
    Text::new("SHUTDOWN...", Point::new(28, 29), text_style)
        .draw(lcd)
        .ok();

    // Progress bar container: 74 x 6, positioned at (27, 33)
    lcd.draw_rect(27, 33, 74, 6, true);

    // Progress fill: up to 70px wide
    let fill_w = ((progress_pct as u32 * 70) / 100).min(70);
    if fill_w > 0 {
        lcd.fill_rect(29, 35, fill_w, 2, true);
    }
}

#[entry]
fn main() -> ! {
    // 0. Electronic power latch: MUST be the first instruction to hold power rail ON!
    power::init();

    // Standalone feed immediately refreshes any watchdog running across soft reboot
    watchdog::feed();

    // Check if reboot was triggered by hardware watchdog (IWDG or WWDG) and clear RCC flags.
    // Under DO-178C flight safety principles, a warm reboot caused by watchdog reset must bypass
    // the throttle-at-idle and switch interlocks to restore flight control in <10ms.
    let was_watchdog_reset = chip::check_and_clear_reset_flags();

    // 1. MCU Profile & Fast DFU Bootloader Check
    let mcu_profile = chip::get_mcu_profile();
    if !was_watchdog_reset {
        boot::check_dfu_entry(&mcu_profile);
    } else {
        boot::init_keys();
    }

    // 2. Initialize System Clock to 48 MHz using external 8 MHz crystal (HSE) + PLL
    // and wait for 40 kHz LSI oscillator to stabilize
    chip::init_system_clock();

    // Initialize SysTick 1.000 ms hardware monotonic timekeeper
    time::init();

    // Initialize FT6236 Capacitive Touchscreen on I2C1 (PB8 SCL, PB9 SDA @ 400kHz, PA15 RST, PC12 INT)
    touch::ft6236::init();

    // 3. Load persistent radio storage and 20-model configuration
    let storage = storage::get_storage();
    storage::load_storage_into(storage);

    // 4. Initialize Buzzer & Tone Style
    let mut buzzer = buzzer::Buzzer::new();
    buzzer.init();
    buzzer.enabled = storage.radio.audio_enabled != 0;
    buzzer.tone_style = buzzer::ToneStyle::from_u8(storage.radio.tone_style);

    // 5. Initialize ST7567 128×64 LCD (clears framebuffer and flushes before turning on backlight)
    let mut lcd = St7567::new();
    lcd.set_backlight_level(storage.radio.backlight_brightness * 10);
    lcd.set_contrast(storage.radio.lcd_contrast);

    // Render Ferrox-RC Splash Screen and trigger welcome chime simultaneously (bypassed on warm watchdog recovery)
    let splash_start_ms = time::millis();
    let mut buzzer_last_ms = splash_start_ms;
    if was_watchdog_reset {
        // Fast alarm warning chirp indicating watchdog recovery occurred
        buzzer.play_tone_pattern(2600, 60, 40, 3);
    } else {
        ui::splash::draw_splash(&mut lcd);
        lcd.flush();
        buzzer.chime_welcome();
    }

    // 6. Start deterministic 2.0s Hardware Watchdog (IWDG) via PAC
    watchdog::start();

    // 7. Initialize ADC1 + DMA1 autonomous continuous scanner
    adc::init();
    watchdog::feed();
    {
        let now = time::millis();
        let dt = now.wrapping_sub(buzzer_last_ms) as u16;
        if dt > 0 {
            buzzer.tick(dt);
            buzzer_last_ms = now;
        }
    }

    // 8. Initialize input calibration and capture resting stick centers
    input::init();
    input::apply_calibration(&storage.radio);
    watchdog::feed();
    {
        let now = time::millis();
        let dt = now.wrapping_sub(buzzer_last_ms) as u16;
        if dt > 0 {
            buzzer.tick(dt);
            buzzer_last_ms = now;
        }
    }

    // 9. Initialize A7105 RF transceiver & AFHDS 2A stack
    let uid = chip::read_uid(&mcu_profile);
    let w0 = u32::from_le_bytes([uid[0], uid[1], uid[2], uid[3]]);
    let w1 = u32::from_le_bytes([uid[4], uid[5], uid[6], uid[7]]);
    let w2 = u32::from_le_bytes([uid[8], uid[9], uid[10], uid[11]]);
    let tx_id = w0 ^ w1 ^ w2;

    let initial_keys = boot::scan_keys();
    let bind_on_boot = (initial_keys & (1 << 12)) != 0 && !was_watchdog_reset;

    let rf_ok = rf::init(tx_id);
    if bind_on_boot {
        rf::set_bind_mode(true);
    }
    watchdog::feed();
    {
        let now = time::millis();
        let dt = now.wrapping_sub(buzzer_last_ms) as u16;
        if dt > 0 {
            buzzer.tick(dt);
            buzzer_last_ms = now;
        }
    }

    // Initialize USB peripheral (Joystick / Serial / Composite / Off)
    usb::init(storage.radio.usb_mode);

    // Initialize CRSF / ExpressLRS expansion bay peripheral (USART2 & PC13)
    crsf::init(storage.radio.ext_module_pwr == 0);

    let mut trims = trim::TrimController::new();
    trims.throttle_enabled = storage.radio.throttle_trim != 0;

    // Load active model trims and receiver ID
    let active = storage.active_model();
    trims.values.roll = active.trims[0];
    trims.values.pitch = active.trims[1];
    trims.values.throttle = active.trims[2];
    trims.values.yaw = active.trims[3];
    rf::set_rx_id(active.rx_id);
    rf::set_rx_settings(
        active.servo_rate_hz,
        active.rx_out_mode,
        active.rx_serial_proto,
    );

    let mut calib_wizard = calib::CalibWizard::new();
    let mut menu_controller = menu::MenuController::new();

    // Check if OK button held at power-on to launch calibration directly
    if (initial_keys & (1 << 10)) != 0 && !was_watchdog_reset {
        calib_wizard.start(&mut buzzer);
    }

    // 10. Hold splash screen for ~1200ms (1.2s) total elapsed time since LCD initialization,
    // continuously advancing buzzer playback to maintain musical cadence
    if !was_watchdog_reset && !calib_wizard.is_active() {
        while time::millis().wrapping_sub(splash_start_ms) < 1200 {
            watchdog::feed();
            let now = time::millis();
            let dt = now.wrapping_sub(buzzer_last_ms) as u16;
            if dt >= 5 {
                buzzer.tick(dt);
                buzzer_last_ms = now;
            }
        }
    }

    // 11. Pre-flight Startup Safety Check: Throttle at idle and switches in safe (UP) positions
    // Bypassed on watchdog reset to immediately resume RF control in flight
    run_preflight_check(storage, &mut buzzer, &mut lcd, &calib_wizard, was_watchdog_reset);

    // Initialize execution tiers
    let init_state = input::poll();
    let mut pipeline = FlightPipeline::new(storage, &init_state);
    let mut idle_manager = BackgroundIdleManager::new(&init_state, bind_on_boot);
    let mut dashboard = ui::dashboard::DashboardController::new();
    let mut last_tick_ms: u32 = 0;

    // Main event loop: decoupled into high-rate flight pipeline and 30 Hz background idle tasks
    loop {
        // Feed watchdog at loop start to guarantee active execution
        watchdog::feed();

        let now = time::millis();
        let dt_ms = (now.wrapping_sub(last_tick_ms)).min(100) as u16;

        let menu_active = menu_controller.is_active() || calib_wizard.is_active();

        // Poll touchscreen when interrupt asserted
        let touch_sample = if touch::ft6236::is_touch_int_asserted() {
            touch::ft6236::read_touch()
        } else {
            None
        };
        let current_sticks = input::poll().sticks;
        let keys = touch::nav::update_inputs(touch_sample.as_ref(), &current_sticks, menu_active);

        if dt_ms > 0 {
            last_tick_ms = now;
            buzzer.tick(dt_ms);
            trims.update(keys, dt_ms, &mut buzzer);

            // Once the transmitter has been running steadily in the flight loop for >5 seconds,
            // clear the consecutive panic count.
            if now > 5000 {
                unsafe {
                    if core::ptr::read_volatile(core::ptr::addr_of!(PANIC_COUNT)) != 0 {
                        core::ptr::write_volatile(core::ptr::addr_of_mut!(PANIC_COUNT), 0);
                    }
                }
            }
        }

        // Tier 1: High-Rate Flight Pipeline Tick (multi-kHz)
        let flight_snapshot = pipeline.tick(now, storage, &trims, menu_active, &mut buzzer);

        // Tier 2: Background Idle & UI Tick (30 Hz rate-governed)
        idle_manager.tick(
            now,
            dt_ms,
            keys,
            &flight_snapshot,
            &mut pipeline,
            storage,
            &mut trims,
            &mut lcd,
            &mut buzzer,
            &mut dashboard,
            &mut menu_controller,
            &mut calib_wizard,
            rf_ok,
        );
    }
}
