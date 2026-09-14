use serde::{Deserialize, Serialize};

/// Step cadence is expressed in steps per second; stride length is in metres.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StepConfig {
    pub enabled: bool,
    pub cadence: f64,
    pub movement_linked: bool,
    pub stride_m: f64,
    pub daily_reset: bool,
    /// Generate accelerometer and gyroscope events from the same motion that drives the steps.
    /// Off by default, and independent of the step-event channel above.
    pub motion_sensors: bool,
}

impl Default for StepConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            cadence: 2.0,
            movement_linked: true,
            stride_m: 0.75,
            daily_reset: false,
            motion_sensors: false,
        }
    }
}

impl StepConfig {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !self.cadence.is_finite() || !(0.0..=50.0).contains(&self.cadence) {
            return Err("step cadence must be between 0 and 50 steps/s");
        }
        if !self.stride_m.is_finite() || self.stride_m <= 0.0 {
            return Err("step stride must be finite and greater than zero");
        }
        Ok(())
    }

    pub fn rate(&self, active: bool, speed: f64) -> f64 {
        if !active || !self.enabled {
            0.0
        } else if self.movement_linked {
            (speed / self.stride_m).min(self.cadence)
        } else {
            self.cadence
        }
    }
}

/// One raw motion sample derived from the same motion state as the position and the step count.
///
/// Accelerometer units are m/s², gyroscope units are rad/s, and both use the Android device
/// coordinate system: x to the right of the screen, y towards the top, z out of the screen. The
/// device is treated as carried upright and facing its direction of travel, so gravity is 9.80665
/// along +y and a footfall adds a forward and vertical pulse at the current cadence.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotionSample {
    pub accelerometer: [f32; 3],
    pub gyroscope: [f32; 3],
}

/// Gravity magnitude in m/s², IS-GPS/Android convention.
pub const GRAVITY: f64 = 9.80665;
/// Peak footfall acceleration at or above this cadence; scaled down for slower movement.
pub const FOOTFALL_PEAK: f64 = 1.2;

/// Build one sample for the given motion state and phase.
///
/// `phase` advances by `cadence × dt` per sample, so the gait follows the step rate exactly rather
/// than an independent random source. With no cadence the sample is gravity plus the lean needed
/// to keep the current speed constant, so a standstill shows only gravity.
pub fn motion_sample(speed: f64, cadence: f64, bearing: f64, phase: f64) -> MotionSample {
    let speed = speed.max(0.0);
    let cadence = cadence.max(0.0);
    // Forward acceleration between footfalls: a step accelerates the body, and the coast in
    // between barely decelerates. Only walking and running cadences produce a gait.
    let footfall = (cadence / 2.0).clamp(0.0, 1.0) * FOOTFALL_PEAK;
    let gait = if cadence > 0.0 { footfall * (phase * std::f64::consts::TAU).sin() } else { 0.0 };
    // A carrier running at a constant speed holds a slight forward lean, which is what the
    // accelerometer sees while the speed is steady.
    let lean = if speed > 0.0 { 0.05 } else { 0.0 };
    let heading = bearing.to_radians();
    // Device-relative axes: upright carriage points +y at the sky, +x to the right of travel.
    let (right, up) = (heading.sin(), heading.cos());
    MotionSample {
        accelerometer: [
            (gait * right + lean * right) as f32,
            (GRAVITY + gait * up + lean * up) as f32,
            (gait * 0.2) as f32,
        ],
        gyroscope: [
            0.0,
            // Yaw follows the cadence at a walking amplitude; a stationary device does not rotate.
            (gait * 0.35) as f32,
            0.0,
        ],
    }
}

/// Counters are saved separately from the potentially large cell-region configuration.
#[derive(Clone, Default, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StepCount {
    pub total: u64,
    pub today: u64,
    pub day: i64,
    pub fraction: f64,
    pub epoch: u64,
}

impl StepCount {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.total > 9_007_199_254_740_991
            || self.today > self.total
            || !self.fraction.is_finite()
            || !(0.0..1.0).contains(&self.fraction)
        {
            return Err("invalid step counters");
        }
        Ok(())
    }

    /// Only the daily statistic resets. Android's cumulative sensor value never resets here.
    pub fn advance(&mut self, steps: f64, day: i64, daily_reset: bool) {
        if day != self.day {
            if daily_reset {
                self.today = 0;
            }
            self.day = day;
        }
        let amount = self.fraction + steps.max(0.0);
        let added = (amount.floor() as u64).min(9_007_199_254_740_991 - self.total);
        self.fraction = amount.fract();
        self.total += added;
        self.today += added;
    }
}

/// Use the device's current timezone; host protocol tests use UTC and inject day boundaries.
pub fn local_day() -> i64 {
    #[cfg(target_os = "android")]
    unsafe {
        let time = libc::time(std::ptr::null_mut());
        let mut calendar: libc::tm = std::mem::zeroed();
        if !libc::localtime_r(&time, &mut calendar).is_null() {
            return i64::from(calendar.tm_year) * 1000 + i64::from(calendar.tm_yday);
        }
    }
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()
        as i64
        / 86_400
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fractions_accumulate_and_midnight_preserves_sensor_total() {
        let mut count = StepCount::default();
        for _ in 0..4 {
            count.advance(0.25, 1, true);
        }
        assert_eq!((count.total, count.today), (1, 1));
        count.advance(2.0, 2, true);
        assert_eq!((count.total, count.today), (3, 2));
        count.advance(1.0, 3, false);
        assert_eq!((count.total, count.today), (4, 3));
    }

    #[test]
    fn linked_cadence_stops_at_rest_and_is_capped() {
        let mut config = StepConfig { enabled: true, ..StepConfig::default() };
        assert_eq!(config.rate(true, 0.0), 0.0);
        assert_eq!(config.rate(true, 0.75), 1.0);
        assert_eq!(config.rate(true, 100.0), 2.0);
        assert_eq!(config.rate(false, 1.5), 0.0);
        config.movement_linked = false;
        assert_eq!(config.rate(true, 0.0), 2.0);
    }

    #[test]
    fn rejects_invalid_settings_and_preserves_large_integer_counts() {
        for cadence in [-1.0, 51.0, f64::NAN] {
            assert!(StepConfig { cadence, ..StepConfig::default() }.validate().is_err());
        }
        assert!(StepConfig { stride_m: 0.0, ..StepConfig::default() }.validate().is_err());
        let mut count = StepCount { total: 16_777_216, ..StepCount::default() };
        count.advance(1.0, 0, false);
        assert_eq!(count.total, 16_777_217);
    }

    #[test]
    fn a_stationary_device_shows_gravity_and_does_not_rotate() {
        let sample = motion_sample(0.0, 0.0, 0.0, 0.0);
        assert!((sample.accelerometer[1] as f64 - GRAVITY).abs() < 1e-6);
        assert_eq!(sample.accelerometer[0], 0.0);
        assert_eq!(sample.accelerometer[2], 0.0);
        assert_eq!(sample.gyroscope, [0.0, 0.0, 0.0]);
        // The same is true at any phase, so a standstill never invents a gait.
        for step in 0..20 {
            let sample = motion_sample(0.0, 0.0, 90.0, step as f64 * 0.3);
            assert!(sample.accelerometer.iter().all(|value| value.is_finite()));
            assert_eq!(sample.gyroscope, [0.0, 0.0, 0.0]);
        }
    }

    #[test]
    fn a_running_gait_comes_from_the_same_cadence_as_the_steps() {
        let cadence = 2.0;
        let phase_step = cadence * 0.02; // 50 Hz sampling
        let mut peaks: usize = 0;
        let mut values = Vec::new();
        // Ten seconds of samples: the number of footfall peaks has to match the step count.
        for index in 0..500 {
            let sample = motion_sample(1.5, cadence, 0.0, index as f64 * phase_step);
            assert!(sample.accelerometer.iter().all(|value| value.is_finite()));
            assert!(sample.gyroscope.iter().all(|value| value.is_finite()));
            values.push(sample.accelerometer[1] as f64);
        }
        for pair in values.windows(3) {
            if pair[1] > pair[0] && pair[1] >= pair[2] && pair[1] > GRAVITY {
                peaks += 1;
            }
        }
        let expected = (cadence * 10.0).round() as usize;
        assert!(
            peaks.abs_diff(expected) <= 1,
            "a {cadence}/s gait produced {peaks} footfall peaks, expected about {expected}"
        );
        // Forward and lateral components stay small: a gait is a vertical oscillation with a lean.
        let forward = values.iter().map(|value| (value - GRAVITY).abs()).fold(0.0, f64::max);
        assert!(forward <= FOOTFALL_PEAK + 0.05, "vertical excursion {forward} is not a gait");
        // The heading rotates the accelerometer with the direction of travel.
        let north = motion_sample(1.5, cadence, 0.0, 0.25 / cadence);
        let east = motion_sample(1.5, cadence, 90.0, 0.25 / cadence);
        assert!((north.accelerometer[2] - east.accelerometer[2]).abs() < 1e-6);
        assert!(north.accelerometer[0].abs() < east.accelerometer[0].abs());
    }

    #[test]
    fn a_moving_device_without_steps_reports_a_lean_instead_of_a_gait() {
        // Movement while the step channel is off still has to produce plausible raw data: a steady
        // speed is a steady forward lean, not a standstill.
        let lean = motion_sample(1.5, 0.0, 90.0, 0.0);
        // Heading east points the device's x axis along the travel direction.
        assert!(lean.accelerometer[0] > 0.0);
        assert!((lean.accelerometer[1] as f64 - GRAVITY).abs() < FOOTFALL_PEAK);
        assert_eq!(lean.gyroscope, [0.0, 0.0, 0.0]);
    }
}
