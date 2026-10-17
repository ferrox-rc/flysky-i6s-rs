//! Capacitive touchscreen subsystem for FlySky FS-i6S.
//!
//! Provides the FocalTech FT6236 hardware I2C1 driver and the navigation/trim translation adapter.

pub mod ft6236;
pub mod nav;

pub use ft6236::{Gesture, TouchEventKind, TouchPoint, TouchSample};
pub use nav::touch_to_nav_keys;
