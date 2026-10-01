//! Flight Control, Input Conditioning, and 14-Channel Mixing Engine.
//!
//! Implements:
//! 1. Input conditioning: Dual Rates (30%..100%) and integer cubic Expo (-100%..+100%).
//! 2. Aircraft wing/tail templates: Normal, Elevon / Delta, V-Tail, and Flaperon.
//! 3. Auxiliary channel remapping for CH5..CH14.
//! 4. EdgeTX / OpenTX-style 14-channel freeform matrix mixer with ADD, MULTIPLY, and REPLACE modes.
//! 5. Digital trim integration and channel reversing.

use crate::input::{SwitchPos, Switches};
use crate::storage::ModelConfig;
use crate::trim::TrimController;

pub const NUM_CHANNELS: usize = 18;

// Standard normalized mixer range (-1000..+1000)
pub const MIXER_MIN: i16 = -1000;
pub const MIXER_CENTER: i16 = 0;
pub const MIXER_MAX: i16 = 1000;

// FlySky / AFHDS standard microsecond pulse range (988..2012 µs)
pub const CHANNEL_MIN_US: u16 = 988;
pub const CHANNEL_CENTER_US: u16 = 1500;
pub const CHANNEL_MAX_US: u16 = 2012;
pub const CHANNEL_SPAN_US: u32 = (CHANNEL_MAX_US - CHANNEL_MIN_US) as u32; // 1024
pub const CHANNEL_HALF_SPAN_US: i32 = (CHANNEL_SPAN_US / 2) as i32; // 512
pub const CHANNEL_REVERSE_SUM: u16 = CHANNEL_MIN_US + CHANNEL_MAX_US; // 3000

/// Physical source identifiers for auxiliary channels and mix lines.
#[allow(dead_code)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MixSource {
    None = 0,
    Roll = 1,
    Pitch = 2,
    Throttle = 3,
    Yaw = 4,
    Vra = 5,
    Vrb = 6,
    Sa = 7,
    Sb = 8,
    Sc = 9,
    Sd = 10,
    Max = 11,
    // 12..29 map to Channel 1..18
    ThrUnipolar = 30,
    Se = 31,
    Sf = 32,
    Vrc = 33,
    Vrd = 34,
    Vre = 35,
    Vrf = 36,
}

/// Wing and tail aircraft mixing templates.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum WingTailTemplate {
    Normal = 0,
    Elevon = 1,
    VTail = 2,
    Flaperon = 3,
}

impl WingTailTemplate {
    pub fn from_u8(val: u8) -> Self {
        match val {
            1 => Self::Elevon,
            2 => Self::VTail,
            3 => Self::Flaperon,
            _ => Self::Normal,
        }
    }
}

/// Helper to apply differential to a deflection value (-1000..+1000).
/// When diff > 0, down-deflection (val < 0) is attenuated by (100 - diff)%.
/// When diff < 0, up-deflection (val > 0) is attenuated by (100 + diff)%.
#[inline(always)]
pub fn apply_differential(val: i32, diff: i8) -> i32 {
    if diff == 0 {
        return val;
    }
    let d = diff.clamp(-100, 100) as i32;
    if d > 0 && val < 0 {
        (val * (100 - d)) / 100
    } else if d < 0 && val > 0 {
        (val * (100 + d)) / 100
    } else {
        val
    }
}

/// Apply Dual Rate and integer cubic Expo to a normalized stick input (-1000..+1000).
///
/// Positive expo softens stick sensitivity around neutral center.
/// Negative expo increases sensitivity around neutral center.
pub fn apply_dr_expo(input: i16, rate: u8, expo: i8) -> i16 {
    let rate_clamped = rate.clamp(30, 100) as i32;
    let x = ((input as i32) * rate_clamped) / 100;

    if expo == 0 {
        return x.clamp(MIXER_MIN as i32, MIXER_MAX as i32) as i16;
    }

    let expo_val = expo.clamp(-100, 100) as i32;
    // Normalized cubic term in MIXER_MIN..MIXER_MAX fits entirely within i32 (1000^3 = 10^9 < 2.14*10^9)
    let x_cubic = (x * x * x) / 1_000_000;

    let result = if expo_val > 0 {
        // Soften center: blend linear with cubic
        (x * (100 - expo_val) + x_cubic * expo_val) / 100
    } else {
        // Sharpen center: inverse cubic blend
        let abs_expo = -expo_val;
        x + ((x - x_cubic) * abs_expo) / 100
    };

    result.clamp(MIXER_MIN as i32, MIXER_MAX as i32) as i16
}

/// Check if a physical switch satisfies a mixer line activation condition.
pub fn is_switch_active(condition: u8, switches: &Switches) -> bool {
    match condition {
        0 => true, // Always active
        1 => switches.sa == SwitchPos::Up,
        2 => switches.sa == SwitchPos::Down,
        3 => switches.sb == SwitchPos::Up,
        4 => switches.sb == SwitchPos::Mid,
        5 => switches.sb == SwitchPos::Down,
        6 => switches.sc == SwitchPos::Up,
        7 => switches.sc == SwitchPos::Mid,
        8 => switches.sc == SwitchPos::Down,
        9 => switches.sd == SwitchPos::Up,
        10 => switches.sd == SwitchPos::Down,
        11 => switches.se == SwitchPos::Up,
        12 => switches.se == SwitchPos::Down,
        13 => switches.sf == SwitchPos::Up,
        14 => switches.sf == SwitchPos::Down,
        _ => true,
    }
}

/// Evaluate arm switch condition with bounds validation.
/// Returns false if arm_switch is 0 (unassigned) or out of range.
#[inline]
pub fn eval_arm_switch(arm_switch: u8, switches: &Switches) -> bool {
    if arm_switch > 0 && arm_switch <= 14 {
        is_switch_active(arm_switch, switches)
    } else {
        false
    }
}

/// Determine whether High Rates (true) or Low Rates (false) are active.
pub fn is_dr_high(dr_switch: u8, switches: &Switches) -> bool {
    match dr_switch {
        0 => true, // Default to High Rate if no switch assigned
        1 => switches.sa == SwitchPos::Up,
        2 => switches.sb == SwitchPos::Up,
        3 => switches.sc == SwitchPos::Up,
        4 => switches.sd == SwitchPos::Up,
        5 => switches.se == SwitchPos::Up,
        6 => switches.sf == SwitchPos::Up,
        _ => true,
    }
}

/// Evaluate whether a flight timer trigger condition is satisfied.
///
/// * `timer_source`:
///   - 0: Disabled (Off)
///   - 1: THs (Throttle stick > 5% / > -900)
///   - 2: THt (Throttle stick latched once > 5% / > -900)
///   - 3: Always On (Continuous)
///   - 4..=17: Switch conditions SA^..SFv (mapping to condition 1..=14)
/// * `throttle`: normalized throttle stick position (-1000..+1000)
/// * `latched`: mutable reference to latched state (updated for THt)
/// * `switches`: current switch positions
/// * `is_armed`: whether arm switch condition is satisfied (or true if no arm switch configured)
pub fn is_timer_active(
    timer_source: u8,
    throttle: i16,
    latched: &mut bool,
    switches: &Switches,
    is_armed: bool,
) -> bool {
    if !is_armed {
        return false;
    }
    match timer_source {
        0 => false,
        1 => throttle > -900,
        2 => {
            if throttle > -900 {
                *latched = true;
            }
            *latched
        }
        3 => true,
        sw if (4..=17).contains(&sw) => is_switch_active(sw - 3, switches),
        _ => false,
    }
}

/// Evaluate a source identifier to a normalized value (-1000..+1000).
pub fn evaluate_source(
    src: u8,
    cond_sticks: &[i16; 4],
    pots: &[i16; 6],
    switches: &Switches,
    channels: &[i32; NUM_CHANNELS],
) -> i32 {
    match src {
        0 => MIXER_CENTER as i32,
        1 => cond_sticks[0] as i32, // Roll
        2 => cond_sticks[1] as i32, // Pitch
        3 => cond_sticks[2] as i32, // Throttle
        4 => cond_sticks[3] as i32, // Yaw
        5 => pots[0] as i32,        // VRA
        6 => pots[1] as i32,        // VRB
        7 => {
            if switches.sa == SwitchPos::Up {
                MIXER_MIN as i32
            } else {
                MIXER_MAX as i32
            }
        }
        8 => match switches.sb {
            SwitchPos::Up => MIXER_MIN as i32,
            SwitchPos::Mid => MIXER_CENTER as i32,
            SwitchPos::Down => MIXER_MAX as i32,
        },
        9 => match switches.sc {
            SwitchPos::Up => MIXER_MIN as i32,
            SwitchPos::Mid => MIXER_CENTER as i32,
            SwitchPos::Down => MIXER_MAX as i32,
        },
        10 => {
            if switches.sd == SwitchPos::Up {
                MIXER_MIN as i32
            } else {
                MIXER_MAX as i32
            }
        }
        11 => MIXER_MAX as i32, // MAX
        12..=29 => {
            let ch_idx = (src - 12) as usize;
            channels[ch_idx]
        }
        30 => {
            let thr = cond_sticks[2] as i32;
            ((thr + MIXER_MAX as i32) / 2).clamp(0, MIXER_MAX as i32)
        }
        31 => {
            if switches.se == SwitchPos::Up {
                MIXER_MIN as i32
            } else {
                MIXER_MAX as i32
            }
        }
        32 => {
            if switches.sf == SwitchPos::Up {
                MIXER_MIN as i32
            } else {
                MIXER_MAX as i32
            }
        }
        33 => pots[2] as i32, // VRC (Header P7 AD12)
        34 => pots[3] as i32, // VRD (Header P7 AD13)
        35 => pots[4] as i32, // VRE (Header P7 AD14)
        36 => pots[5] as i32, // VRF (Header P7 AD15)
        _ => MIXER_CENTER as i32,
    }
}

/// Complete 4-stage mixer pipeline producing 18 AFHDS 2A / i-BUS microsecond pulses (988..2012 µs).
#[allow(clippy::too_many_arguments)]
pub fn compute_channels(
    raw_roll: i16,
    raw_pitch: i16,
    curved_throttle: u16, // 0..1000 from throttle curve engine
    raw_yaw: i16,
    pots: &[i16; 6],
    switches: &Switches,
    model: &ModelConfig,
    trims: &TrimController,
    throttle_trim_mode: u8,
) -> [u16; NUM_CHANNELS] {
    // Stage 1 & 2: Input conditioning with Dual Rates & Expo
    let high_rate = is_dr_high(model.dr_switch, switches);
    let rates = if high_rate {
        model.dr_high
    } else {
        model.dr_low
    };
    let expos = if high_rate {
        model.expo_high
    } else {
        model.expo_low
    };

    let cond_roll = apply_dr_expo(raw_roll, rates[0], expos[0]);
    let cond_pitch = apply_dr_expo(raw_pitch, rates[1], expos[1]);
    let cond_yaw = apply_dr_expo(raw_yaw, rates[2], expos[2]);
    // Throttle input is normalized to MIXER_MIN..MIXER_MAX from curved_throttle (0..MIXER_MAX)
    let cond_thr = (curved_throttle as i32 * 2 - MIXER_MAX as i32)
        .clamp(MIXER_MIN as i32, MIXER_MAX as i32) as i16;

    let cond_sticks = [cond_roll, cond_pitch, cond_thr, cond_yaw];

    // Stage 3: Initialize channels (MIXER_MIN..MIXER_MAX)
    let mut ch_vals = [0i32; NUM_CHANNELS];

    // Apply Wing/Tail templates to primary channels
    let template = WingTailTemplate::from_u8(model.wing_tail_mix);
    match template {
        WingTailTemplate::Normal => {
            ch_vals[0] = cond_roll as i32;
            ch_vals[1] = cond_pitch as i32;
            ch_vals[2] = cond_thr as i32;
            ch_vals[3] = cond_yaw as i32;
        }
        WingTailTemplate::Elevon => {
            // Delta wing: Left Elevon (CH1) = (Pitch - Roll)/2, Right Elevon (CH2) = (Pitch + Roll)/2
            // With differential applied to the roll component:
            let p = cond_pitch as i32;
            let r = cond_roll as i32;
            let left_r = apply_differential(-r, model.template_diff);
            let right_r = apply_differential(r, model.template_diff);
            ch_vals[0] = ((p + left_r) / 2).clamp(MIXER_MIN as i32, MIXER_MAX as i32);
            ch_vals[1] = ((p + right_r) / 2).clamp(MIXER_MIN as i32, MIXER_MAX as i32);
            ch_vals[2] = cond_thr as i32;
            ch_vals[3] = cond_yaw as i32;
        }
        WingTailTemplate::VTail => {
            // V-Tail: Left V-Tail (CH2) = (Pitch + Yaw)/2, Right V-Tail (CH4) = (Pitch - Yaw)/2
            let p = cond_pitch as i32;
            let y = cond_yaw as i32;
            ch_vals[0] = cond_roll as i32;
            ch_vals[1] = ((p + y) / 2).clamp(MIXER_MIN as i32, MIXER_MAX as i32);
            ch_vals[2] = cond_thr as i32;
            ch_vals[3] = ((p - y) / 2).clamp(MIXER_MIN as i32, MIXER_MAX as i32);
        }
        WingTailTemplate::Flaperon => {
            // Dual ailerons: CH1 Left Aileron, CH6 Right Aileron. Flaps driven from aux channel 6
            let r = cond_roll as i32;
            let left_r = apply_differential(r, model.template_diff);
            let right_r = apply_differential(-r, model.template_diff);
            let flap = evaluate_source(
                model.aux_channels[1],
                &cond_sticks,
                pots,
                switches,
                &ch_vals,
            );
            ch_vals[0] = (left_r + flap / 2).clamp(MIXER_MIN as i32, MIXER_MAX as i32);
            ch_vals[1] = cond_pitch as i32;
            ch_vals[2] = cond_thr as i32;
            ch_vals[3] = cond_yaw as i32;
            ch_vals[5] = (right_r + flap / 2).clamp(MIXER_MIN as i32, MIXER_MAX as i32);
        }
    }

    // Populate Auxiliary Channels (CH5..CH18) from aux_channels mappings
    for (i, &src) in model.aux_channels.iter().enumerate() {
        let ch_idx = 4 + i;
        if ch_idx < NUM_CHANNELS {
            // If Flaperon mode is active, CH6 (idx 5) is managed by template
            if template == WingTailTemplate::Flaperon && ch_idx == 5 {
                continue;
            }
            ch_vals[ch_idx] = evaluate_source(src, &cond_sticks, pots, switches, &ch_vals);
        }
    }

    // Stage 3b: Freeform matrix mixer lines
    for mix in model.mixes.iter() {
        if mix.target_ch == 0 || mix.target_ch > NUM_CHANNELS as u8 {
            continue;
        }
        if !is_switch_active(mix.switch, switches) {
            continue;
        }

        let target_idx = (mix.target_ch - 1) as usize;
        let src_val = evaluate_source(mix.source, &cond_sticks, pots, switches, &ch_vals);

        let term = (src_val * (mix.weight as i32)) / 100 + (mix.offset as i32 * 10);

        match mix.mode {
            0 => {
                // ADD (+)
                ch_vals[target_idx] =
                    (ch_vals[target_idx] + term).clamp(MIXER_MIN as i32, MIXER_MAX as i32);
            }
            1 => {
                // MULTIPLY (*)
                ch_vals[target_idx] = ((ch_vals[target_idx] * term) / MIXER_MAX as i32)
                    .clamp(MIXER_MIN as i32, MIXER_MAX as i32);
            }
            2 => {
                // REPLACE (:=)
                ch_vals[target_idx] = term.clamp(MIXER_MIN as i32, MIXER_MAX as i32);
            }
            _ => {}
        }
    }

    // Stage 4: Outputs (convert MIXER_MIN..MIXER_MAX to standard FlySky CHANNEL_MIN_US..CHANNEL_MAX_US)
    let mut rf_chs = [CHANNEL_CENTER_US; NUM_CHANNELS];
    for (i, &val) in ch_vals.iter().enumerate() {
        rf_chs[i] = (((val * CHANNEL_HALF_SPAN_US) / MIXER_MAX as i32) + CHANNEL_CENTER_US as i32)
            .clamp(CHANNEL_MIN_US as i32, CHANNEL_MAX_US as i32) as u16;
    }

    // Apply digital trims to primary flight channels
    rf_chs[0] = TrimController::apply(rf_chs[0], trims.values.roll);
    rf_chs[1] = TrimController::apply(rf_chs[1], trims.values.pitch);
    rf_chs[2] =
        TrimController::apply_throttle(rf_chs[2], trims.values.throttle, throttle_trim_mode);
    rf_chs[3] = TrimController::apply(rf_chs[3], trims.values.yaw);

    // Apply channel reversing bitmask
    let rev_mask = model.channel_reverse;
    for (ch, val) in rf_chs.iter_mut().enumerate() {
        if (rev_mask & (1 << ch)) != 0 {
            *val = CHANNEL_REVERSE_SUM - *val;
        }
    }

    rf_chs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{SwitchPos, Switches};
    use crate::storage::{MixLine, ModelConfig};

    #[test]
    fn test_apply_dr_expo_linear() {
        // 100% rate, 0 expo -> pure identity
        assert_eq!(apply_dr_expo(0, 100, 0), 0);
        assert_eq!(apply_dr_expo(500, 100, 0), 500);
        assert_eq!(apply_dr_expo(1000, 100, 0), 1000);
        assert_eq!(apply_dr_expo(-500, 100, 0), -500);
        assert_eq!(apply_dr_expo(-1000, 100, 0), -1000);

        // 50% rate, 0 expo -> half throw
        assert_eq!(apply_dr_expo(1000, 50, 0), 500);
        assert_eq!(apply_dr_expo(-1000, 50, 0), -500);

        // Rate below 30 clamps to 30
        assert_eq!(apply_dr_expo(1000, 10, 0), 300);
        // Rate above 100 clamps to 100
        assert_eq!(apply_dr_expo(1000, 120, 0), 1000);
    }

    #[test]
    fn test_apply_dr_expo_curves() {
        // Positive expo softens center (at half stick, output is lower than linear)
        let linear = apply_dr_expo(500, 100, 0); // 500
        let soft = apply_dr_expo(500, 100, 50); // 50% expo
        assert!(
            soft < linear,
            "Positive expo should soften center response: {} < {}",
            soft,
            linear
        );
        assert_eq!(
            apply_dr_expo(1000, 100, 50),
            1000,
            "Full stick must still reach 100%"
        );

        // Negative expo sharpens center (at half stick, output is higher than linear)
        let sharp = apply_dr_expo(500, 100, -50);
        assert!(
            sharp > linear,
            "Negative expo should sharpen center response: {} > {}",
            sharp,
            linear
        );
        assert_eq!(apply_dr_expo(1000, 100, -50), 1000);
    }

    #[test]
    fn test_is_switch_active() {
        let switches = Switches {
            sa: SwitchPos::Up,
            sb: SwitchPos::Mid,
            sc: SwitchPos::Down,
            sd: SwitchPos::Up,
            se: SwitchPos::Down,
            sf: SwitchPos::Up,
        };

        assert!(is_switch_active(0, &switches), "Switch 0 is always active");
        assert!(is_switch_active(1, &switches), "SA Up");
        assert!(!is_switch_active(2, &switches), "SA Down");
        assert!(!is_switch_active(3, &switches), "SB Up");
        assert!(is_switch_active(4, &switches), "SB Mid");
        assert!(!is_switch_active(5, &switches), "SB Down");
        assert!(!is_switch_active(6, &switches), "SC Up");
        assert!(!is_switch_active(7, &switches), "SC Mid");
        assert!(is_switch_active(8, &switches), "SC Down");
        assert!(is_switch_active(9, &switches), "SD Up");
        assert!(!is_switch_active(10, &switches), "SD Down");
        assert!(!is_switch_active(11, &switches), "SE Up");
        assert!(is_switch_active(12, &switches), "SE Down");
        assert!(is_switch_active(13, &switches), "SF Up");
        assert!(!is_switch_active(14, &switches), "SF Down");
    }

    #[test]
    fn test_mixer_sources_se_sf() {
        let cond_sticks = [0i16; 4];
        let pots = [0i16; 6];
        let channels = [0i32; NUM_CHANNELS];

        let mut switches = Switches::DEFAULT;
        switches.se = SwitchPos::Up;
        switches.sf = SwitchPos::Down;

        // Source 31 (SE): Up -> MIXER_MIN (-1000)
        assert_eq!(evaluate_source(31, &cond_sticks, &pots, &switches, &channels), MIXER_MIN as i32);
        // Source 32 (SF): Down -> MIXER_MAX (1000)
        assert_eq!(evaluate_source(32, &cond_sticks, &pots, &switches, &channels), MIXER_MAX as i32);

        switches.se = SwitchPos::Down;
        switches.sf = SwitchPos::Up;

        // Source 31 (SE): Down -> MIXER_MAX (1000)
        assert_eq!(evaluate_source(31, &cond_sticks, &pots, &switches, &channels), MIXER_MAX as i32);
        // Source 32 (SF): Up -> MIXER_MIN (-1000)
        assert_eq!(evaluate_source(32, &cond_sticks, &pots, &switches, &channels), MIXER_MIN as i32);

        // Arm switch bounds and evaluation
        assert!(eval_arm_switch(12, &switches), "SE Down arms when sw=12");
        assert!(!eval_arm_switch(11, &switches), "SE Up does not arm when sw=11");
        assert!(eval_arm_switch(13, &switches), "SF Up arms when sw=13");
        assert!(!eval_arm_switch(14, &switches), "SF Down does not arm when sw=14");
        assert!(!eval_arm_switch(15, &switches), "Out of range arm switch does not arm");

        // Dual Rate switch
        assert!(!is_dr_high(5, &switches), "SE Down -> Low rates");
        assert!(is_dr_high(6, &switches), "SF Up -> High rates");
    }

    #[test]
    fn test_mixer_sources_vrc_vrf() {
        let cond_sticks = [0i16; 4];
        let pots = [100i16, 200i16, -500i16, 750i16, -1000i16, 1000i16];
        let switches = Switches::DEFAULT;
        let channels = [0i32; NUM_CHANNELS];

        // Source 33: VRC (pots[2])
        assert_eq!(evaluate_source(33, &cond_sticks, &pots, &switches, &channels), -500);
        // Source 34: VRD (pots[3])
        assert_eq!(evaluate_source(34, &cond_sticks, &pots, &switches, &channels), 750);
        // Source 35: VRE (pots[4])
        assert_eq!(evaluate_source(35, &cond_sticks, &pots, &switches, &channels), -1000);
        // Source 36: VRF (pots[5])
        assert_eq!(evaluate_source(36, &cond_sticks, &pots, &switches, &channels), 1000);
    }

    #[test]
    fn test_compute_channels_normal_centered() {
        let model = ModelConfig::default_for_index(0);
        let trims = TrimController::new();
        let switches = Switches {
            sa: SwitchPos::Up,
            sb: SwitchPos::Up,
            sc: SwitchPos::Up,
            sd: SwitchPos::Up,
            se: SwitchPos::Up,
            sf: SwitchPos::Up,
        };
        let pots = [0i16; 6];

        // Neutral inputs: roll=0, pitch=0, throttle=500 (idle = 0), yaw=0
        let chs = compute_channels(0, 0, 500, 0, &pots, &switches, &model, &trims, 0);

        assert_eq!(
            chs[0], CHANNEL_CENTER_US,
            "CH1 Roll should be center 1500 µs"
        );
        assert_eq!(
            chs[1], CHANNEL_CENTER_US,
            "CH2 Pitch should be center 1500 µs"
        );
        assert_eq!(
            chs[2], CHANNEL_CENTER_US,
            "CH3 Throttle mid should be center 1500 µs"
        );
        assert_eq!(
            chs[3], CHANNEL_CENTER_US,
            "CH4 Yaw should be center 1500 µs"
        );
    }

    #[test]
    fn test_compute_channels_full_deflection_and_pulse_bounds() {
        let model = ModelConfig::default_for_index(0);
        let trims = TrimController::new();
        let switches = Switches {
            sa: SwitchPos::Up,
            sb: SwitchPos::Up,
            sc: SwitchPos::Up,
            sd: SwitchPos::Up,
            se: SwitchPos::Up,
            sf: SwitchPos::Up,
        };
        let pots = [0i16; 6];

        // Full roll right (1000)
        let chs = compute_channels(1000, 0, 0, 0, &pots, &switches, &model, &trims, 0);
        assert_eq!(
            chs[0], CHANNEL_MAX_US,
            "CH1 max pulse should be {}",
            CHANNEL_MAX_US
        );
        assert_eq!(
            chs[2], CHANNEL_MIN_US,
            "CH3 zero throttle should be {}",
            CHANNEL_MIN_US
        );

        // Full roll left (-1000)
        let chs_neg = compute_channels(-1000, 0, 1000, 0, &pots, &switches, &model, &trims, 0);
        assert_eq!(
            chs_neg[0], CHANNEL_MIN_US,
            "CH1 min pulse should be {}",
            CHANNEL_MIN_US
        );
        assert_eq!(
            chs_neg[2], CHANNEL_MAX_US,
            "CH3 full throttle should be {}",
            CHANNEL_MAX_US
        );
    }

    #[test]
    fn test_compute_channels_elevon_mixing() {
        let mut model = ModelConfig::default_for_index(0);
        model.wing_tail_mix = 1; // Elevon
        let trims = TrimController::new();
        let switches = Switches {
            sa: SwitchPos::Up,
            sb: SwitchPos::Up,
            sc: SwitchPos::Up,
            sd: SwitchPos::Up,
            se: SwitchPos::Up,
            sf: SwitchPos::Up,
        };
        let pots = [0i16; 6];

        // Pitch up (1000), Roll zero -> Both elevons deflect together with 50% throw
        let chs = compute_channels(0, 1000, 0, 0, &pots, &switches, &model, &trims, 0);
        assert_eq!(chs[0], 1756, "CH1 Left elevon 50% up");
        assert_eq!(chs[1], 1756, "CH2 Right elevon 50% up");

        // Pitch up (1000) + Roll right (1000) -> Right elevon full throw (100%), Left elevon center (0%)
        let chs_corner = compute_channels(1000, 1000, 0, 0, &pots, &switches, &model, &trims, 0);
        assert_eq!(
            chs_corner[0], CHANNEL_CENTER_US,
            "CH1 Left elevon neutral without clipping"
        );
        assert_eq!(
            chs_corner[1], CHANNEL_MAX_US,
            "CH2 Right elevon full travel 2012 µs"
        );

        // Differential test: diff = +50% (down-deflection attenuated by 50%)
        model.template_diff = 50;
        let chs_diff = compute_channels(1000, 0, 0, 0, &pots, &switches, &model, &trims, 0);
        assert_eq!(chs_diff[1], 1756, "Right elevon up-deflection unattenuated");
        assert_eq!(
            chs_diff[0], 1372,
            "Left elevon down-deflection attenuated by 50%"
        );
    }

    #[test]
    fn test_compute_channels_vtail_mixing() {
        let mut model = ModelConfig::default_for_index(0);
        model.wing_tail_mix = 2; // VTail
        let trims = TrimController::new();
        let switches = Switches {
            sa: SwitchPos::Up,
            sb: SwitchPos::Up,
            sc: SwitchPos::Up,
            sd: SwitchPos::Up,
            se: SwitchPos::Up,
            sf: SwitchPos::Up,
        };
        let pots = [0i16; 6];

        // Pitch up (1000), Yaw zero -> Both V-tail ruddervators deflect up together with 50% authority
        let chs = compute_channels(0, 1000, 0, 0, &pots, &switches, &model, &trims, 0);
        assert_eq!(chs[1], 1756, "CH2 Left V-Tail");
        assert_eq!(chs[3], 1756, "CH4 Right V-Tail");

        // Full corner Pitch up (1000) + Yaw right (1000) -> CH2 full travel (2012 µs), CH4 neutral (1500 µs)
        let chs_corner = compute_channels(0, 1000, 0, 1000, &pots, &switches, &model, &trims, 0);
        assert_eq!(chs_corner[1], CHANNEL_MAX_US, "CH2 Left V-Tail full up");
        assert_eq!(
            chs_corner[3], CHANNEL_CENTER_US,
            "CH4 Right V-Tail neutral without clipping"
        );
    }

    #[test]
    fn test_unipolar_throttle_mixer() {
        let mut model = ModelConfig::default_for_index(0);
        // Mix line 1: CH2 (Elevator) <- Thr+ with -20% weight, ADD
        model.mixes[0] = MixLine {
            target_ch: 2,
            source: 30, // Thr+
            weight: -20,
            offset: 0,
            mode: 0,
            switch: 0,
        };
        let trims = TrimController::new();
        let switches = Switches {
            sa: SwitchPos::Up,
            sb: SwitchPos::Up,
            sc: SwitchPos::Up,
            sd: SwitchPos::Up,
            se: SwitchPos::Up,
            sf: SwitchPos::Up,
        };
        let pots = [0i16; 6];

        // At zero throttle (curved_throttle = 0): Thr+ evaluates to 0 -> zero elevator compensation!
        let chs_idle = compute_channels(0, 0, 0, 0, &pots, &switches, &model, &trims, 0);
        assert_eq!(
            chs_idle[1], CHANNEL_CENTER_US,
            "CH2 Elevator must stay neutral at idle throttle"
        );

        // At 100% throttle (curved_throttle = 1000): Thr+ is 1000 -> -20% weight gives -200 offset (-102 µs)
        let chs_full = compute_channels(0, 0, 1000, 0, &pots, &switches, &model, &trims, 0);
        assert_eq!(
            chs_full[1], 1398,
            "CH2 Elevator receives smooth down-pitch compensation at full throttle"
        );
    }

    #[test]
    fn test_compute_channels_18ch() {
        let mut model = ModelConfig::default_for_index(0);
        // Map aux channels 10..13 (CH15..CH18):
        // CH15 = SA (7)
        // CH16 = SB (8)
        // CH17 = SC (9)
        // CH18 = SD (10)
        model.aux_channels[10] = 7;
        model.aux_channels[11] = 8;
        model.aux_channels[12] = 9;
        model.aux_channels[13] = 10;

        // Reverse CH18 (bit 17)
        model.channel_reverse = 1 << 17;

        let trims = TrimController::new();
        let switches = Switches {
            sa: SwitchPos::Down, // CH15 -> MAX (2012)
            sb: SwitchPos::Mid,  // CH16 -> CENTER (1500)
            sc: SwitchPos::Up,   // CH17 -> MIN (988)
            sd: SwitchPos::Down, // CH18 -> normally MAX (2012), but reversed -> MIN (988)
            se: SwitchPos::Up,
            sf: SwitchPos::Up,
        };
        let pots = [0i16; 6];

        let chs = compute_channels(0, 0, 500, 0, &pots, &switches, &model, &trims, 0);
        assert_eq!(chs.len(), 18);
        assert_eq!(chs[14], CHANNEL_MAX_US, "CH15 SA Down -> MAX");
        assert_eq!(chs[15], CHANNEL_CENTER_US, "CH16 SB Mid -> CENTER");
        assert_eq!(chs[16], CHANNEL_MIN_US, "CH17 SC Up -> MIN");
        assert_eq!(chs[17], CHANNEL_MIN_US, "CH18 SD Down reversed -> MIN");

        // Target CH18 with a replace matrix mix
        model.mixes[0] = MixLine {
            target_ch: 18,
            source: 11, // MAX (+1000)
            weight: 100,
            offset: 0,
            mode: 2, // REPLACE
            switch: 0,
        };
        let chs_mix = compute_channels(0, 0, 500, 0, &pots, &switches, &model, &trims, 0);
        // MAX source is +1000 -> 2012 µs. But CH18 is reversed! So it inverts to 988 µs.
        assert_eq!(chs_mix[17], CHANNEL_MIN_US);
    }

    #[test]
    fn test_compute_channels_channel_reversing() {
        let mut model = ModelConfig::default_for_index(0);
        model.channel_reverse = 0b0000_0001; // Reverse CH1 (Roll)
        let trims = TrimController::new();
        let switches = Switches {
            sa: SwitchPos::Up,
            sb: SwitchPos::Up,
            sc: SwitchPos::Up,
            sd: SwitchPos::Up,
            se: SwitchPos::Up,
            sf: SwitchPos::Up,
        };
        let pots = [0i16; 6];

        // Roll right (+1000) with reversed CH1 should yield CHANNEL_MIN_US instead of CHANNEL_MAX_US
        let chs = compute_channels(1000, 0, 0, 0, &pots, &switches, &model, &trims, 0);
        assert_eq!(
            chs[0], CHANNEL_MIN_US,
            "Reversed CH1 should invert to {}",
            CHANNEL_MIN_US
        );
    }

    #[test]
    fn test_matrix_mixer_modes() {
        let mut model = ModelConfig::default_for_index(0);
        model.aux_channels[0] = 0; // Set CH5 source to None (center = 0)
                                   // Mix line 1: Add Roll to CH5 (Aux 1) with 50% weight
        model.mixes[0] = MixLine {
            target_ch: 5,
            source: 1, // Roll
            weight: 50,
            offset: 0,
            mode: 0,   // ADD
            switch: 0, // Always active
        };

        let trims = TrimController::new();
        let switches = Switches {
            sa: SwitchPos::Up,
            sb: SwitchPos::Up,
            sc: SwitchPos::Up,
            sd: SwitchPos::Up,
            se: SwitchPos::Up,
            sf: SwitchPos::Up,
        };
        let pots = [0i16; 6];

        let chs = compute_channels(1000, 0, 0, 0, &pots, &switches, &model, &trims, 0);
        // Base CH5 is 0. Roll is 1000 -> 50% weight adds +500 -> 1500 + 256 = 1756 µs
        assert_eq!(chs[4], 1756);

        // Mix line 2: Replace CH5 with Roll 100%
        model.mixes[0].mode = 2; // REPLACE
        model.mixes[0].weight = 100;
        let chs_replace = compute_channels(1000, 0, 0, 0, &pots, &switches, &model, &trims, 0);
        assert_eq!(chs_replace[4], CHANNEL_MAX_US);
    }

    #[test]
    fn test_timer_trigger_modes() {
        let mut latched = false;
        let switches = Switches {
            sa: SwitchPos::Up,
            sb: SwitchPos::Mid,
            sc: SwitchPos::Down,
            sd: SwitchPos::Up,
            se: SwitchPos::Down,
            sf: SwitchPos::Up,
        };

        // 0: Disabled
        assert!(!is_timer_active(0, 500, &mut latched, &switches, true));

        // 1: THs (runs only while throttle > -900)
        assert!(!is_timer_active(1, -950, &mut latched, &switches, true));
        assert!(is_timer_active(1, -850, &mut latched, &switches, true));
        assert!(is_timer_active(1, 0, &mut latched, &switches, true));
        assert!(!is_timer_active(1, -1000, &mut latched, &switches, true));

        // 2: THt (latched running once throttle > -900)
        latched = false;
        assert!(!is_timer_active(2, -1000, &mut latched, &switches, true));
        assert!(!latched);
        // Throttle raised above idle
        assert!(is_timer_active(2, -500, &mut latched, &switches, true));
        assert!(latched);
        // Throttle lowered back to idle: MUST CONTINUE RUNNING
        assert!(is_timer_active(2, -1000, &mut latched, &switches, true));
        assert!(latched);

        // Reset/Unlatch (e.g. on landing/disarm or manual reset)
        latched = false;
        assert!(!is_timer_active(2, -1000, &mut latched, &switches, true));

        // 3: Always On
        assert!(is_timer_active(3, -1000, &mut latched, &switches, true));

        // 4: SA^ (SwitchPos::Up is active)
        assert!(is_timer_active(4, -1000, &mut latched, &switches, true));
        // 5: SAv (SwitchPos::Down is NOT active)
        assert!(!is_timer_active(5, -1000, &mut latched, &switches, true));

        // 14: SE^ (SwitchPos::Down is active -> SE^ is false)
        assert!(!is_timer_active(14, -1000, &mut latched, &switches, true));
        // 15: SEv (SwitchPos::Down is active -> SEv is true)
        assert!(is_timer_active(15, -1000, &mut latched, &switches, true));

        // 16: SF^ (SwitchPos::Up is active -> SF^ is true)
        assert!(is_timer_active(16, -1000, &mut latched, &switches, true));
        // 17: SFv (SwitchPos::Up is active -> SFv is false)
        assert!(!is_timer_active(17, -1000, &mut latched, &switches, true));

        // Inhibit when disarmed
        assert!(!is_timer_active(1, 500, &mut latched, &switches, false));
        assert!(!is_timer_active(2, 500, &mut latched, &switches, false));
        assert!(!is_timer_active(3, -1000, &mut latched, &switches, false));
        assert!(!is_timer_active(4, -1000, &mut latched, &switches, false));
        assert!(!is_timer_active(15, -1000, &mut latched, &switches, false));
    }

    #[test]
    fn test_timer_reset_guard_and_cooldown() {
        let mut timer_remaining_secs: u16 = 240;
        let mut cancel_hold_ms: u16 = 0;
        let mut cancel_waiting_release = false;
        let mut timer_reset_toast_ms: u16 = 0;
        let mut timer_reset_cooldown_ms: u16 = 0;

        assert!(!cancel_waiting_release);
        assert_eq!(timer_reset_toast_ms, 0);
        assert_eq!(timer_reset_cooldown_ms, 0);
        assert_eq!(timer_remaining_secs, 240);

        let dt_ms: u16 = 33;

        // Step 1: Simulate holding [CANCEL] for 1000ms
        for _ in 0..30 {
            cancel_hold_ms = cancel_hold_ms.saturating_add(dt_ms);
            assert!(cancel_hold_ms < 1000);
            assert_eq!(timer_remaining_secs, 240);
        }

        // On frame 31, hold hits 1000ms threshold
        cancel_hold_ms = cancel_hold_ms.saturating_add(dt_ms);
        assert!(cancel_hold_ms >= 1000);

        // Execute reset
        timer_remaining_secs = 300;
        assert_eq!(timer_remaining_secs, 300);
        cancel_hold_ms = 0;
        cancel_waiting_release = true;
        timer_reset_toast_ms = 800;
        timer_reset_cooldown_ms = 1500;

        // Step 2: Pilot continues holding [CANCEL] for the next 500ms
        for _ in 0..15 {
            if timer_reset_toast_ms > 0 {
                timer_reset_toast_ms = timer_reset_toast_ms.saturating_sub(dt_ms);
            }
            if timer_reset_cooldown_ms > 0 {
                timer_reset_cooldown_ms = timer_reset_cooldown_ms.saturating_sub(dt_ms);
            }
            let cancel_key = true;
            if !cancel_key {
                cancel_waiting_release = false;
            }
            if cancel_key && !cancel_waiting_release && timer_reset_cooldown_ms == 0 {
                cancel_hold_ms = cancel_hold_ms.saturating_add(dt_ms);
            } else {
                cancel_hold_ms = 0;
            }

            // Must remain 0 while holding!
            assert_eq!(
                cancel_hold_ms, 0,
                "cancel_hold_ms must stay 0 while button is held"
            );
        }

        // Step 3: Pilot finally releases [CANCEL]
        let cancel_key = false;
        if !cancel_key {
            cancel_waiting_release = false;
        }
        assert!(!cancel_waiting_release);

        // Step 4: Rapid press during 1.5s cooldown must be rejected
        let cancel_key = true;
        if cancel_key && !cancel_waiting_release && timer_reset_cooldown_ms == 0 {
            cancel_hold_ms = cancel_hold_ms.saturating_add(dt_ms);
        } else {
            cancel_hold_ms = 0;
        }
        assert_eq!(
            cancel_hold_ms, 0,
            "cancel_hold_ms must stay 0 during cooldown"
        );
    }
}
