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
    pub active: bool,
    pub swiped: bool,
}

static mut GLOBAL_SWIPE_TRACKER: SwipeTracker = SwipeTracker {
    start_x: 0,
    start_y: 0,
    last_x: 0,
    last_y: 0,
    active: false,
    swiped: false,
};

/// Evaluates a valid touch sample and screen tap zones into UI navigation key flags.
///
/// Only invoked when a touch interrupt event (pulse on PC12) has occurred and a valid
/// `TouchSample` was read from the FT6236 controller.
pub fn touch_to_nav_keys(sample: &TouchSample, menu_active: bool) -> u16 {
    let tracker = unsafe { &mut *core::ptr::addr_of_mut!(GLOBAL_SWIPE_TRACKER) };
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
                tracker.start_x = pt.x;
                tracker.start_y = pt.y;
                tracker.last_x = pt.x;
                tracker.last_y = pt.y;
                tracker.active = true;
                tracker.swiped = false;
            }
            TouchEventKind::Contact => {
                if !tracker.active {
                    tracker.start_x = pt.x;
                    tracker.start_y = pt.y;
                    tracker.active = true;
                    tracker.swiped = false;
                }
                tracker.last_x = pt.x;
                tracker.last_y = pt.y;

                // Software swipe detection on drag displacement
                let dx = pt.x as i16 - tracker.start_x as i16;
                let dy = pt.y as i16 - tracker.start_y as i16;

                if menu_active {
                    // Vertical displacement takes priority for vertical list/page navigation
                    if dy <= -16 {
                        keys |= KEY_NAV_UP;
                        tracker.start_y = pt.y;
                        tracker.swiped = true;
                    } else if dy >= 16 {
                        keys |= KEY_NAV_DOWN;
                        tracker.start_y = pt.y;
                        tracker.swiped = true;
                    } else if dx <= -20 {
                        keys |= KEY_NAV_CANCEL;
                        tracker.start_x = pt.x;
                        tracker.swiped = true;
                    } else if dx >= 20 {
                        keys |= KEY_NAV_OK;
                        tracker.start_x = pt.x;
                        tracker.swiped = true;
                    }
                } else {
                    // Dashboard mode: horizontal and vertical swipes cycle dashboard pages
                    if dx <= -20 || dy >= 16 {
                        keys |= KEY_NAV_DOWN;
                        tracker.start_x = pt.x;
                        tracker.start_y = pt.y;
                        tracker.swiped = true;
                    } else if dx >= 20 || dy <= -16 {
                        keys |= KEY_NAV_UP;
                        tracker.start_x = pt.x;
                        tracker.start_y = pt.y;
                        tracker.swiped = true;
                    }
                }
            }
            TouchEventKind::LiftUp => {
                // If finger lifted without having triggered a swipe gesture, process as a tap
                if tracker.active && !tracker.swiped {
                    let tap_x = tracker.start_x;
                    let tap_y = tracker.start_y;
                    keys |= evaluate_tap_zones(tap_x, tap_y, menu_active);
                }
                tracker.active = false;
                tracker.swiped = false;
            }
            TouchEventKind::NoEvent => {}
        }

        // On flight dashboard only: virtual trims on screen edges evaluate immediately during press/hold
        if !menu_active && !tracker.swiped && keys == 0 {
            if pt.x < 16 || pt.x > 112 || pt.y > 52 {
                keys |= evaluate_tap_zones(pt.x, pt.y, false);
            }
        }
    } else {
        // Touch point is None: finger released / lifted
        if tracker.active && !tracker.swiped {
            keys |= evaluate_tap_zones(tracker.start_x, tracker.start_y, menu_active);
        }
        tracker.active = false;
        tracker.swiped = false;
    }

    keys
}

/// Evaluates static screen tap coordinates into navigation or trim actions.
pub fn evaluate_tap_zones(x: u8, y: u8, menu_active: bool) -> u16 {
    let mut keys = 0u16;

    if menu_active {
        // Priority 1: Footer navigation buttons ([ESC] Back / [OK] Select)
        if y >= 48 {
            if x >= 70 {
                // Right side of footer: "[ESC] Back" -> Cancel
                keys |= KEY_NAV_CANCEL;
            } else if x <= 58 {
                // Left side of footer: "[OK] Select" -> OK
                keys |= KEY_NAV_OK;
            } else {
                // Center footer -> Down
                keys |= KEY_NAV_DOWN;
            }
        }
        // Priority 2: Header / Up navigation
        else if y < 20 {
            keys |= KEY_NAV_UP;
        }
        // Priority 3: Outer border Cancel / OK
        else if x < 28 {
            keys |= KEY_NAV_CANCEL;
        } else if x > 100 {
            keys |= KEY_NAV_OK;
        }
        // Priority 4: Content body -> Select
        else {
            keys |= KEY_NAV_OK;
        }
    } else {
        // Flight Dashboard:
        // Left border: Throttle trim (Up / Down)
        if x < 16 {
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
        // Center tap opens menu immediately
        else if (32..=96).contains(&x) && (20..=44).contains(&y) {
            keys |= KEY_NAV_OK | KEY_MENU_OPEN;
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
        process_touch_release(menu_active)
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
        assert_ne!(evaluate_tap_zones(64, 50, true) & KEY_NAV_DOWN, 0);
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
}
