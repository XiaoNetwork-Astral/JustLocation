//! Provider routing adapters. Input, stored geometry and playback all use WGS84.
use crate::{
    Position,
    cell_providers::HttpRequest,
    coordinates::{CoordinateSystem, convert},
    maps::MapProvider,
    route::{MAX_POINTS, Playback, Route},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum TravelMode {
    Walking,
    Cycling,
    Driving,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Coordinate {
    pub latitude: f64,
    pub longitude: f64,
}
impl Coordinate {
    fn position(&self) -> Result<Position, String> {
        let p = Position::new(self.latitude, self.longitude);
        p.validate()?;
        Ok(p)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanRequest {
    pub provider: MapProvider,
    pub mode: TravelMode,
    pub origin: Coordinate,
    pub destination: Coordinate,
    #[serde(default)]
    pub waypoints: Vec<Coordinate>,
    /// Playback speed is independent of the provider's estimated journey duration.
    pub speed: f64,
}
impl PlanRequest {
    pub fn validate(&self) -> Result<(), String> {
        self.origin.position()?;
        self.destination.position()?;
        for point in &self.waypoints {
            point.position()?;
        }
        if !self.speed.is_finite() || self.speed <= 0.0 || self.speed > 1000.0 {
            return Err("route speed must be greater than 0 and at most 1000 m/s".into());
        }
        let max = if self.mode == TravelMode::Driving {
            match self.provider {
                MapProvider::Amap => 16,
                MapProvider::Tencent => 30,
                MapProvider::Baidu => 10,
            }
        } else {
            0
        };
        if self.waypoints.len() > max {
            return Err(format!(
                "unsupported waypoint count: this provider/mode accepts at most {max}; account permissions may impose a lower limit"
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Segment {
    pub first: usize,
    pub last: usize,
    pub distance_m: Option<f64>,
    pub duration_s: Option<f64>,
    pub road_name: String,
    pub instruction: String,
    /// Provider-specific road/navigation codes, not inferred building or elevation data.
    pub attributes: std::collections::BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Geometry {
    pub provider: MapProvider,
    pub mode: TravelMode,
    pub distance_m: f64,
    pub duration_s: f64,
    pub segments: Vec<Segment>,
}
impl Geometry {
    pub fn validate(&self, points: usize) -> Result<(), &'static str> {
        if !self.distance_m.is_finite()
            || self.distance_m < 0.0
            || !self.duration_s.is_finite()
            || self.duration_s < 0.0
            || self.segments.is_empty()
            || self.segments.len() > MAX_POINTS
        {
            return Err("invalid planned route metadata");
        }
        for s in &self.segments {
            if s.first > s.last
                || s.last >= points
                || [s.distance_m, s.duration_s]
                    .into_iter()
                    .flatten()
                    .any(|x| !x.is_finite() || x < 0.0)
            {
                return Err("invalid planned route segment");
            }
        }
        Ok(())
    }
}

pub fn capabilities() -> Value {
    json!({"coordinate_system":"wgs84","providers":([MapProvider::Amap,MapProvider::Tencent,MapProvider::Baidu].map(|p| json!({
        "provider":p,"place_search":true,"route_modes":["walking","cycling","driving"],
        "driving_waypoints_max":match p {MapProvider::Amap=>16,MapProvider::Tencent=>30,MapProvider::Baidu=>10},
        "walking_cycling_waypoints_max":0,"key_type":"WebService","quota":"account-dependent",
        "independent_of_base_map":true,"map_matching":false,"building_avoidance":false,
        "level_constraints":false,"planned_geometry":"preserved; corner smoothing and horizontal drift disabled"
    })))})
}

pub fn request(plan: &PlanRequest, key: &str) -> Result<HttpRequest, String> {
    plan.validate()?;
    let coordinate = |p: &Coordinate| -> Result<String, String> {
        let p = if plan.provider == MapProvider::Baidu {
            p.position()?
        } else {
            convert(&p.position()?, CoordinateSystem::Wgs84, CoordinateSystem::Gcj02)?
        };
        Ok(if plan.provider == MapProvider::Amap {
            format!("{:.6},{:.6}", p.longitude, p.latitude)
        } else {
            format!("{:.6},{:.6}", p.latitude, p.longitude)
        })
    };
    let mode = match plan.mode {
        TravelMode::Walking => "walking",
        TravelMode::Driving => "driving",
        TravelMode::Cycling => {
            if plan.provider == MapProvider::Baidu {
                "riding"
            } else {
                "bicycling"
            }
        }
    };
    let (url, mut query): (String, Vec<(&str, String)>) = match plan.provider {
        MapProvider::Amap => (
            format!("https://restapi.amap.com/v5/direction/{mode}"),
            vec![
                ("key", key.into()),
                ("origin", coordinate(&plan.origin)?),
                ("destination", coordinate(&plan.destination)?),
                ("show_fields", "cost,polyline,navi".into()),
                (
                    if plan.mode == TravelMode::Driving { "strategy" } else { "alternative_route" },
                    if plan.mode == TravelMode::Driving { "32" } else { "3" }.into(),
                ),
            ],
        ),
        MapProvider::Tencent => (
            format!("https://apis.map.qq.com/ws/direction/v1/{mode}/"),
            vec![
                ("key", key.into()),
                ("from", coordinate(&plan.origin)?),
                ("to", coordinate(&plan.destination)?),
                ("output", "json".into()),
            ],
        ),
        MapProvider::Baidu => (
            format!("https://api.map.baidu.com/directionlite/v1/{mode}"),
            vec![
                ("ak", key.into()),
                ("origin", coordinate(&plan.origin)?),
                ("destination", coordinate(&plan.destination)?),
                ("coord_type", "wgs84".into()),
                ("ret_coordtype", "gcj02".into()),
            ],
        ),
    };
    if plan.provider == MapProvider::Tencent && plan.mode == TravelMode::Driving {
        query.extend([("get_mp", "1".into()), ("no_step", "0".into())]);
    }
    if plan.provider == MapProvider::Baidu && plan.mode == TravelMode::Driving {
        query.push(("steps_info", "1".into()));
    }
    if !plan.waypoints.is_empty() {
        query.push((
            "waypoints",
            plan.waypoints
                .iter()
                .map(coordinate)
                .collect::<Result<Vec<_>, _>>()?
                .join(if plan.provider == MapProvider::Baidu { "|" } else { ";" }),
        ));
    }
    Ok(HttpRequest {
        url,
        query: query.into_iter().map(|(k, v)| (k.into(), v)).collect(),
        body: None,
        bearer: None,
    })
}

fn number(v: &Value) -> Option<f64> {
    v.as_f64().or_else(|| v.as_str()?.parse().ok()).filter(|x| x.is_finite() && *x >= 0.0)
}
fn point(lat: f64, lon: f64) -> Result<Position, String> {
    convert(&Position::new(lat, lon), CoordinateSystem::Gcj02, CoordinateSystem::Wgs84)
        .map_err(str::to_owned)
}
fn polyline(value: &Value) -> Result<Vec<Position>, String> {
    let text = value.as_str().ok_or("route step has no full geometry")?;
    let mut points = Vec::new();
    for pair in text.split(';') {
        let (lon, lat) = pair.split_once(',').ok_or("invalid route polyline")?;
        points.push(point(
            lat.parse().map_err(|_| "invalid route latitude")?,
            lon.parse().map_err(|_| "invalid route longitude")?,
        )?);
        if points.len() > MAX_POINTS {
            return Err("route exceeds 100000 points".into());
        }
    }
    Ok(points)
}
fn tencent_polyline(value: &Value) -> Result<Vec<Position>, String> {
    let values = value.as_array().ok_or("route has no full geometry")?;
    if values.len() < 4 || values.len() % 2 != 0 || values.len() > MAX_POINTS * 2 {
        return Err("invalid route polyline length".into());
    }
    let mut previous = [0.0; 2];
    let mut points = Vec::new();
    for (index, pair) in values.chunks_exact(2).enumerate() {
        for i in 0..2 {
            let value = pair[i].as_f64().filter(|x| x.is_finite()).ok_or("invalid route delta")?;
            previous[i] = if index == 0 { value } else { previous[i] + value / 1e6 };
        }
        points.push(point(previous[0], previous[1])?);
    }
    Ok(points)
}

pub fn parse(plan: &PlanRequest, data: &Value) -> Result<Vec<Route>, String> {
    plan.validate()?;
    let success = if plan.provider == MapProvider::Amap {
        data["status"] == "1"
    } else {
        data["status"] == 0
    };
    if !success {
        return Err(
            "route service rejected the request; check key permissions, coverage and quota".into(),
        );
    }
    let paths = if plan.provider == MapProvider::Amap {
        &data["route"]["paths"]
    } else {
        &data["result"]["routes"]
    };
    let paths = paths.as_array().ok_or("route service returned no candidate list")?;
    if paths.is_empty() {
        return Err("no route found for the requested points and travel mode".into());
    }
    let mut result = Vec::new();
    for path in paths {
        let duration = if plan.provider == MapProvider::Amap {
            &path["cost"]["duration"]
        } else {
            &path["duration"]
        };
        let mut geometry = Geometry {
            provider: plan.provider,
            mode: plan.mode,
            distance_m: number(&path["distance"]).ok_or("missing route distance")?,
            duration_s: number(duration).ok_or("missing route duration")?
                * if plan.provider == MapProvider::Tencent { 60.0 } else { 1.0 },
            segments: Vec::new(),
        };
        let mut points = if plan.provider == MapProvider::Tencent {
            tencent_polyline(&path["polyline"])?
        } else {
            Vec::new()
        };
        let steps =
            path["steps"].as_array().filter(|x| !x.is_empty()).ok_or("route has no steps")?;
        for step in steps {
            let (first, last) = if plan.provider == MapProvider::Tencent {
                let indices = step["polyline_idx"]
                    .as_array()
                    .filter(|x| x.len() == 2)
                    .ok_or("missing route step indices")?;
                let a = indices[0].as_u64().ok_or("invalid route step index")?;
                let b = indices[1].as_u64().ok_or("invalid route step index")?;
                if a > b || b >= points.len() as u64 * 2 {
                    return Err("route step indices outside geometry".into());
                }
                ((a / 2) as usize, (b / 2) as usize)
            } else {
                let mut line = polyline(if plan.provider == MapProvider::Amap {
                    &step["polyline"]
                } else {
                    &step["path"]
                })?;
                // Only coalesce the shared endpoint, never simplify or connect missing geometry.
                let first = if let Some(last) = points.last() {
                    if *last != line[0] {
                        return Err("route step geometry is disconnected".into());
                    }
                    line.remove(0);
                    points.len() - 1
                } else {
                    0
                };
                if points.len() + line.len() > MAX_POINTS {
                    return Err("route exceeds 100000 points".into());
                }
                points.extend(line);
                (first, points.len() - 1)
            };
            let attributes = [
                "navi",
                "walk_type",
                "type",
                "road_type",
                "road_types",
                "road_class",
                "action",
                "assistant_action",
                "act_desc",
                "accessorial_desc",
                "leg_index",
            ]
            .into_iter()
            .filter_map(|key| step.get(key).map(|v| (key.to_owned(), v.clone())))
            .collect();
            geometry.segments.push(Segment {
                first,
                last,
                distance_m: number(step.get("step_distance").unwrap_or(&step["distance"])),
                duration_s: number(if plan.provider == MapProvider::Amap {
                    &step["cost"]["duration"]
                } else {
                    &step["duration"]
                })
                .map(|x| x * if plan.provider == MapProvider::Tencent { 60.0 } else { 1.0 }),
                road_name: step["road_name"].as_str().unwrap_or("").into(),
                instruction: step["instruction"].as_str().unwrap_or("").into(),
                attributes,
            });
        }
        let route = Route {
            points,
            breaks: vec![],
            speed: plan.speed,
            repeat_count: 1,
            repeat_delay: 0.0,
            geometry: Some(geometry),
        };
        Playback::new(route.clone(), Instant::now())?;
        result.push(route);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn plan(provider: MapProvider, mode: TravelMode) -> PlanRequest {
        PlanRequest {
            provider,
            mode,
            origin: Coordinate { latitude: 31.2304, longitude: 121.4737 },
            destination: Coordinate { latitude: 31.2314, longitude: 121.4747 },
            waypoints: vec![],
            speed: 1.4,
        }
    }
    fn fixture(provider: MapProvider) -> Value {
        match provider {
            MapProvider::Amap => {
                json!({"status":"1","route":{"paths":[{"distance":"100","cost":{"duration":"90"},"steps":[{"step_distance":"50","road_name":"bridge","navi":{"walk_type":"22"},"polyline":"121.47822305927693,31.22845773757727;121.4783,31.2285"},{"step_distance":"50","polyline":"121.4783,31.2285;121.4783,31.2286"}]}]}})
            }
            MapProvider::Tencent => {
                json!({"status":0,"result":{"routes":[{"distance":100,"duration":1.5,"polyline":[31.22845773757727,121.47822305927693,42.26242273,76.94072307,100,0],"steps":[{"distance":100,"type":2,"polyline_idx":[0,5]}]}]}})
            }
            MapProvider::Baidu => {
                json!({"status":0,"result":{"routes":[{"distance":100,"duration":90,"steps":[{"distance":100,"path":"121.47822305927693,31.22845773757727;121.4783,31.2285;121.4783,31.2286","road_types":"3"}]}]}})
            }
        }
    }
    #[test]
    fn all_modes_convert_units_coordinates_and_preserve_geometry() {
        for provider in [MapProvider::Amap, MapProvider::Tencent, MapProvider::Baidu] {
            for mode in [TravelMode::Walking, TravelMode::Cycling, TravelMode::Driving] {
                let plan = plan(provider, mode);
                let request = request(&plan, "private-key").unwrap();
                assert!(request.url.starts_with("https://"));
                assert_eq!(request.body, None);
                if provider == MapProvider::Baidu {
                    assert!(request.query.contains(&("coord_type".into(), "wgs84".into())));
                }
                let parsed = parse(&plan, &fixture(provider)).unwrap();
                let route = &parsed[0];
                assert_eq!(route.points.len(), 3);
                assert!(
                    crate::route::distance(&route.points[0], &Position::new(31.2304, 121.4737))
                        < 0.01
                );
                let metadata = route.geometry.as_ref().unwrap();
                assert_eq!(metadata.duration_s, 90.0);
                assert_eq!(metadata.distance_m, 100.0);
                assert_eq!(metadata.segments.last().unwrap().last, 2);
                let now = Instant::now();
                let mut exact = Playback::new(route.clone(), now).unwrap();
                let mut smoothed = Playback::smoothed(route.clone(), now, 100.0).unwrap();
                for seconds in 0..60 {
                    let t = now + std::time::Duration::from_secs(seconds);
                    assert_eq!(exact.advance(t), smoothed.advance(t));
                }
                let encoded = serde_json::to_string(route).unwrap();
                let decoded: Route = serde_json::from_str(&encoded).unwrap();
                assert_eq!(decoded.points, route.points);
                assert_eq!(
                    serde_json::to_value(decoded.geometry).unwrap(),
                    serde_json::to_value(&route.geometry).unwrap()
                );
            }
        }
    }
    #[test]
    fn no_fallback_for_unreachable_incomplete_or_unsupported_routes() {
        let mut p = plan(MapProvider::Amap, TravelMode::Walking);
        p.waypoints.push(Coordinate { latitude: 31.23, longitude: 121.47 });
        assert!(request(&p, "key").err().unwrap().contains("unsupported waypoint"));
        p.waypoints.clear();
        for data in
            [json!({"status":"0","info":"private-key"}), json!({"status":"1","route":{"paths":[]}})]
        {
            let error = parse(&p, &data).unwrap_err();
            assert!(!error.contains("private-key"));
        }
        let mut data = fixture(MapProvider::Amap);
        data["route"]["paths"][0]["steps"][1]["polyline"] = json!("121.479,31.229;121.480,31.230");
        assert!(parse(&p, &data).unwrap_err().contains("disconnected"));
        data["route"]["paths"][0]["steps"][0]["polyline"] = Value::Null;
        assert!(parse(&p, &data).unwrap_err().contains("geometry"));
        let mut data = fixture(MapProvider::Tencent);
        data["result"]["routes"][0]["polyline"][0] = json!(999);
        assert!(parse(&plan(MapProvider::Tencent, TravelMode::Walking), &data).is_err());
    }
    #[test]
    fn multiple_candidates_and_long_routes_remain_selectable() {
        let p = plan(MapProvider::Baidu, TravelMode::Driving);
        let mut data = fixture(MapProvider::Baidu);
        let path = data["result"]["routes"][0].clone();
        data["result"]["routes"].as_array_mut().unwrap().push(path);
        let line = (0..10_000)
            .map(|i| format!("121.47,{:.6}", 31.23 + i as f64 / 1e6))
            .collect::<Vec<_>>()
            .join(";");
        data["result"]["routes"][1]["steps"][0]["path"] = json!(line);
        let parsed = parse(&p, &data).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[1].points.len(), 10_000);
        let directory = std::env::temp_dir()
            .join(format!("jl-planned-route-{}", crate::scode::new_id().unwrap()));
        let id = crate::route_store::save(&directory, &parsed[1]).unwrap();
        let loaded = crate::route_store::load(&directory, &id).unwrap();
        assert_eq!(loaded.points, parsed[1].points);
        assert_eq!(loaded.geometry.unwrap().segments[0].last, 9999);
        crate::route_store::remove(&directory, &id).unwrap();
        std::fs::remove_dir(directory.join("routes")).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }
}
