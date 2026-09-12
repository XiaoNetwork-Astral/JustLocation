use crate::Position;
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Instant};

pub const MAX_POINTS: usize = 100_000;
pub const INLINE_POINTS: usize = 128;

const EARTH_RADIUS: f64 = 6_371_008.8;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Route {
    pub points: Vec<Position>,
    /// Indices of segment starts; no movement is interpolated across a break.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub breaks: Vec<usize>,
    /// Metres per second.
    pub speed: f64,
    /// Total number of plays, including the first.
    #[serde(default = "once")]
    pub repeat_count: u32,
    /// Seconds held at the end before returning to the start.
    #[serde(default)]
    pub repeat_delay: f64,
}

fn once() -> u32 {
    1
}

#[derive(Serialize)]
pub struct RouteState {
    pub plan: Option<Route>,
    pub point_count: usize,
    pub distance: f64,
    pub total_distance: f64,
    pub paused: bool,
    pub completed: bool,
    pub lap: u32,
    pub waiting_seconds: f64,
}

#[derive(Clone)]
pub struct Playback {
    plan: Arc<Route>,
    path: Arc<Vec<Position>>,
    lengths: Arc<Vec<f64>>,
    cumulative: Arc<Vec<f64>>,
    total: f64,
    elapsed: f64,
    paused: bool,
    updated: Instant,
    factor: f64,
    moving_seconds: f64,
}

fn cumulative(lengths: &[f64]) -> Vec<f64> {
    let mut sum = 0.0;
    lengths
        .iter()
        .map(|length| {
            sum += length;
            sum
        })
        .collect()
}

fn arc(a: &Position, b: &Position) -> f64 {
    let dlat = (b.latitude - a.latitude).to_radians();
    let dlon = (b.longitude - a.longitude).to_radians();
    let h = (dlat / 2.0).sin().powi(2)
        + a.latitude.to_radians().cos()
            * b.latitude.to_radians().cos()
            * (dlon / 2.0).sin().powi(2);
    2.0 * h.clamp(0.0, 1.0).sqrt().asin()
}

pub fn distance(a: &Position, b: &Position) -> f64 {
    arc(a, b) * EARTH_RADIUS
}

pub(super) fn interpolate(a: &Position, b: &Position, fraction: f64) -> Position {
    let mut position =
        crate::motion::translate(a, distance(a, b) * fraction, bearing(a, b).to_degrees());
    position.altitude = a.altitude * (1.0 - fraction) + b.altitude * fraction;
    position
}

fn bearing(a: &Position, b: &Position) -> f64 {
    let lat1 = a.latitude.to_radians();
    let lat2 = b.latitude.to_radians();
    let dlon = (b.longitude - a.longitude).to_radians();
    (dlon.sin() * lat2.cos()).atan2(lat1.cos() * lat2.sin() - lat1.sin() * lat2.cos() * dlon.cos())
}

impl Playback {
    pub fn travelled(&self) -> f64 {
        let (lap, distance, _) = self.progress();
        (lap - 1) as f64 * self.total + distance
    }

    pub fn speed(&self) -> f64 {
        self.plan.speed
    }

    pub fn new(plan: Route, now: Instant) -> Result<Self, &'static str> {
        if !(2..=MAX_POINTS).contains(&plan.points.len()) {
            return Err("a route needs 2 to 100000 points");
        }
        if !plan.speed.is_finite() || plan.speed <= 0.0 || plan.speed > 1000.0 {
            return Err("route speed must be greater than 0 and at most 1000 m/s");
        }
        if !(1..=10_000).contains(&plan.repeat_count) {
            return Err("route play count must be between 1 and 10000");
        }
        if !plan.repeat_delay.is_finite() || !(0.0..=86_400.0).contains(&plan.repeat_delay) {
            return Err("route repeat delay must be between 0 and 86400 seconds");
        }
        for point in &plan.points {
            point.validate()?;
        }
        if plan.breaks.iter().any(|&i| i == 0 || i >= plan.points.len())
            || plan.breaks.windows(2).any(|p| p[0] >= p[1])
        {
            return Err("route breaks must be increasing segment-start indices inside the route");
        }
        let lengths: Vec<_> = plan
            .points
            .windows(2)
            .enumerate()
            .map(|(i, pair)| {
                if plan.breaks.binary_search(&(i + 1)).is_ok() {
                    0.0
                } else {
                    distance(&pair[0], &pair[1])
                }
            })
            .collect();
        if lengths.iter().any(|length| *length > EARTH_RADIUS * (std::f64::consts::PI - 1e-6)) {
            return Err("adjacent route points must not be antipodal");
        }
        let cumulative = cumulative(&lengths);
        if *cumulative.last().unwrap() <= 0.0 {
            return Err("route must contain at least one moving segment");
        }
        Ok(Self {
            total: lengths.iter().sum(),
            path: Arc::new(plan.points.clone()),
            plan: Arc::new(plan),
            lengths: Arc::new(lengths),
            cumulative: Arc::new(cumulative),
            elapsed: 0.0,
            paused: false,
            updated: now,
            factor: 1.0,
            moving_seconds: 0.0,
        })
    }

    pub fn smoothed(plan: Route, now: Instant, radius: f64) -> Result<Self, &'static str> {
        let mut playback = Self::new(plan, now)?;
        if radius > 0.0 {
            let mut path = Vec::new();
            let mut lengths = Vec::new();
            let mut start = 0;
            for end in playback.plan.breaks.iter().copied().chain([playback.plan.points.len()]) {
                let segment = crate::smoothing::corners(&playback.plan.points[start..end], radius);
                if !path.is_empty() {
                    lengths.push(0.0);
                }
                lengths.extend(segment.windows(2).map(|p| distance(&p[0], &p[1])));
                path.extend(segment);
                start = end;
            }
            playback.total = lengths.iter().sum();
            playback.cumulative = Arc::new(cumulative(&lengths));
            playback.lengths = Arc::new(lengths);
            playback.path = Arc::new(path);
        }
        Ok(playback)
    }

    pub fn advance(&mut self, now: Instant) -> Position {
        self.advance_with(now, |_, _| 1.0, 1.0)
    }

    pub fn advance_varied(&mut self, now: Instant, realism: &crate::realism::Realism) -> Position {
        self.advance_with(now, |from, to| realism.average_factor(from, to), realism.factor(now))
    }

    pub fn moving_seconds(&self) -> f64 {
        self.moving_seconds
    }

    fn advance_with(
        &mut self,
        now: Instant,
        average: impl Fn(Instant, Instant) -> f64,
        factor: f64,
    ) -> Position {
        self.moving_seconds = 0.0;
        if !self.paused {
            let mut cursor = self.updated.min(now);
            let travel = self.total / self.plan.speed;
            let cycle = travel + self.plan.repeat_delay;
            while cursor < now && self.elapsed < self.duration() {
                let seconds = now.duration_since(cursor).as_secs_f64();
                let index = (self.elapsed / cycle).floor();
                let within = self.elapsed - index * cycle;
                let moving = within < travel;
                let end =
                    (index * cycle + if moving { travel } else { cycle }).min(self.duration());
                if end <= self.elapsed {
                    self.elapsed = self.elapsed.next_up();
                    continue;
                }
                let available = if moving { seconds * average(cursor, now) } else { seconds };
                let remaining = end - self.elapsed;
                if available >= remaining {
                    let consumed = if moving {
                        // Find arrival in wall time so repeat waits never inherit the speed multiplier.
                        let (mut low, mut high) = (0.0, seconds);
                        for _ in 0..48 {
                            let middle = (low + high) / 2.0;
                            let time = cursor + std::time::Duration::from_secs_f64(middle);
                            if middle * average(cursor, time) < remaining {
                                low = middle;
                            } else {
                                high = middle;
                            }
                        }
                        high
                    } else {
                        remaining
                    };
                    if moving {
                        self.moving_seconds += consumed;
                    }
                    self.elapsed = end;
                    cursor += std::time::Duration::from_secs_f64(consumed);
                } else {
                    if moving {
                        self.moving_seconds += seconds;
                    }
                    self.elapsed += available;
                    break;
                }
            }
        }
        self.updated = now;
        self.factor = factor;
        self.position()
    }

    pub fn pause(&mut self, paused: bool) -> Result<(), &'static str> {
        if self.elapsed >= self.duration() {
            return Err("route has already arrived");
        }
        self.paused = paused;
        Ok(())
    }

    pub fn plan(&self) -> &Route {
        &self.plan
    }

    pub fn state(&self) -> RouteState {
        let (lap, distance, waiting_seconds) = self.progress();
        RouteState {
            plan: (self.plan.points.len() <= INLINE_POINTS).then(|| (*self.plan).clone()),
            point_count: self.plan.points.len(),
            distance,
            total_distance: self.total,
            paused: self.paused,
            completed: self.elapsed >= self.duration(),
            lap,
            waiting_seconds,
        }
    }

    fn duration(&self) -> f64 {
        self.total / self.plan.speed * self.plan.repeat_count as f64
            + self.plan.repeat_delay * (self.plan.repeat_count - 1) as f64
    }

    // Derive the current lap directly: a delayed tick may cross many laps.
    // Waiting is clock state, so it never blocks pause, stop or other requests.
    fn progress(&self) -> (u32, f64, f64) {
        if self.elapsed >= self.duration() {
            return (self.plan.repeat_count, self.total, 0.0);
        }
        let travel = self.total / self.plan.speed;
        let cycle = travel + self.plan.repeat_delay;
        let index = (self.elapsed / cycle).floor();
        let within = self.elapsed - index * cycle;
        if within >= travel {
            (index as u32 + 1, self.total, cycle - within)
        } else {
            (index as u32 + 1, within * self.plan.speed, 0.0)
        }
    }

    pub fn position(&self) -> Position {
        let (_, distance, _) = self.progress();
        if distance >= self.total {
            let mut end = self.path.last().unwrap().clone();
            end.speed = 0.0;
            return end;
        }
        let index = self.cumulative.partition_point(|&end| end <= distance);
        let remaining = distance - if index == 0 { 0.0 } else { self.cumulative[index - 1] };
        let a = &self.path[index];
        let b = &self.path[index + 1];
        let fraction = remaining / self.lengths[index];
        let angular = remaining / EARTH_RADIUS;
        let heading = bearing(a, b);
        let latitude = a.latitude.to_radians();
        let lat = (latitude.sin() * angular.cos() + latitude.cos() * angular.sin() * heading.cos())
            .clamp(-1.0, 1.0)
            .asin();
        let lon = a.longitude.to_radians()
            + (heading.sin() * angular.sin() * latitude.cos())
                .atan2(angular.cos() - latitude.sin() * lat.sin());
        let mut point = Position {
            latitude: lat.to_degrees(),
            longitude: (lon.to_degrees() + 180.0).rem_euclid(360.0) - 180.0,
            altitude: a.altitude * (1.0 - fraction) + b.altitude * fraction,
            accuracy: a.accuracy,
            speed: if self.paused { 0.0 } else { self.plan.speed * self.factor },
            bearing: 0.0,
        };
        point.bearing = bearing(&point, b).to_degrees().rem_euclid(360.0) % 360.0;
        point
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn repeat_boundaries_hold_the_end_and_restart_only_after_the_interval() {
        let now = Instant::now();
        let mut playback = Playback::new(
            Route {
                points: vec![Position::new(0.0, 0.0), Position::new(0.0, 0.001)],
                speed: 10.0,
                repeat_count: 2,
                repeat_delay: 5.0,
                breaks: vec![],
            },
            now,
        )
        .unwrap();
        let travel = playback.total / playback.plan.speed;
        let endpoint = playback.advance(now + Duration::from_secs_f64(travel + 0.01));
        assert_eq!(endpoint.longitude, 0.001);
        assert_eq!(endpoint.speed, 0.0);
        assert!(!playback.state().completed);
        assert!((playback.state().waiting_seconds - 4.99).abs() < 1e-6);
        let restart = playback.advance(now + Duration::from_secs_f64(travel + 5.01));
        assert_eq!(playback.state().lap, 2);
        assert!(restart.longitude > 0.0 && restart.longitude < 0.000001);
        assert_eq!(restart.speed, 10.0);
        playback.advance(now + Duration::from_secs_f64(travel * 2.0 + 5.01));
        assert!(playback.state().completed);
        assert_eq!(playback.state().waiting_seconds, 0.0);
    }

    #[test]
    fn antimeridian_takes_the_short_path() {
        let now = Instant::now();
        let mut playback = Playback::new(
            Route {
                points: vec![Position::new(0.0, 179.999), Position::new(0.0, -179.999)],
                speed: 10.0,
                repeat_count: 1,
                repeat_delay: 0.0,
                breaks: vec![],
            },
            now,
        )
        .unwrap();
        assert!((playback.total - 222.39016).abs() < 0.01);
        let point = playback.advance(now + Duration::from_secs_f64(playback.total / 20.0));
        assert!((point.longitude.abs() - 180.0).abs() < 0.000001);
        assert!(point.latitude.abs() < 0.000001);
    }

    #[test]
    fn crossing_a_waypoint_preserves_distance_and_interpolates_altitude() {
        let now = Instant::now();
        let mut last = Position::new(0.001, 0.001);
        last.altitude = 100.0;
        let mut playback = Playback::new(
            Route {
                points: vec![Position::new(0.0, 0.0), Position::new(0.0, 0.001), last],
                speed: 10.0,
                repeat_count: 1,
                repeat_delay: 0.0,
                breaks: vec![],
            },
            now,
        )
        .unwrap();
        let point = playback.advance(now + Duration::from_secs_f64(playback.total * 0.75 / 10.0));
        assert!((point.latitude - 0.0005).abs() < 0.000001);
        assert!((point.altitude - 50.0).abs() < 0.001);
        assert!(point.bearing.min(360.0 - point.bearing) < 0.001);
    }
}
