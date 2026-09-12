use crate::Position;
use serde::{Deserialize, Serialize};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RealismConfig {
    pub enabled: bool,
    pub drift_radius_m: f64,
    pub altitude_m: f64,
    pub bearing_degrees: f64,
    pub speed_variation: f64,
    pub period_seconds: f64,
    pub corner_radius_m: f64,
    pub seed: Option<u64>,
}

impl Default for RealismConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            drift_radius_m: 2.0,
            altitude_m: 1.0,
            bearing_degrees: 3.0,
            speed_variation: 0.1,
            period_seconds: 5.0,
            corner_radius_m: 5.0,
            seed: None,
        }
    }
}

impl RealismConfig {
    pub fn validate(&self) -> Result<(), &'static str> {
        for (value, min, max) in [
            (self.drift_radius_m, 0.0, 100.0),
            (self.altitude_m, 0.0, 100.0),
            (self.bearing_degrees, 0.0, 45.0),
            (self.speed_variation, 0.0, 0.5),
            (self.period_seconds, 1.0, 60.0),
            (self.corner_radius_m, 0.0, 100.0),
        ] {
            if !value.is_finite() || !(min..=max).contains(&value) {
                return Err("realism parameter is outside its supported range");
            }
        }
        if self.seed.is_some_and(|seed| seed > 9_007_199_254_740_991) {
            return Err("realism seed exceeds the JSON integer range");
        }
        Ok(())
    }
}

/// Time-indexed randomness gives the same trajectory regardless of polling frequency.
#[derive(Clone)]
pub struct Realism {
    pub config: RealismConfig,
    started: Instant,
    seed: u64,
}

impl Default for Realism {
    fn default() -> Self {
        Self::new(RealismConfig::default(), Instant::now())
    }
}

impl Realism {
    pub fn new(config: RealismConfig, now: Instant) -> Self {
        let seed = config.seed.unwrap_or_else(|| {
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos() as u64
        });
        Self { config, started: now, seed }
    }

    pub fn reset(&mut self, now: Instant) {
        self.started = now;
    }

    fn value(&self, index: u64, channel: u64) -> f64 {
        crate::jitter::signed_noise(self.seed, index, channel)
    }

    fn coordinate(&self, now: Instant) -> f64 {
        now.saturating_duration_since(self.started).as_secs_f64() / self.config.period_seconds
    }

    fn noise(&self, now: Instant, channel: u64) -> f64 {
        let t = self.coordinate(now);
        let index = t.floor() as u64;
        let u = t.fract();
        let blend = u * u * (3.0 - 2.0 * u);
        self.value(index, channel) * (1.0 - blend) + self.value(index + 1, channel) * blend
    }

    pub fn factor(&self, now: Instant) -> f64 {
        if self.config.enabled {
            1.0 + self.noise(now, 0) * self.config.speed_variation
        } else {
            1.0
        }
    }

    pub fn average_factor(&self, from: Instant, to: Instant) -> f64 {
        if !self.config.enabled || to <= from {
            return 1.0;
        }
        let mut start = self.coordinate(from);
        let end = self.coordinate(to);
        if end <= start {
            return 1.0;
        }
        let duration = end - start;
        let mut integral = 0.0;
        // Integral of the smoothstep polynomial. No history grows with session duration.
        let primitive = |u: f64| u.powi(3) - u.powi(4) / 2.0;
        while start < end {
            let index = start.floor() as u64;
            let stop = end.min(index as f64 + 1.0);
            let left = start - index as f64;
            let right = stop - index as f64;
            let a = self.value(index, 0);
            let b = self.value(index + 1, 0);
            integral += a * (right - left) + (b - a) * (primitive(right) - primitive(left));
            start = stop;
        }
        1.0 + integral / duration * self.config.speed_variation
    }

    /// Offsets apply to an output copy; the saved anchor and the next movement origin stay exact.
    pub fn output(&self, original: &Position, now: Instant) -> Position {
        if !self.config.enabled {
            return original.clone();
        }
        let x = self.noise(now, 1) * self.config.drift_radius_m / 2.0_f64.sqrt();
        let y = self.noise(now, 2) * self.config.drift_radius_m / 2.0_f64.sqrt();
        let mut point = crate::motion::translate(original, x.hypot(y), x.atan2(y).to_degrees());
        point.altitude += self.noise(now, 3) * self.config.altitude_m;
        point.bearing =
            (point.bearing + self.noise(now, 4) * self.config.bearing_degrees).rem_euclid(360.0);
        point.accuracy = point.accuracy.max(self.config.drift_radius_m);
        point
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn fixed_seed_is_continuous_bounded_and_polling_independent() {
        let now = Instant::now();
        let realism =
            Realism::new(RealismConfig { enabled: true, seed: Some(3), ..Default::default() }, now);
        let point = Position::new(0.0, 179.999999);
        for second in 0..300 {
            let time = now + Duration::from_secs(second);
            let output = realism.output(&point, time);
            output.validate().unwrap();
            assert!(crate::route::distance(&point, &output) <= 2.0 + 1e-6);
            assert!(output.altitude.abs() <= 1.0);
            assert!((0.9..=1.1).contains(&realism.factor(time)));
            let next = realism.output(&point, time + Duration::from_millis(1));
            assert!(crate::route::distance(&output, &next) < 0.01);
        }
        let end = now + Duration::from_secs(20);
        let whole = realism.average_factor(now, end) * 20.0;
        let parts: f64 = (0..200)
            .map(|i| {
                realism.average_factor(
                    now + Duration::from_millis(i * 100),
                    now + Duration::from_millis((i + 1) * 100),
                ) * 0.1
            })
            .sum();
        assert!((whole - parts).abs() < 1e-10);
        assert_eq!(Realism::default().output(&point, end), point);
    }
}
