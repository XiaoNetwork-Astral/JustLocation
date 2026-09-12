use crate::{
    Position,
    route::{distance, interpolate},
};

/// Cut at most one quarter of either adjacent leg and sample a quadratic corner curve.
/// The source plan, endpoints, altitude interpolation and dateline wrapping are preserved.
pub fn corners(points: &[Position], radius: f64) -> Vec<Position> {
    if points.len() < 3 || radius <= 0.0 {
        return points.to_vec();
    }
    let mut path = vec![points[0].clone()];
    for window in points.windows(3) {
        let (a, b, c) = (&window[0], &window[1], &window[2]);
        let incoming = distance(a, b);
        let outgoing = distance(b, c);
        let cut = radius.min(incoming * 0.25).min(outgoing * 0.25);
        if cut < 0.1 {
            path.push(b.clone());
            continue;
        }
        let entry = interpolate(b, a, cut / incoming);
        let exit = interpolate(b, c, cut / outgoing);
        path.push(entry.clone());
        for step in 1..=12 {
            let t = step as f64 / 12.0;
            let candidate = interpolate(&interpolate(&entry, b, t), &interpolate(b, &exit, t), t);
            if distance(path.last().unwrap(), &candidate) >= 0.01 {
                path.push(candidate);
            }
        }
    }
    path.push(points.last().unwrap().clone());
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_endpoints_and_smooths_a_short_dateline_corner() {
        let points = vec![
            Position::new(0.0, 179.9999),
            Position::new(0.0, -179.9999),
            Position::new(0.0001, -179.9999),
        ];
        let original = points.clone();
        let path = corners(&points, 5.0);
        assert_eq!(points, original);
        assert_eq!(path.first(), points.first());
        assert_eq!(path.last(), points.last());
        assert!(path.len() > 3);
        assert!(!path.contains(&points[1]));
        for point in &path {
            point.validate().unwrap();
        }
        assert!(path.windows(2).all(|pair| distance(&pair[0], &pair[1]) > 0.0));
        assert!(
            path.windows(2).map(|p| distance(&p[0], &p[1])).sum::<f64>()
                < points.windows(2).map(|p| distance(&p[0], &p[1])).sum::<f64>()
        );
    }
}
