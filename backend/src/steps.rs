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
}

impl Default for StepConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            cadence: 2.0,
            movement_linked: true,
            stride_m: 0.75,
            daily_reset: false,
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
}
