//! Curve interpolation engine for throttle and pitch curves.
//! Supports switchable 5-point and 9-point curves with linear or Catmull-Rom cubic spline smoothing.

/// Evaluate a 5-point or 9-point throttle curve with optional Catmull-Rom spline smoothing.
/// - `input`: raw stick axis (0 .. 1000, where 0 is idle/bottom, 1000 is 100% full throttle)
/// - `pts_mode`: 5 or 9
/// - `smooth`: false (linear interpolation) or true (Catmull-Rom cubic spline)
/// - `curve`: array of 9 points (0..100)
///
/// Returns output pulse percentage (0 .. 1000).
pub fn evaluate_curve(input: u16, pts_mode: u8, smooth: bool, curve: &[u8; 9]) -> u16 {
    let x = input.min(1000) as i32;

    if pts_mode == 9 {
        evaluate_9pt(x, smooth, curve)
    } else {
        evaluate_5pt(x, smooth, curve)
    }
}

fn evaluate_5pt(x: i32, smooth: bool, curve: &[u8; 9]) -> u16 {
    let pts: [i32; 5] = [
        curve[0] as i32 * 10,
        curve[1] as i32 * 10,
        curve[2] as i32 * 10,
        curve[3] as i32 * 10,
        curve[4] as i32 * 10,
    ];

    if x >= 1000 {
        return pts[4].clamp(0, 1000) as u16;
    }
    if x <= 0 {
        return pts[0].clamp(0, 1000) as u16;
    }

    let i = (x / 250).min(3) as usize;
    let dx = x - (i as i32 * 250);

    if !smooth {
        // Linear interpolation
        let y = pts[i] + ((pts[i + 1] - pts[i]) * dx) / 250;
        return y.clamp(0, 1000) as u16;
    }

    // Catmull-Rom spline interpolation with boundary tangent extrapolation
    let p0 = if i > 0 { pts[i - 1] } else { 2 * pts[0] - pts[1] };
    let p1 = pts[i];
    let p2 = pts[i + 1];
    let p3 = if i + 2 < 5 { pts[i + 2] } else { 2 * pts[4] - pts[3] };
    let t = (dx * 256) / 250;

    catmull_rom(p0, p1, p2, p3, t)
}

fn evaluate_9pt(x: i32, smooth: bool, curve: &[u8; 9]) -> u16 {
    let pts: [i32; 9] = [
        curve[0] as i32 * 10,
        curve[1] as i32 * 10,
        curve[2] as i32 * 10,
        curve[3] as i32 * 10,
        curve[4] as i32 * 10,
        curve[5] as i32 * 10,
        curve[6] as i32 * 10,
        curve[7] as i32 * 10,
        curve[8] as i32 * 10,
    ];

    if x >= 1000 {
        return pts[8].clamp(0, 1000) as u16;
    }
    if x <= 0 {
        return pts[0].clamp(0, 1000) as u16;
    }

    let i = (x / 125).min(7) as usize;
    let dx = x - (i as i32 * 125);

    if !smooth {
        let y = pts[i] + ((pts[i + 1] - pts[i]) * dx) / 125;
        return y.clamp(0, 1000) as u16;
    }

    let p0 = if i > 0 { pts[i - 1] } else { 2 * pts[0] - pts[1] };
    let p1 = pts[i];
    let p2 = pts[i + 1];
    let p3 = if i + 2 < 9 { pts[i + 2] } else { 2 * pts[8] - pts[7] };
    let t = (dx * 256) / 125;

    catmull_rom(p0, p1, p2, p3, t)
}

#[inline]
fn catmull_rom(p0: i32, p1: i32, p2: i32, p3: i32, t: i32) -> u16 {
    let t2 = (t * t) / 256;
    let t3 = (t2 * t) / 256;

    let h00 = 2 * t3 - 3 * t2 + 256;
    let h10 = t3 - 2 * t2 + t;
    let h01 = -2 * t3 + 3 * t2;
    let h11 = t3 - t2;

    let m1 = (p2 - p0) / 2;
    let m2 = (p3 - p1) / 2;

    let y = (h00 * p1 + h01 * p2 + h10 * m1 + h11 * m2) / 256;
    y.clamp(0, 1000) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_5pt_linear_identity() {
        let curve = [0, 25, 50, 75, 100, 0, 0, 0, 0];
        assert_eq!(evaluate_curve(0, 5, false, &curve), 0);
        assert_eq!(evaluate_curve(250, 5, false, &curve), 250);
        assert_eq!(evaluate_curve(500, 5, false, &curve), 500);
        assert_eq!(evaluate_curve(750, 5, false, &curve), 750);
        assert_eq!(evaluate_curve(1000, 5, false, &curve), 1000);
        // Interpolated mid-point
        assert_eq!(evaluate_curve(125, 5, false, &curve), 125);
        assert_eq!(evaluate_curve(375, 5, false, &curve), 375);
    }

    #[test]
    fn test_5pt_linear_flat_and_inverted() {
        let flat = [50, 50, 50, 50, 50, 0, 0, 0, 0];
        assert_eq!(evaluate_curve(0, 5, false, &flat), 500);
        assert_eq!(evaluate_curve(500, 5, false, &flat), 500);
        assert_eq!(evaluate_curve(1000, 5, false, &flat), 500);

        let inverted = [100, 75, 50, 25, 0, 0, 0, 0, 0];
        assert_eq!(evaluate_curve(0, 5, false, &inverted), 1000);
        assert_eq!(evaluate_curve(250, 5, false, &inverted), 750);
        assert_eq!(evaluate_curve(500, 5, false, &inverted), 500);
        assert_eq!(evaluate_curve(750, 5, false, &inverted), 250);
        assert_eq!(evaluate_curve(1000, 5, false, &inverted), 0);
    }

    #[test]
    fn test_5pt_spline_endpoints_and_monotonicity() {
        let curve = [0, 25, 50, 75, 100, 0, 0, 0, 0];
        assert_eq!(evaluate_curve(0, 5, true, &curve), 0);
        assert_eq!(evaluate_curve(500, 5, true, &curve), 500);
        assert_eq!(evaluate_curve(1000, 5, true, &curve), 1000);

        // Verify smooth progression
        let mut prev = 0;
        for x in (50..=1000).step_by(50) {
            let val = evaluate_curve(x, 5, true, &curve);
            assert!(val >= prev, "Spline must be monotonically non-decreasing at {}", x);
            prev = val;
        }
    }

    #[test]
    fn test_9pt_linear_and_spline() {
        let curve = [0, 12, 25, 37, 50, 62, 75, 87, 100];
        assert_eq!(evaluate_curve(0, 9, false, &curve), 0);
        assert_eq!(evaluate_curve(500, 9, false, &curve), 500);
        assert_eq!(evaluate_curve(1000, 9, false, &curve), 1000);

        assert_eq!(evaluate_curve(0, 9, true, &curve), 0);
        assert_eq!(evaluate_curve(500, 9, true, &curve), 500);
        assert_eq!(evaluate_curve(1000, 9, true, &curve), 1000);
    }

    #[test]
    fn test_out_of_bounds_clamping() {
        let curve5 = [0, 25, 50, 75, 100, 0, 0, 0, 0];
        assert_eq!(evaluate_curve(1500, 5, false, &curve5), 1000);
        assert_eq!(evaluate_curve(1500, 5, true, &curve5), 1000);

        let curve9 = [0, 12, 25, 37, 50, 62, 75, 87, 100];
        assert_eq!(evaluate_curve(1500, 9, false, &curve9), 1000);
        assert_eq!(evaluate_curve(1500, 9, true, &curve9), 1000);
    }
}
