//! Speed variation with caller-supplied randomness.
//! Each second selects a threshold from 40, 50, 60, 180, 200, 240 and 300.
//! When threshold >= elapsed seconds modulo 300, clamp U to 0.4..0.8;
//! otherwise clamp U + 0.2 to 0.8..1.2. This biases early ticks toward slower speeds.
//! Bearing and altitude offsets use independent random magnitudes and signs.

/// Thresholds used to select the speed multiplier interval.
const THRESHOLDS: [u32; 7] = [40, 50, 60, 180, 200, 240, 300];
const SLOW_MAX: f64 = 0.8;
const SLOW_MIN: f64 = 0.4;
const FAST_MAX: f64 = 1.2;
const FAST_MIN: f64 = 0.8;

#[derive(Clone, Debug)]
pub struct Jitter {
    enabled: bool,
    /// Generated multipliers indexed by elapsed whole seconds.
    factors: Vec<f64>,
}

impl Jitter {
    pub fn new(enabled: bool) -> Self {
        Self { enabled, factors: Vec::new() }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Current multiplier, or 1 when variation is disabled.
    pub fn factor(&self) -> f64 {
        if !self.enabled {
            return 1.0;
        }
        self.factors.last().copied().unwrap_or(1.0)
    }

    /// Generate one whole second's multiplier. Each random call returns a value in [0, 1).
    pub fn advance<F: FnMut() -> f64>(&mut self, seconds: u64, mut random: F) {
        if !self.enabled {
            return;
        }
        let threshold =
            THRESHOLDS[(random() * THRESHOLDS.len() as f64) as usize % THRESHOLDS.len()];
        let u = random().clamp(0.0, 1.0);
        let phase = (seconds % 300) as u32;
        let factor = if threshold >= phase {
            u.max(SLOW_MIN).min(SLOW_MAX)
        } else {
            (u + 0.2).max(FAST_MIN).min(FAST_MAX)
        };
        self.factors.push(factor);
    }

    /// Random signed bearing offset of up to 15 degrees.
    pub fn bearing_offset<F: FnMut() -> f64>(&self, mut random: F) -> f64 {
        if !self.enabled {
            return 0.0;
        }
        let u = random().clamp(0.0, 1.0);
        if random() < 0.5 { -u * 15.0 } else { u * 15.0 }
    }

    /// Random signed altitude offset of up to 3 meters.
    pub fn altitude_offset<F: FnMut() -> f64>(&self, mut random: F) -> f64 {
        if !self.enabled {
            return 0.0;
        }
        let u = random().clamp(0.0, 1.0);
        if random() < 0.5 { -u * 3.0 } else { u * 3.0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sequence(values: Vec<f64>) -> impl FnMut() -> f64 {
        let mut index = 0;
        move || {
            let value = values[index.min(values.len() - 1)];
            index += 1;
            value
        }
    }

    #[test]
    fn disabled_jitter_leaves_the_speed_alone() {
        let mut jitter = Jitter::new(false);
        jitter.advance(5, sequence(vec![0.0, 0.0]));
        assert_eq!(jitter.factor(), 1.0);
        assert_eq!(jitter.bearing_offset(sequence(vec![1.0, 0.0])), 0.0);
        assert_eq!(jitter.altitude_offset(sequence(vec![1.0, 0.0])), 0.0);
    }

    #[test]
    fn the_second_after_the_start_always_takes_the_slow_branch() {
        // At t modulo 300 == 0, every threshold selects the lower interval.
        for threshold_pick in [0.0, 0.5, 0.99] {
            let mut jitter = Jitter::new(true);
            jitter.advance(0, sequence(vec![threshold_pick, 0.9]));
            assert_eq!(jitter.factor(), SLOW_MAX, "threshold pick {threshold_pick}");
        }
        let mut jitter = Jitter::new(true);
        jitter.advance(0, sequence(vec![0.0, 0.1]));
        assert_eq!(jitter.factor(), SLOW_MIN);
    }

    #[test]
    fn a_late_second_with_a_small_threshold_takes_the_fast_branch() {
        // Threshold 40 at t=100 selects the upper interval: 0.5 + 0.2 clamps to 0.8.
        let mut jitter = Jitter::new(true);
        jitter.advance(100, sequence(vec![0.0, 0.5]));
        assert_eq!(jitter.factor(), FAST_MIN);
        let mut jitter = Jitter::new(true);
        jitter.advance(100, sequence(vec![0.0, 0.99]));
        assert!((jitter.factor() - 1.19).abs() < 1e-9);
        let mut jitter = Jitter::new(true);
        jitter.advance(100, sequence(vec![0.0, 1.0]));
        assert_eq!(jitter.factor(), FAST_MAX);
    }

    #[test]
    fn the_branch_flips_over_a_full_cycle() {
        // The same threshold selects lower and then upper intervals as time advances.
        let early = {
            let mut jitter = Jitter::new(true);
            jitter.advance(10, sequence(vec![0.0, 0.95]));
            jitter.factor()
        };
        let late = {
            let mut jitter = Jitter::new(true);
            jitter.advance(200, sequence(vec![0.0, 0.5]));
            jitter.factor()
        };
        assert!(early <= SLOW_MAX);
        assert!(late >= FAST_MIN);
    }

    #[test]
    fn the_last_factor_is_reused_until_the_next_tick() {
        let mut jitter = Jitter::new(true);
        assert_eq!(jitter.factor(), 1.0, "the original speed applies before the first tick");
        jitter.advance(5, sequence(vec![0.0, 0.6]));
        let first = jitter.factor();
        assert_eq!(jitter.factor(), first, "reads within the same tick must return the same value");
        jitter.advance(6, sequence(vec![0.0, 0.99]));
        assert_ne!(jitter.factor(), first);
    }

    #[test]
    fn offsets_stay_inside_the_documented_ranges() {
        let jitter = Jitter::new(true);
        let negative = jitter.bearing_offset(sequence(vec![1.0, 0.0]));
        let positive = jitter.bearing_offset(sequence(vec![1.0, 0.9]));
        assert!((negative + 15.0).abs() < 1e-9);
        assert!((positive - 15.0).abs() < 1e-9);
        let low = jitter.altitude_offset(sequence(vec![1.0, 0.0]));
        let high = jitter.altitude_offset(sequence(vec![1.0, 0.9]));
        assert!((low + 3.0).abs() < 1e-9);
        assert!((high - 3.0).abs() < 1e-9);
    }
}
