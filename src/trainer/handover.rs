//! Safety Handover Controller for buddy-box training.
//!
//! Enables an instructor (Master) to safely yield flight control to a student (Trainee)
//! while guaranteeing immediate, deterministic pilot recovery:
//! - **Instant Stick Deflection Takeover**: If the instructor deflects Roll, Pitch, or Yaw
//!   sticks by more than 5% (> 50 µs from center 1500 µs) or advances Throttle by > 5% (> 1050 µs),
//!   the instructor instantly reclaims master authority.
//! - **Signal Loss Fallback**: If the student's link fails or times out, control reverts to the
//!   instructor immediately (0 ms glitch-free transition).
//! - **Auxiliary Channel Retention**: Auxiliary switches, flight modes, and arming remain securely
//!   anchored to the instructor's transmitter.

pub const TAKEOVER_THRESHOLD_US: u16 = 50; // 5% of 1000 µs span
pub const THROTTLE_TAKEOVER_MIN_US: u16 = 1050; // 5% above 1000 µs idle

/// Current operational state of the trainer link.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TrainerState {
    /// Trainer function is inactive or switch is OFF; instructor has sole control.
    Disabled,
    /// Trainer switch is active, student signal is healthy, and student is actively flying.
    StudentActive,
    /// Instructor has overridden student control due to stick deflection (> 5%).
    InstructorOverride,
    /// Student signal lost, unlinked, or timed out; safe fallback to instructor.
    SignalLost,
}

/// Safety Handover Controller.
#[derive(Copy, Clone, Debug)]
pub struct SafetyHandover {
    pub state: TrainerState,
}

impl Default for SafetyHandover {
    fn default() -> Self {
        Self::new()
    }
}

impl SafetyHandover {
    pub const fn new() -> Self {
        Self {
            state: TrainerState::Disabled,
        }
    }

    /// Checks whether the instructor sticks have deflected beyond the safe handover threshold.
    ///
    /// Evaluates the primary 4 flight controls:
    /// - Roll (index 0): center 1500 µs, threshold ±50 µs
    /// - Pitch (index 1): center 1500 µs, threshold ±50 µs
    /// - Throttle (index 2): idle ~1000 µs, threshold > 1050 µs
    /// - Yaw (index 3): center 1500 µs, threshold ±50 µs
    pub fn is_instructor_deflected(instructor_chs: &[u16]) -> bool {
        if instructor_chs.len() < 4 {
            return true;
        }

        // Roll (CH1) deflection > 5%
        let roll_diff = (instructor_chs[0] as i32 - 1500).abs();
        if roll_diff > TAKEOVER_THRESHOLD_US as i32 {
            return true;
        }

        // Pitch (CH2) deflection > 5%
        let pitch_diff = (instructor_chs[1] as i32 - 1500).abs();
        if pitch_diff > TAKEOVER_THRESHOLD_US as i32 {
            return true;
        }

        // Throttle (CH3) raised > 5% above idle
        if instructor_chs[2] > THROTTLE_TAKEOVER_MIN_US {
            return true;
        }

        // Yaw (CH4) deflection > 5%
        let yaw_diff = (instructor_chs[3] as i32 - 1500).abs();
        if yaw_diff > TAKEOVER_THRESHOLD_US as i32 {
            return true;
        }

        false
    }

    /// Resolve effective channel outputs between instructor and student.
    ///
    /// Arguments:
    /// - `trainer_switch_active`: whether trainer switch (e.g. SWD momentary or toggle) is engaged.
    /// - `student_chs`: valid student channels if signal is active, or `None` if unlinked.
    /// - `instructor_chs`: instructor channels computed from physical sticks & trims.
    /// - `output_chs`: destination slice for final transmission channels.
    pub fn process(
        &mut self,
        trainer_switch_active: bool,
        student_chs: Option<&[u16; 8]>,
        instructor_chs: &[u16],
        output_chs: &mut [u16],
    ) {
        // Copy instructor channels by default
        let count = output_chs.len().min(instructor_chs.len());
        output_chs[..count].copy_from_slice(&instructor_chs[..count]);

        if !trainer_switch_active {
            self.state = TrainerState::Disabled;
            return;
        }

        let s_chs = match student_chs {
            Some(s) => s,
            None => {
                self.state = TrainerState::SignalLost;
                return;
            }
        };

        // If the instructor moves any stick beyond threshold, instructor takes over instantly
        if Self::is_instructor_deflected(instructor_chs) {
            self.state = TrainerState::InstructorOverride;
            return;
        }

        // Student has control of primary 4 flight channels
        self.state = TrainerState::StudentActive;
        let replace_count = output_chs.len().min(4).min(s_chs.len());
        output_chs[..replace_count].copy_from_slice(&s_chs[..replace_count]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instructor_deflection_detection() {
        // Centered sticks (mode 2: roll=1500, pitch=1500, thr=1000, yaw=1500)
        let centered = [1500, 1500, 1000, 1500];
        assert!(!SafetyHandover::is_instructor_deflected(&centered));

        // Slight stick drift within deadband (30 µs deflection < 50 µs threshold)
        let minor_drift = [1530, 1480, 1030, 1520];
        assert!(!SafetyHandover::is_instructor_deflected(&minor_drift));

        // Roll stick deflection (> 50 µs)
        let roll_takeover = [1560, 1500, 1000, 1500];
        assert!(SafetyHandover::is_instructor_deflected(&roll_takeover));

        // Pitch stick deflection
        let pitch_takeover = [1500, 1440, 1000, 1500];
        assert!(SafetyHandover::is_instructor_deflected(&pitch_takeover));

        // Throttle raised (> 1050 µs)
        let thr_takeover = [1500, 1500, 1100, 1500];
        assert!(SafetyHandover::is_instructor_deflected(&thr_takeover));

        // Yaw deflection
        let yaw_takeover = [1500, 1500, 1000, 1560];
        assert!(SafetyHandover::is_instructor_deflected(&yaw_takeover));
    }

    #[test]
    fn test_handover_state_transitions() {
        let mut handover = SafetyHandover::new();
        let instructor_idle = [1500, 1500, 1000, 1500, 1500, 1500, 1500, 1500];
        let student_input = [1600, 1400, 1300, 1550, 1500, 1500, 1500, 1500];
        let mut out = [0u16; 8];

        // 1. Trainer switch OFF -> Instructor has control
        handover.process(false, Some(&student_input), &instructor_idle, &mut out);
        assert_eq!(handover.state, TrainerState::Disabled);
        assert_eq!(out, instructor_idle);

        // 2. Trainer switch ON + signal valid + instructor idle -> Student in control
        handover.process(true, Some(&student_input), &instructor_idle, &mut out);
        assert_eq!(handover.state, TrainerState::StudentActive);
        assert_eq!(out[0..4], student_input[0..4]);
        assert_eq!(out[4..8], instructor_idle[4..8]); // Aux channels preserved

        // 3. Instructor deflects stick -> Instant override
        let instructor_intervene = [1600, 1500, 1000, 1500, 1500, 1500, 1500, 1500];
        handover.process(true, Some(&student_input), &instructor_intervene, &mut out);
        assert_eq!(handover.state, TrainerState::InstructorOverride);
        assert_eq!(out, instructor_intervene);

        // 4. Student signal lost -> Fallback to instructor
        handover.process(true, None, &instructor_idle, &mut out);
        assert_eq!(handover.state, TrainerState::SignalLost);
        assert_eq!(out, instructor_idle);
    }
}
