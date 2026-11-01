//! The curves the sidebar slides along: how far along its way, from 0 to 1, at a fraction of its
//! duration, from 0 to 1.

/// Fast, then slowing down to a stop, without going past it.
pub fn ease_out(t: f64) -> f64 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(4)
}

/// `t` of the way from `from` to `to`, to the nearest point.
pub fn lerp(from: i32, to: i32, t: f64) -> i32 {
    (f64::from(from) + f64::from(to - from) * t).round() as i32
}

#[cfg(test)]
mod tests {
    use super::{ease_out, lerp};

    fn samples(curve: fn(f64) -> f64) -> Vec<f64> {
        (0..=100).map(|i| curve(f64::from(i) / 100.0)).collect()
    }

    #[test]
    fn eases_out_from_start_to_end_without_overshoot() {
        assert_eq!(ease_out(0.0), 0.0);
        assert_eq!(ease_out(1.0), 1.0);
        let values = samples(ease_out);
        assert!(values.windows(2).all(|pair| pair[0] <= pair[1]));
        assert!(values.iter().all(|&value| value <= 1.0));
    }

    #[test]
    fn eases_out_strongly() {
        assert!(ease_out(0.25) > 0.65);
        assert!(ease_out(0.5) > 0.9);
    }

    #[test]
    fn interpolates_to_the_nearest_point() {
        assert_eq!(lerp(269, 9, 0.0), 269);
        assert_eq!(lerp(269, 9, 0.5), 139);
        assert_eq!(lerp(269, 9, 1.0), 9);
        assert_eq!(lerp(0, 3, 0.5), 2);
    }
}
