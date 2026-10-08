//! Touch Navigation and Digital Trim Adapter for FlySky FS-i6S.
//!
//! Bridges the FT6236 capacitive touchscreen and front tactile buttons (`PA9`/`PA10`)
//! into standard `NavKeys` and `boot::scan_keys()` bitfields:
//!
//! 1. Navigation:
//!    - Swipe UP / Upper Tap Zone   -> `Up` (bit 9)
//!    - Swipe DOWN / Lower Tap Zone -> `Down` (bit 8)
//!    - Swipe RIGHT / Right Tap     -> `OK` (bit 10)
//!    - Swipe LEFT / Left Tap       -> `Cancel` (bit 11)
//!    - Right Front Button (`PA10`) -> `OK` (bit 10)
//!    - Left Front Button (`PA9`)   -> `Cancel` (bit 11)
//!
//! 2. Digital Trims (Replacing FS-i6X mechanical rockers):
//!    - **Modifier Mode**: Hold Left Front (`PA9`) + Left Stick deflection adjusts Yaw / Throttle trims;
//!      hold Right Front (`PA10`) + Right Stick deflection adjusts Roll / Pitch trims.
//!    - **Virtual Touch Targets**: Perimeter taps on the flight dashboard directly nudge trim axes.

use crate::boot;
use crate::input::Sticks;
use crate::touch::ft6236::{Gesture, TouchEventKind, TouchSample};
#[cfg(test)]
use crate::touch::ft6236::TouchPoint;

/// Stick deflection threshold required to engage modifier trim mode.
pub const TRIM_MODIFIER_STICK_THRESHOLD: i16 = 350;

/// Bitfield constants matching `boot::scan_keys()`
pub const KEY_TRIM_ROLL_R: u16 = 1 << 0;
pub const KEY_TRIM_ROLL_L: u16 = 1 << 1;
pub const KEY_TRIM_PITCH_U: u16 = 1 << 2;
pub const KEY_TRIM_PITCH_D: u16 = 1 << 3;
pub const KEY_TRIM_THR_U: u16 = 1 << 4;
pub const KEY_TRIM_THR_D: u16 = 1 << 5;
pub const KEY_TRIM_YAW_R: u16 = 1 << 6;
pub const KEY_TRIM_YAW_L: u16 = 1 << 7;
pub const KEY_NAV_DOWN: u16 = 1 << 8;
pub const KEY_NAV_UP: u16 = 1 << 9;
pub const KEY_NAV_OK: u16 = 1 << 10;
pub const KEY_NAV_CANCEL: u16 = 1 << 11;
pub const KEY_BIND: u16 = 1 << 12;
pub const KEY_MENU_OPEN: u16 = 1 << 13;

/// Evaluates a valid touch sample and screen tap zones into UI navigation key flags.
/// Software drag displacement tracking state across consecutive touch interrupt frames.
#[derive(Copy, Clone, Debug, Default)]
pub struct SwipeTracker {
    pub start_x: u8,
    pub start_y: u8,
    pub last_x: u8,
    pub last_y: u8,
    pub anchor_y: u8,
    pub max_y: u8,
    pub min_y: u8,
    pub scroll_dir: i8, // 0 = neutral, 1 = down, -1 = up
    pub active: bool,
    pub swiped: bool,
    pub last_packet_ms: u32,
    pub last_scroll_ms: u32,
}

impl SwipeTracker {
    #[inline(always)]
    pub fn reset_at(&mut self, x: u8, y: u8) {
        self.start_x = x;
        self.start_y = y;
        self.last_x = x;
        self.last_y = y;
        self.anchor_y = y;
        self.max_y = y;
        self.min_y = y;
        self.scroll_dir = 0;
        self.active = true;
        self.swiped = false;
        self.last_scroll_ms = 0;
    }

    #[inline(always)]
    pub fn clear(&mut self) {
        self.active = false;
        self.swiped = false;
        self.scroll_dir = 0;
        self.last_scroll_ms = 0;
    }
}

static mut GLOBAL_SWIPE_TRACKER: SwipeTracker = SwipeTracker {
    start_x: 0,
    start_y: 0,
    last_x: 0,
    last_y: 0,
    anchor_y: 0,
    max_y: 0,
    min_y: 0,
    scroll_dir: 0,
    active: false,
    swiped: false,
    last_packet_ms: 0,
    last_scroll_ms: 0,
};

/// Evaluates a valid touch sample and screen tap zones into UI navigation key flags.
///
/// Only invoked when a touch interrupt event (pulse on PC12) has occurred and a valid
/// `TouchSample` was read from the FT6236 controller.
pub fn touch_to_nav_keys(sample: &TouchSample, menu_active: bool) -> u16 {
    let tracker = unsafe { &mut *core::ptr::addr_of_mut!(GLOBAL_SWIPE_TRACKER) };
    #[cfg(not(test))]
    {
        tracker.last_packet_ms = crate::time::millis();
    }
    touch_to_nav_keys_with_tracker(sample, menu_active, tracker)
}

/// Evaluates touch sample with an explicit swipe tracker instance (testable without globals).
pub fn touch_to_nav_keys_with_tracker(
    sample: &TouchSample,
    menu_active: bool,
    tracker: &mut SwipeTracker,
) -> u16 {
    let mut keys = 0u16;

    // 1. Hardware Gesture Register Decoding (if provided by controller)
    match sample.gesture {
        Gesture::SwipeUp => keys |= KEY_NAV_UP,
        Gesture::SwipeDown => keys |= KEY_NAV_DOWN,
        Gesture::SwipeRight => keys |= KEY_NAV_OK,
        Gesture::SwipeLeft => keys |= KEY_NAV_CANCEL,
        Gesture::None => {}
    }

    if let Some(pt) = sample.point {
        match pt.event {
            TouchEventKind::PressDown => {
                tracker.reset_at(pt.x, pt.y);
            }
            TouchEventKind::Contact => {
                if !tracker.active {
                    tracker.reset_at(pt.x, pt.y);
                }

                if menu_active {
                    // Vertical displacement strictly scrolls list UP / DOWN
                    // Horizontal displacement is intentionally ignored so swiping never accidentally selects or cancels!
                    #[cfg(not(test))]
                    let now = crate::time::millis();
                    #[cfg(test)]
                    let now: u32 = 0;

                    // 1. Peak & Turning point tracking for instant directional reversal
                    if pt.y > tracker.max_y {
                        tracker.max_y = pt.y;
                    }
                    if pt.y < tracker.min_y {
                        tracker.min_y = pt.y;
                    }

                    // Deliberate direction reversal detection (>= 3px movement opposite to prior motion):
                    // If moving/scrolled DOWN and finger moves UP:
                    if tracker.scroll_dir >= 0 && pt.y <= tracker.max_y.saturating_sub(3) {
                        tracker.anchor_y = tracker.max_y;
                        tracker.min_y = pt.y;
                        tracker.scroll_dir = 0;
                    }
                    // If moving/scrolled UP and finger moves DOWN:
                    else if tracker.scroll_dir <= 0 && pt.y >= tracker.min_y.saturating_add(3) {
                        tracker.anchor_y = tracker.min_y;
                        tracker.max_y = pt.y;
                        tracker.scroll_dir = 0;
                    }

                    let dy = pt.y as i16 - tracker.anchor_y as i16;
                    let elapsed = if tracker.last_scroll_ms == 0 {
                        u32::MAX
                    } else {
                        now.wrapping_sub(tracker.last_scroll_ms)
                    };

                    // Threshold: 12 pixels (~19% of screen height)
                    // Pacing: repeat in same direction requires >= 200 ms (smooth 5 items/sec).
                    // Direction reversal or fresh step fires IMMEDIATELY on first frame!
                    if dy <= -12 {
                        let can_scroll = tracker.scroll_dir != -1 || elapsed >= 200;
                        if can_scroll {
                            keys |= KEY_NAV_UP;
                            tracker.anchor_y = pt.y;
                            tracker.max_y = pt.y;
                            tracker.min_y = pt.y;
                            tracker.scroll_dir = -1;
                            tracker.last_scroll_ms = now.max(1);
                            tracker.swiped = true;
                        }
                    } else if dy >= 12 {
                        let can_scroll = tracker.scroll_dir != 1 || elapsed >= 200;
                        if can_scroll {
                            keys |= KEY_NAV_DOWN;
                            tracker.anchor_y = pt.y;
                            tracker.max_y = pt.y;
                            tracker.min_y = pt.y;
                            tracker.scroll_dir = 1;
                            tracker.last_scroll_ms = now.max(1);
                            tracker.swiped = true;
                        }
                    }
                } else {
                    // Flight Dashboard mode:
                    // Center hold area: (28..=100, 16..=48)
                    let started_in_center = (28..=100).contains(&tracker.start_x)
                        && (16..=48).contains(&tracker.start_y);

                    let dx = pt.x as i16 - tracker.start_x as i16;
                    let dy = pt.y as i16 - tracker.start_y as i16;

                    if started_in_center {
                        // Deliberate swipe to cycle dashboard page:
                        // Horizontal swipe (left/right >= 16px) or vertical swipe (up/down >= 16px)
                        if dx <= -16 || dy >= 16 {
                            if !tracker.swiped {
                                keys |= KEY_NAV_DOWN;
                                tracker.swiped = true;
                            }
                        } else if dx >= 16 || dy <= -16 {
                            if !tracker.swiped {
                                keys |= KEY_NAV_UP;
                                tracker.swiped = true;
                            }
                        } else {
                            // Still holding inside center hold area -> accumulate for menu open
                            keys |= KEY_MENU_OPEN;
                        }
                    } else {
                        // Swipes starting outside center area cycle dashboard pages
                        if !tracker.swiped {
                            if dx <= -14 || dy >= 14 {
                                keys |= KEY_NAV_DOWN;
                                tracker.swiped = true;
                            } else if dx >= 14 || dy <= -14 {
                                keys |= KEY_NAV_UP;
                                tracker.swiped = true;
                            }
                        }
                    }
                }

                tracker.last_x = pt.x;
                tracker.last_y = pt.y;
            }
            TouchEventKind::LiftUp => {
                // If finger lifted without having triggered a swipe gesture, process as a tap
                if tracker.active && !tracker.swiped {
                    let tap_x = tracker.start_x;
                    let tap_y = tracker.start_y;
                    keys |= evaluate_tap_zones(tap_x, tap_y, menu_active);
                }
                tracker.clear();
            }
            TouchEventKind::NoEvent => {}
        }

        // On flight dashboard only: virtual trims on screen edges evaluate immediately during press/hold
        if !menu_active && !tracker.swiped && keys == 0 {
            if pt.x < 16 || pt.x > 112 || pt.y > 52 {
                keys |= evaluate_tap_zones(pt.x, pt.y, false);
            } else if (28..=100).contains(&pt.x) && (16..=48).contains(&pt.y) {
                // Continuous center hold emits KEY_MENU_OPEN to accumulate towards the 1.2s menu open requirement
                keys |= KEY_MENU_OPEN;
            }
        }
    } else {
        // Touch point is None: finger released / lifted
        if tracker.active && !tracker.swiped {
            keys |= evaluate_tap_zones(tracker.start_x, tracker.start_y, menu_active);
        }
        tracker.clear();
    }

    keys
}

/// Evaluates static screen tap coordinates into navigation or trim actions.
pub fn evaluate_tap_zones(x: u8, y: u8, menu_active: bool) -> u16 {
    let mut keys = 0u16;

    if menu_active {
        // Priority 1: Footer navigation buttons ([ESC] Back / [OK] Select)
        if y >= 46 {
            if x >= 64 {
                // Right side of footer: "[ESC] Back" -> Cancel
                keys |= KEY_NAV_CANCEL;
            } else {
                // Left side of footer: "[OK] Select" -> OK
                keys |= KEY_NAV_OK;
            }
        }
        // Priority 2: Header / Up navigation
        else if y < 18 {
            keys |= KEY_NAV_UP;
        }
        // Content body (18..=45) intentionally does NOT emit OK on tap to prevent accidental selection!
        // Pilots select via the dedicated [OK] footer button or tactile OK (PA10).
    } else {
        // Flight Dashboard:
        // Top status bar (y < 14): Tap to cycle dashboard page forward!
        if y < 14 {
            keys |= KEY_NAV_DOWN;
        }
        // Left border: Throttle trim (Up / Down)
        else if x < 16 {
            if y < 32 {
                keys |= KEY_TRIM_THR_U;
            } else {
                keys |= KEY_TRIM_THR_D;
            }
        }
        // Right border: Pitch trim (Up / Down)
        else if x > 112 {
            if y < 32 {
                keys |= KEY_TRIM_PITCH_U;
            } else {
                keys |= KEY_TRIM_PITCH_D;
            }
        }
        // Bottom border: Yaw and Roll trims
        else if y > 52 {
            if x < 36 {
                keys |= KEY_TRIM_YAW_L;
            } else if x < 64 {
                keys |= KEY_TRIM_YAW_R;
            } else if x < 92 {
                keys |= KEY_TRIM_ROLL_L;
            } else {
                keys |= KEY_TRIM_ROLL_R;
            }
        }
        // Center area emits KEY_MENU_OPEN to accumulate towards the 1.2s menu open requirement
        else if (32..=96).contains(&x) && (20..=44).contains(&y) {
            keys |= KEY_MENU_OPEN;
        }
    }

    keys
}

/// Evaluates touch release when no touch packet / interrupt is asserted.
pub fn process_touch_release(menu_active: bool) -> u16 {
    let tracker = unsafe { &mut *core::ptr::addr_of_mut!(GLOBAL_SWIPE_TRACKER) };
    let mut keys = 0u16;
    if tracker.active && !tracker.swiped {
        keys |= evaluate_tap_zones(tracker.start_x, tracker.start_y, menu_active);
    }
    tracker.active = false;
    tracker.swiped = false;
    keys
}

/// Evaluates modifier trims using Left Front (`PA9`) and Right Front (`PA10`) buttons
/// combined with gimbal stick deflections.
pub fn process_modifier_trims(
    left_held: bool,
    right_held: bool,
    sticks: &Sticks,
) -> (u16, bool, bool) {
    process_modifier_trims_cfg(left_held, right_held, sticks, false)
}

/// Evaluates modifier trims with configurable throttle trim enablement.
pub fn process_modifier_trims_cfg(
    left_held: bool,
    right_held: bool,
    sticks: &Sticks,
    throttle_trim_enabled: bool,
) -> (u16, bool, bool) {
    let mut trim_keys = 0u16;
    let mut suppress_left = false;
    let mut suppress_right = false;

    // Left Front held -> Left Stick controls Throttle (if enabled) & Yaw trims
    if left_held {
        if sticks.yaw < -TRIM_MODIFIER_STICK_THRESHOLD {
            trim_keys |= KEY_TRIM_YAW_L;
            suppress_left = true;
        } else if sticks.yaw > TRIM_MODIFIER_STICK_THRESHOLD {
            trim_keys |= KEY_TRIM_YAW_R;
            suppress_left = true;
        }

        if throttle_trim_enabled {
            if sticks.throttle < -TRIM_MODIFIER_STICK_THRESHOLD {
                trim_keys |= KEY_TRIM_THR_D;
                suppress_left = true;
            } else if sticks.throttle > TRIM_MODIFIER_STICK_THRESHOLD {
                trim_keys |= KEY_TRIM_THR_U;
                suppress_left = true;
            }
        }
    }

    // Right Front held -> Right Stick controls Roll & Pitch trims
    if right_held {
        if sticks.roll < -TRIM_MODIFIER_STICK_THRESHOLD {
            trim_keys |= KEY_TRIM_ROLL_L;
            suppress_right = true;
        } else if sticks.roll > TRIM_MODIFIER_STICK_THRESHOLD {
            trim_keys |= KEY_TRIM_ROLL_R;
            suppress_right = true;
        }

        if sticks.pitch < -TRIM_MODIFIER_STICK_THRESHOLD {
            trim_keys |= KEY_TRIM_PITCH_D;
            suppress_right = true;
        } else if sticks.pitch > TRIM_MODIFIER_STICK_THRESHOLD {
            trim_keys |= KEY_TRIM_PITCH_U;
            suppress_right = true;
        }
    }

    (trim_keys, suppress_left, suppress_right)
}

/// Unified touch & button processing tick.
///
/// Reads touch sensor, processes modifier trims, and sets the composite key state
/// in `boot::set_touch_keys()`.
pub fn update_inputs(
    touch_sample: Option<&TouchSample>,
    sticks: &Sticks,
    menu_active: bool,
    rear_left_func: u8,
    rear_right_func: u8,
    throttle_trim_enabled: bool,
) -> u16 {
    let (left_btn, right_btn) = boot::read_front_buttons();

    let mut combined = 0u16;

    if menu_active {
        // When any menu or wizard is open, rear buttons unconditionally act as UI navigation
        if left_btn {
            combined |= KEY_NAV_CANCEL;
        }
        if right_btn {
            combined |= KEY_NAV_OK;
        }
    } else {
        // When in flight / dashboard mode:
        // func 2: Trims Modifier
        let left_mod = rear_left_func == 2;
        let right_mod = rear_right_func == 2;
        if (left_mod && left_btn) || (right_mod && right_btn) {
            let (mod_trims, _, _) = process_modifier_trims_cfg(
                left_btn && left_mod,
                right_btn && right_mod,
                sticks,
                throttle_trim_enabled,
            );
            combined |= mod_trims;
        }
    }

    // 2. Process touch navigation & virtual trims
    let touch_keys = if let Some(sample) = touch_sample {
        touch_to_nav_keys(sample, menu_active)
    } else {
        let tracker = unsafe { &mut *core::ptr::addr_of_mut!(GLOBAL_SWIPE_TRACKER) };
        if tracker.active {
            #[cfg(not(test))]
            let now = crate::time::millis();
            #[cfg(test)]
            let now: u32 = 0;
            if now.wrapping_sub(tracker.last_packet_ms) >= 40 {
                let mut keys = 0u16;
                if !tracker.swiped {
                    keys |= evaluate_tap_zones(tracker.start_x, tracker.start_y, menu_active);
                }
                tracker.clear();
                keys
            } else {
                0
            }
        } else {
            0
        }
    };
    combined |= touch_keys;

    boot::set_touch_keys(combined);
    combined
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gesture_navigation() {
        let sample_up = TouchSample {
            gesture: Gesture::SwipeUp,
            point: None,
        };
        assert_eq!(touch_to_nav_keys(&sample_up, true), KEY_NAV_UP);

        let sample_down = TouchSample {
            gesture: Gesture::SwipeDown,
            point: None,
        };
        assert_eq!(touch_to_nav_keys(&sample_down, true), KEY_NAV_DOWN);

        let sample_right = TouchSample {
            gesture: Gesture::SwipeRight,
            point: None,
        };
        assert_eq!(touch_to_nav_keys(&sample_right, true), KEY_NAV_OK);

        let sample_left = TouchSample {
            gesture: Gesture::SwipeLeft,
            point: None,
        };
        assert_eq!(touch_to_nav_keys(&sample_left, true), KEY_NAV_CANCEL);
    }

    #[test]
    fn test_menu_tap_zones() {
        assert_ne!(evaluate_tap_zones(64, 10, true) & KEY_NAV_UP, 0);
        assert_eq!(evaluate_tap_zones(80, 55, true), KEY_NAV_CANCEL); // [ESC] Back
        assert_eq!(evaluate_tap_zones(40, 55, true), KEY_NAV_OK);     // [OK] Select
    }

    #[test]
    fn test_menu_tap_zones_contact_and_liftup() {
        let mut tracker = SwipeTracker::default();

        // 1. Initial press down at [ESC] Back zone
        let pt_press = TouchPoint {
            x: 80,
            y: 55,
            raw_x: 110,
            raw_y: 96,
            event: TouchEventKind::PressDown,
        };
        let sample_press = TouchSample {
            gesture: Gesture::None,
            point: Some(pt_press),
        };
        // During initial press down, keys are 0 while tracker awaits drag vs tap
        assert_eq!(touch_to_nav_keys_with_tracker(&sample_press, true, &mut tracker), 0);
        assert!(tracker.active);

        // 2. LiftUp event (release / quick tap) emits KEY_NAV_CANCEL
        let pt_lift = TouchPoint {
            x: 80,
            y: 55,
            raw_x: 110,
            raw_y: 96,
            event: TouchEventKind::LiftUp,
        };
        let sample_lift = TouchSample {
            gesture: Gesture::None,
            point: Some(pt_lift),
        };
        assert_eq!(touch_to_nav_keys_with_tracker(&sample_lift, true, &mut tracker), KEY_NAV_CANCEL);
        assert!(!tracker.active);

        // 3. NoEvent should NOT trigger any keys
        let pt_none = TouchPoint {
            x: 110,
            y: 30,
            raw_x: 60,
            raw_y: 36,
            event: TouchEventKind::NoEvent,
        };
        let sample_none = TouchSample {
            gesture: Gesture::None,
            point: Some(pt_none),
        };
        assert_eq!(touch_to_nav_keys_with_tracker(&sample_none, true, &mut SwipeTracker::default()), 0);
    }

    #[test]
    fn test_software_swipe_drag() {
        let mut tracker = SwipeTracker::default();

        // Press down at center of menu
        let pt_start = TouchPoint {
            x: 60,
            y: 35,
            raw_x: 70,
            raw_y: 136,
            event: TouchEventKind::PressDown,
        };
        let s_start = TouchSample {
            gesture: Gesture::None,
            point: Some(pt_start),
        };
        assert_eq!(touch_to_nav_keys_with_tracker(&s_start, true, &mut tracker), 0);

        // Drag down by 20 pixels (y: 35 -> 55)
        let pt_drag = TouchPoint {
            x: 60,
            y: 55,
            raw_x: 110,
            raw_y: 136,
            event: TouchEventKind::Contact,
        };
        let s_drag = TouchSample {
            gesture: Gesture::None,
            point: Some(pt_drag),
        };
        assert_eq!(touch_to_nav_keys_with_tracker(&s_drag, true, &mut tracker), KEY_NAV_DOWN);
        assert!(tracker.swiped);

        // Release after swipe should NOT emit a tap
        let pt_end = TouchPoint {
            x: 60,
            y: 55,
            raw_x: 110,
            raw_y: 136,
            event: TouchEventKind::LiftUp,
        };
        let s_end = TouchSample {
            gesture: Gesture::None,
            point: Some(pt_end),
        };
        assert_eq!(touch_to_nav_keys_with_tracker(&s_end, true, &mut tracker), 0);
    }

    #[test]
    fn test_virtual_trims_on_dashboard() {
        // Tap left border -> Throttle trim up
        let pt_thr_u = TouchPoint {
            x: 5,
            y: 15,
            raw_x: 30,
            raw_y: 246,
            event: TouchEventKind::PressDown,
        };
        let sample = TouchSample {
            gesture: Gesture::None,
            point: Some(pt_thr_u),
        };
        assert_eq!(touch_to_nav_keys(&sample, false), KEY_TRIM_THR_U);

        // Tap right border -> Pitch trim down
        let pt_pit_d = TouchPoint {
            x: 120,
            y: 45,
            raw_x: 90,
            raw_y: 16,
            event: TouchEventKind::PressDown,
        };
        let sample_pit = TouchSample {
            gesture: Gesture::None,
            point: Some(pt_pit_d),
        };
        assert_eq!(touch_to_nav_keys(&sample_pit, false), KEY_TRIM_PITCH_D);
    }

    #[test]
    fn test_modifier_trims() {
        let mut sticks = Sticks {
            roll: 0,
            pitch: 0,
            throttle: -1000, // Real-world idle throttle at rest
            yaw: 0,
        };

        // Neither button pressed
        let (trims, supp_l, supp_r) = process_modifier_trims(false, false, &sticks);
        assert_eq!(trims, 0);
        assert!(!supp_l);
        assert!(!supp_r);

        // Left Front held with throttle at -1000 but throttle trim disabled -> No false trim!
        let (trims, supp_l, _) = process_modifier_trims(true, false, &sticks);
        assert_eq!(trims, 0);
        assert!(!supp_l); // Normal Cancel NOT suppressed!

        // Left Front held + Yaw stick left -> Yaw trim emitted
        sticks.yaw = -500;
        let (trims, supp_l, _) = process_modifier_trims(true, false, &sticks);
        assert_eq!(trims, KEY_TRIM_YAW_L);
        assert!(supp_l); // Suppressed normal Cancel

        // Right Front held + Roll stick right
        sticks.yaw = 0;
        sticks.roll = 600;
        let (trims, _, supp_r) = process_modifier_trims(false, true, &sticks);
        assert_eq!(trims, KEY_TRIM_ROLL_R);
        assert!(supp_r); // Suppressed normal OK
    }

    #[test]
    fn test_menu_touch_scrolling_and_instant_reversal() {
        let mut tracker = SwipeTracker::default();

        // 1. Initial press at y = 30
        let s_press = TouchSample {
            gesture: Gesture::None,
            point: Some(TouchPoint {
                x: 60,
                y: 30,
                raw_x: 60,
                raw_y: 120,
                event: TouchEventKind::PressDown,
            }),
        };
        assert_eq!(touch_to_nav_keys_with_tracker(&s_press, true, &mut tracker), 0);

        // 2. Drag down by 12 px (to y = 42) -> fires KEY_NAV_DOWN
        let s_drag_down = TouchSample {
            gesture: Gesture::None,
            point: Some(TouchPoint {
                x: 60,
                y: 42,
                raw_x: 84,
                raw_y: 120,
                event: TouchEventKind::Contact,
            }),
        };
        assert_eq!(touch_to_nav_keys_with_tracker(&s_drag_down, true, &mut tracker), KEY_NAV_DOWN);
        assert_eq!(tracker.scroll_dir, 1);

        // 3. Move further down to y = 46 (peak at 46)
        let s_drag_down2 = TouchSample {
            gesture: Gesture::None,
            point: Some(TouchPoint {
                x: 60,
                y: 46,
                raw_x: 92,
                raw_y: 120,
                event: TouchEventKind::Contact,
            }),
        };
        // Should not trigger (only 4px past 42, and elapsed time < 200 ms in test)
        assert_eq!(touch_to_nav_keys_with_tracker(&s_drag_down2, true, &mut tracker), 0);
        assert_eq!(tracker.max_y, 46);

        // 4. Reverse direction! Move up to y = 42 (4px up from peak 46 -> reversal detected!)
        let s_rev = TouchSample {
            gesture: Gesture::None,
            point: Some(TouchPoint {
                x: 60,
                y: 42,
                raw_x: 84,
                raw_y: 120,
                event: TouchEventKind::Contact,
            }),
        };
        assert_eq!(touch_to_nav_keys_with_tracker(&s_rev, true, &mut tracker), 0);
        assert_eq!(tracker.anchor_y, 46); // Anchor reset to peak 46!

        // 5. Move up to y = 34 (46 - 34 = 12 px up from peak!) -> fires KEY_NAV_UP immediately!
        let s_up = TouchSample {
            gesture: Gesture::None,
            point: Some(TouchPoint {
                x: 60,
                y: 34,
                raw_x: 68,
                raw_y: 120,
                event: TouchEventKind::Contact,
            }),
        };
        assert_eq!(touch_to_nav_keys_with_tracker(&s_up, true, &mut tracker), KEY_NAV_UP);
        assert_eq!(tracker.scroll_dir, -1);
    }

    #[test]
    fn test_dashboard_center_hold_and_swipe() {
        let mut tracker = SwipeTracker::default();

        // 1. Press and hold in center (x=64, y=30) -> emits KEY_MENU_OPEN
        let s_center = TouchSample {
            gesture: Gesture::None,
            point: Some(TouchPoint {
                x: 64,
                y: 30,
                raw_x: 60,
                raw_y: 128,
                event: TouchEventKind::Contact,
            }),
        };
        assert_eq!(touch_to_nav_keys_with_tracker(&s_center, false, &mut tracker), KEY_MENU_OPEN);

        // 2. Minor tremor in place (x=66, y=32) -> still emits KEY_MENU_OPEN
        let s_tremor = TouchSample {
            gesture: Gesture::None,
            point: Some(TouchPoint {
                x: 66,
                y: 32,
                raw_x: 64,
                raw_y: 124,
                event: TouchEventKind::Contact,
            }),
        };
        assert_eq!(touch_to_nav_keys_with_tracker(&s_tremor, false, &mut tracker), KEY_MENU_OPEN);

        // 3. Deliberate horizontal swipe left (x=45, dx = -19) -> emits KEY_NAV_DOWN (next dashboard page)
        let s_swipe_left = TouchSample {
            gesture: Gesture::None,
            point: Some(TouchPoint {
                x: 45,
                y: 30,
                raw_x: 60,
                raw_y: 166,
                event: TouchEventKind::Contact,
            }),
        };
        assert_eq!(touch_to_nav_keys_with_tracker(&s_swipe_left, false, &mut tracker), KEY_NAV_DOWN);
        assert!(tracker.swiped);
    }
}
