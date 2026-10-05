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

/// Evaluates a valid touch sample and screen tap zones into UI navigation key flags.
///
/// Only invoked when a touch interrupt event (pulse on PC12) has occurred and a valid
/// `TouchSample` was read from the FT6236 controller.
pub fn touch_to_nav_keys(sample: &TouchSample, menu_active: bool) -> u16 {
    let mut keys = 0u16;

    // 1. Gesture Decoding
    match sample.gesture {
        Gesture::SwipeUp => keys |= KEY_NAV_UP,
        Gesture::SwipeDown => keys |= KEY_NAV_DOWN,
        Gesture::SwipeRight => keys |= KEY_NAV_OK,
        Gesture::SwipeLeft => keys |= KEY_NAV_CANCEL,
        Gesture::None => {}
    }

    // 2. Direct Tap Hitboxes (Active on PressDown, Contact, or LiftUp when no swipe gesture)
    if keys == 0 {
        if let Some(pt) = sample.point {
            if pt.event != TouchEventKind::NoEvent {
            if menu_active {
                // Menu Navigation Zones:
                // Top header / row 0: Up
                // Bottom footer / row 2: Down
                // Left 25%: Cancel
                // Right 25%: OK
                if pt.y < 22 {
                    keys |= KEY_NAV_UP;
                } else if pt.y > 44 {
                    keys |= KEY_NAV_DOWN;
                } else if pt.x < 32 {
                    keys |= KEY_NAV_CANCEL;
                } else if pt.x > 96 {
                    keys |= KEY_NAV_OK;
                } else {
                    keys |= KEY_NAV_OK; // Center tap selects
                }
            } else {
                // Flight Dashboard Virtual Trims:
                // Left border: Throttle trim (Up / Down)
                if pt.x < 16 {
                    if pt.y < 32 {
                        keys |= KEY_TRIM_THR_U;
                    } else {
                        keys |= KEY_TRIM_THR_D;
                    }
                }
                // Right border: Pitch trim (Up / Down)
                else if pt.x > 112 {
                    if pt.y < 32 {
                        keys |= KEY_TRIM_PITCH_U;
                    } else {
                        keys |= KEY_TRIM_PITCH_D;
                    }
                }
                // Bottom border: Yaw and Roll trims
                else if pt.y > 52 {
                    if pt.x < 36 {
                        keys |= KEY_TRIM_YAW_L;
                    } else if pt.x < 64 {
                        keys |= KEY_TRIM_YAW_R;
                    } else if pt.x < 92 {
                        keys |= KEY_TRIM_ROLL_L;
                    } else {
                        keys |= KEY_TRIM_ROLL_R;
                    }
                }
                else if pt.x >= 32 && pt.x <= 96 && pt.y >= 20 && pt.y <= 44 {
                    keys |= KEY_NAV_OK;
                }
            }
        }
    }
}

    keys
}

/// Evaluates modifier trims using Left Front (`PA9`) and Right Front (`PA10`) buttons
/// combined with gimbal stick deflections.
///
/// Returns:
/// - `trim_keys`: Trim bitmask (`KEY_TRIM_*`)
/// - `suppress_left_nav`: True if Left Front button was consumed as trim modifier
/// - `suppress_right_nav`: True if Right Front button was consumed as trim modifier
pub fn process_modifier_trims(
    left_held: bool,
    right_held: bool,
    sticks: &Sticks,
) -> (u16, bool, bool) {
    let mut trim_keys = 0u16;
    let mut suppress_left = false;
    let mut suppress_right = false;

    // Left Front held -> Left Stick controls Throttle & Yaw trims
    if left_held {
        if sticks.yaw < -TRIM_MODIFIER_STICK_THRESHOLD {
            trim_keys |= KEY_TRIM_YAW_L;
            suppress_left = true;
        } else if sticks.yaw > TRIM_MODIFIER_STICK_THRESHOLD {
            trim_keys |= KEY_TRIM_YAW_R;
            suppress_left = true;
        }

        if sticks.throttle < -TRIM_MODIFIER_STICK_THRESHOLD {
            trim_keys |= KEY_TRIM_THR_D;
            suppress_left = true;
        } else if sticks.throttle > TRIM_MODIFIER_STICK_THRESHOLD {
            trim_keys |= KEY_TRIM_THR_U;
            suppress_left = true;
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
) -> u16 {
    let (left_btn, right_btn) = boot::read_front_buttons();

    // 1. Process modifier trims
    let (mod_trims, suppress_left, suppress_right) =
        process_modifier_trims(left_btn, right_btn, sticks);

    // 2. Process touch navigation & virtual trims only when touch event happened
    let touch_keys = if let Some(sample) = touch_sample {
        touch_to_nav_keys(sample, menu_active)
    } else {
        0
    };

    // 3. Assemble composite keys
    let mut combined = mod_trims | touch_keys;

    if left_btn && !suppress_left {
        combined |= KEY_NAV_CANCEL;
    }
    if right_btn && !suppress_right {
        combined |= KEY_NAV_OK;
    }

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
        let pt_top = TouchPoint {
            x: 64,
            y: 10,
            raw_x: 20,
            raw_y: 128,
            event: TouchEventKind::PressDown,
        };
        let sample_top = TouchSample {
            gesture: Gesture::None,
            point: Some(pt_top),
        };
        assert_ne!(touch_to_nav_keys(&sample_top, true) & KEY_NAV_UP, 0);

        let pt_bottom = TouchPoint {
            x: 64,
            y: 50,
            raw_x: 100,
            raw_y: 128,
            event: TouchEventKind::PressDown,
        };
        let sample_bottom = TouchSample {
            gesture: Gesture::None,
            point: Some(pt_bottom),
        };
        assert_ne!(touch_to_nav_keys(&sample_bottom, true) & KEY_NAV_DOWN, 0);
    }

    #[test]
    fn test_menu_tap_zones_contact_and_liftup() {
        // Contact event (finger held / active touch) should trigger tap zones
        let pt_contact = TouchPoint {
            x: 10,
            y: 30,
            raw_x: 60,
            raw_y: 236,
            event: TouchEventKind::Contact,
        };
        let sample_contact = TouchSample {
            gesture: Gesture::None,
            point: Some(pt_contact),
        };
        assert_eq!(touch_to_nav_keys(&sample_contact, true), KEY_NAV_CANCEL);

        // LiftUp event (release / quick tap) should also trigger tap zones
        let pt_lift = TouchPoint {
            x: 110,
            y: 30,
            raw_x: 60,
            raw_y: 36,
            event: TouchEventKind::LiftUp,
        };
        let sample_lift = TouchSample {
            gesture: Gesture::None,
            point: Some(pt_lift),
        };
        assert_eq!(touch_to_nav_keys(&sample_lift, true), KEY_NAV_OK);

        // NoEvent should NOT trigger any keys
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
        assert_eq!(touch_to_nav_keys(&sample_none, true), 0);
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
            throttle: 0,
            yaw: 0,
        };

        // Neither button pressed
        let (trims, supp_l, supp_r) = process_modifier_trims(false, false, &sticks);
        assert_eq!(trims, 0);
        assert!(!supp_l);
        assert!(!supp_r);

        // Left Front held + Yaw stick left
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
