// Formulae adapted from wandergis/coordtransform (MIT), also used by ui/src/coordinates.ts.
// Copyright (c) 2015 记忆的残骸; see backend/licenses/coordtransform.txt.
use crate::Position;
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CoordinateSystem {
    Wgs84,
    Gcj02,
    Bd09,
}

fn inside(p: &Position) -> bool {
    p.longitude > 73.66 && p.longitude < 135.05 && p.latitude > 3.86 && p.latitude < 53.55
}
fn to_gcj(p: &Position) -> Position {
    if !inside(p) {
        return p.clone();
    }
    let (x, y) = (p.longitude - 105.0, p.latitude - 35.0);
    let waves = (20.0 * (6.0 * x * PI).sin() + 20.0 * (2.0 * x * PI).sin()) * 2.0 / 3.0;
    let mut lat =
        -100.0 + 2.0 * x + 3.0 * y + 0.2 * y * y + 0.1 * x * y + 0.2 * x.abs().sqrt() + waves;
    lat += (20.0 * (y * PI).sin() + 40.0 * (y / 3.0 * PI).sin()) * 2.0 / 3.0;
    lat += (160.0 * (y / 12.0 * PI).sin() + 320.0 * (y * PI / 30.0).sin()) * 2.0 / 3.0;
    let mut lon = 300.0 + x + 2.0 * y + 0.1 * x * x + 0.1 * x * y + 0.1 * x.abs().sqrt() + waves;
    lon += (20.0 * (x * PI).sin() + 40.0 * (x / 3.0 * PI).sin()) * 2.0 / 3.0;
    lon += (150.0 * (x / 12.0 * PI).sin() + 300.0 * (x / 30.0 * PI).sin()) * 2.0 / 3.0;
    let radians = p.latitude.to_radians();
    let magic = 1.0 - 0.00669342162296594323 * radians.sin().powi(2);
    let root = magic.sqrt();
    let mut result = p.clone();
    result.latitude +=
        lat * 180.0 / ((6378245.0 * (1.0 - 0.00669342162296594323) / (magic * root)) * PI);
    result.longitude += lon * 180.0 / ((6378245.0 / root) * radians.cos() * PI);
    result
}
fn from_gcj(p: &Position) -> Result<Position, &'static str> {
    if !inside(p) {
        return Ok(p.clone());
    }
    let mut result = p.clone();
    for _ in 0..20 {
        let projected = to_gcj(&result);
        let (lat, lon) = (projected.latitude - p.latitude, projected.longitude - p.longitude);
        result.latitude -= lat;
        result.longitude -= lon;
        if lat.abs().max(lon.abs()) < 1e-10 {
            return Ok(result);
        }
    }
    Err("coordinate is near the conversion boundary; supply WGS84 directly")
}
pub fn convert(
    p: &Position,
    from: CoordinateSystem,
    to: CoordinateSystem,
) -> Result<Position, &'static str> {
    p.validate()?;
    if from == to {
        return Ok(p.clone());
    }
    let x_pi = PI * 3000.0 / 180.0;
    let gcj = match from {
        CoordinateSystem::Wgs84 => to_gcj(p),
        CoordinateSystem::Gcj02 => p.clone(),
        CoordinateSystem::Bd09 => {
            let (x, y) = (p.longitude - 0.0065, p.latitude - 0.006);
            let z = x.hypot(y) - 0.00002 * (y * x_pi).sin();
            let theta = y.atan2(x) - 0.000003 * (x * x_pi).cos();
            let mut result = p.clone();
            result.latitude = z * theta.sin();
            result.longitude = z * theta.cos();
            result
        }
    };
    let result = match to {
        CoordinateSystem::Wgs84 => from_gcj(&gcj)?,
        CoordinateSystem::Gcj02 => gcj,
        CoordinateSystem::Bd09 => {
            let z = gcj.longitude.hypot(gcj.latitude) + 0.00002 * (gcj.latitude * x_pi).sin();
            let theta = gcj.latitude.atan2(gcj.longitude) + 0.000003 * (gcj.longitude * x_pi).cos();
            let mut result = gcj.clone();
            result.latitude = z * theta.sin() + 0.006;
            result.longitude = z * theta.cos() + 0.0065;
            result
        }
    };
    result.validate()?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn conversions_round_trip_and_preserve_noncoordinate_fields() {
        let mut original = Position::new(31.2304, 121.4737);
        original.altitude = 42.0;
        for system in [CoordinateSystem::Gcj02, CoordinateSystem::Bd09] {
            let projected = convert(&original, CoordinateSystem::Wgs84, system).unwrap();
            let result = convert(&projected, system, CoordinateSystem::Wgs84).unwrap();
            assert!(crate::route::distance(&original, &result) < 0.15);
            assert_eq!(result.altitude, 42.0);
        }
        let overseas = Position::new(51.5, -0.1);
        assert_eq!(
            convert(&overseas, CoordinateSystem::Gcj02, CoordinateSystem::Wgs84).unwrap(),
            overseas
        );
    }
}
