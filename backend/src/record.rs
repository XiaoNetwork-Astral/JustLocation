//! Collect real positions into a route without producing output.
//! Distance filtering removes repeated samples. Once the point limit is reached,
//! further samples are rejected and the collected track remains available to save.

use crate::Position;
use serde::{Deserialize, Serialize};

/// Match the playback route's point limit.
pub const MAX_POINTS: usize = crate::route::MAX_POINTS;
/// Minimum distance between accepted points, in meters.
pub const DEFAULT_MIN_DISTANCE: f64 = 0.5;
const EARTH_RADIUS: f64 = 6_371_008.8;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Recording {
    points: Vec<Position>,
    /// Accumulated recording duration.
    duration: f64,
    full: bool,
    pub id: String,
    pub paused: bool,
    pub breaks: Vec<usize>,
    #[serde(default)]
    origin: Option<f64>,
    #[serde(default)]
    base: f64,
    #[serde(default)]
    next_segment: bool,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct RecordState {
    pub points: Vec<Position>,
    pub count: usize,
    pub seconds: f64,
    /// The point limit has been reached; collected points remain available.
    pub full: bool,
    /// Samples rejected by distance filtering.
    pub skipped: u64,
}

impl Recording {
    pub fn new() -> Self {
        Self { origin: Some(0.0), ..Self::default() }
    }

    pub fn is_full(&self) -> bool {
        self.full
    }

    pub fn points(&self) -> &[Position] {
        &self.points
    }

    pub fn seconds(&self) -> f64 {
        self.duration
    }

    /// Add a real position and report whether it was accepted.
    /// The caller supplies monotonic seconds since recording began.
    pub fn add(&mut self, position: Position, seconds: f64) -> Result<bool, &'static str> {
        if self.paused {
            return Err("recording is paused; resume before adding points");
        }
        if self.full {
            return Ok(false);
        }
        if !seconds.is_finite() || seconds < 0.0 {
            return Err("recording time must be finite and not negative");
        }
        position.validate()?;
        // A timestamp regression contributes zero elapsed time.
        let origin = *self.origin.get_or_insert(seconds);
        self.duration = self.duration.max(self.base + (seconds - origin).max(0.0));
        if let Some(previous) = self.points.last() {
            let moved = distance(previous, &position);
            // Reject stationary samples regardless of the time between callbacks.
            if !self.next_segment && moved < DEFAULT_MIN_DISTANCE {
                return Ok(false);
            }
        }
        if self.next_segment && !self.points.is_empty() {
            self.breaks.push(self.points.len());
        }
        self.next_segment = false;
        self.points.push(position);
        if self.points.len() >= MAX_POINTS {
            self.full = true;
        }
        Ok(true)
    }

    pub fn pause(&mut self) {
        self.paused = true;
    }
    pub fn resume(&mut self) {
        self.paused = false;
        self.base = self.duration;
        self.origin = None;
        self.next_segment = true;
    }
    pub fn state(&self, skipped: u64) -> RecordState {
        RecordState {
            points: self.points.clone(),
            count: self.points.len(),
            seconds: self.duration,
            full: self.full,
            skipped,
        }
    }
}

/// Great-circle distance in meters, using the shared geographic calculation.
fn distance(a: &Position, b: &Position) -> f64 {
    let dlat = (b.latitude - a.latitude).to_radians();
    let dlon = (b.longitude - a.longitude).to_radians();
    let h = (dlat / 2.0).sin().powi(2)
        + a.latitude.to_radians().cos()
            * b.latitude.to_radians().cos()
            * (dlon / 2.0).sin().powi(2);
    2.0 * h.clamp(0.0, 1.0).sqrt().asin() * EARTH_RADIUS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(latitude: f64, longitude: f64) -> Position {
        Position::new(latitude, longitude)
    }

    #[test]
    fn repeated_samples_of_a_still_position_are_dropped() {
        let mut recording = Recording::new();
        assert!(recording.add(at(31.0, 121.0), 0.0).unwrap());
        for step in 1..10 {
            assert!(!recording.add(at(31.0, 121.0), step as f64 * 0.5).unwrap());
        }
        assert_eq!(recording.points().len(), 1);
    }

    #[test]
    fn moving_samples_are_kept_even_when_they_arrive_quickly() {
        let mut recording = Recording::new();
        recording.add(at(31.0, 121.0), 0.0).unwrap();
        assert!(recording.add(at(31.0001, 121.0), 0.1).unwrap());
        assert!(recording.add(at(31.0002, 121.0), 0.2).unwrap());
        assert_eq!(recording.points().len(), 3);
        assert!((recording.seconds() - 0.2).abs() < 1e-9);
    }

    #[test]
    fn the_track_stops_at_the_limit_and_says_so() {
        let mut recording = Recording::new();
        for index in 0..MAX_POINTS + 5 {
            recording.add(at(31.0 + index as f64 * 0.0001, 121.0), index as f64).unwrap();
        }
        assert_eq!(recording.points().len(), MAX_POINTS);
        assert!(recording.is_full());
        assert!(!recording.add(at(32.0, 121.0), MAX_POINTS as f64 + 1.0).unwrap());
        assert_eq!(recording.points().len(), MAX_POINTS);
    }

    #[test]
    fn invalid_positions_are_rejected_instead_of_poisoning_the_track() {
        let mut recording = Recording::new();
        let mut broken = at(31.0, 121.0);
        broken.latitude = 91.0;
        assert!(recording.add(broken, 0.0).is_err());
        broken = at(31.0, 121.0);
        broken.accuracy = -1.0;
        assert!(recording.add(broken, 0.0).is_err());
        assert!(recording.add(at(31.0, 121.0), f64::NAN).is_err());
        assert!(recording.points().is_empty());
    }

    #[test]
    fn a_clock_that_goes_backwards_does_not_shrink_the_duration() {
        let mut recording = Recording::new();
        recording.add(at(31.0, 121.0), 10.0).unwrap();
        recording.add(at(31.0001, 121.0), 5.0).unwrap();
        assert!((recording.seconds() - 10.0).abs() < 1e-9);
        assert_eq!(recording.points().len(), 2);
    }

    #[test]
    fn the_state_reports_what_the_panel_needs() {
        let mut recording = Recording::new();
        recording.add(at(31.0, 121.0), 0.0).unwrap();
        recording.add(at(31.0, 121.0), 1.0).unwrap();
        let state = recording.state(1);
        assert_eq!(state.count, 1);
        assert_eq!(state.points.len(), 1);
        assert_eq!(state.skipped, 1);
        assert!(!state.full);
        assert!((state.seconds - 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_recorded_track_can_be_played_back_directly() {
        let mut recording = Recording::new();
        recording.add(at(31.0, 121.0), 0.0).unwrap();
        recording.add(at(31.001, 121.0), 5.0).unwrap();
        let route = crate::route::Route {
            geometry: None,
            points: recording.points().to_vec(),
            speed: 5.0,
            repeat_count: 1,
            repeat_delay: 0.0,
            breaks: vec![],
        };
        assert!(crate::route::Playback::new(route, std::time::Instant::now()).is_ok());
    }
}
