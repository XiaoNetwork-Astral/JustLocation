use crate::Position;
use std::time::{Duration, Instant};

/// A controller must renew its direction within two seconds to keep moving.
#[derive(Clone)]
pub struct Motion {
    origin: Position,
    started: Instant,
}

impl Motion {
    pub fn new(
        mut position: Position,
        speed: f64,
        bearing: f64,
        now: Instant,
    ) -> Result<Self, &'static str> {
        position.speed = speed;
        position.bearing = bearing;
        position.validate()?;
        if speed > 1000.0 {
            return Err("speed must not exceed 1000 m/s");
        }
        Ok(Self { origin: position, started: now })
    }

    pub fn advance(&mut self, now: Instant) -> Position {
        let elapsed = now.saturating_duration_since(self.started);
        let distance = self.origin.speed * elapsed.min(Duration::from_secs(2)).as_secs_f64();
        let angle = distance / 6_371_008.8;
        let lat = self.origin.latitude.to_radians();
        let bearing = self.origin.bearing.to_radians();
        let next_lat = (lat.sin() * angle.cos() + lat.cos() * angle.sin() * bearing.cos())
            .clamp(-1.0, 1.0)
            .asin();
        let next_lon = self.origin.longitude.to_radians()
            + (bearing.sin() * angle.sin() * lat.cos())
                .atan2(angle.cos() - lat.sin() * next_lat.sin());
        let mut position = self.origin.clone();
        position.latitude = next_lat.to_degrees();
        position.longitude = (next_lon.to_degrees() + 180.0).rem_euclid(360.0) - 180.0;
        if elapsed >= Duration::from_secs(2) {
            position.speed = 0.0;
        }
        position
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_by_elapsed_time_and_stops_when_controller_disappears() {
        let start = Instant::now();
        let mut motion = Motion::new(Position::new(0.0, 0.0), 10.0, 90.0, start).unwrap();
        let one = motion.advance(start + Duration::from_secs(1));
        assert!((one.longitude - 0.00008993).abs() < 0.0000001);
        assert_eq!(one.speed, 10.0);
        let expired = motion.advance(start + Duration::from_secs(30));
        assert!((expired.longitude - 0.00017986).abs() < 0.0000001);
        assert_eq!(expired.speed, 0.0);
        assert_eq!(motion.advance(start + Duration::from_secs(60)), expired);
    }

    #[test]
    fn crosses_dateline_without_invalid_coordinates_and_preserves_altitude() {
        let start = Instant::now();
        let mut position = Position::new(0.0, 179.99999);
        position.altitude = 12.0;
        let mut motion = Motion::new(position, 10.0, 90.0, start).unwrap();
        let output = motion.advance(start + Duration::from_secs(1));
        assert!(output.longitude < -179.99);
        assert_eq!(output.altitude, 12.0);
        assert!(output.validate().is_ok());
    }

    #[test]
    fn rejects_invalid_motion() {
        for (speed, bearing) in
            [(-1.0, 0.0), (1001.0, 0.0), (f64::NAN, 0.0), (1.0, 360.0), (1.0, -1.0)]
        {
            assert!(Motion::new(Position::new(0.0, 0.0), speed, bearing, Instant::now()).is_err());
        }
    }
}
