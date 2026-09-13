use super::Result;
use crate::{
    Position,
    route::{Playback, Route},
};

pub(super) fn parse(text: &str, speed: f64) -> Result<Vec<(String, Route)>> {
    // DTDs are disabled by the parser; GPX never needs external entities.
    let doc = roxmltree::Document::parse(text).map_err(|e| format!("invalid GPX: {e}"))?;
    let root = doc.root_element();
    if root.tag_name().name() != "gpx" {
        return Err("expected a GPX document".into());
    }
    let mut routes = Vec::new();
    for element in root.children().filter(|n| n.is_element()) {
        let name = element
            .children()
            .find(|n| n.has_tag_name("name"))
            .and_then(|n| n.text())
            .unwrap_or("Imported route")
            .trim();
        let name = if name.is_empty() { "Imported route" } else { name };
        match element.tag_name().name() {
            "rte" => routes.push((name.to_owned(), plan(element, "rtept", speed)?)),
            "trk" => {
                let segments: Vec<_> =
                    element.children().filter(|n| n.has_tag_name("trkseg")).collect();
                let mut track = Route {
                    geometry: None,
                    points: vec![],
                    breaks: vec![],
                    speed,
                    repeat_count: 1,
                    repeat_delay: 0.0,
                };
                for segment in segments {
                    let points = points(segment, "trkpt")?;
                    if points.is_empty() {
                        continue;
                    }
                    if !track.points.is_empty() {
                        track.breaks.push(track.points.len());
                    }
                    track.points.extend(points);
                }
                Playback::new(track.clone(), std::time::Instant::now())?;
                routes.push((name.to_owned(), track));
            }
            _ => {}
        }
    }
    if routes.is_empty() {
        return Err("GPX contains no routes or track segments".into());
    }
    Ok(routes)
}
fn plan(element: roxmltree::Node<'_, '_>, tag: &str, speed: f64) -> Result<Route> {
    let route = Route {
        geometry: None,
        points: points(element, tag)?,
        breaks: vec![],
        speed,
        repeat_count: 1,
        repeat_delay: 0.0,
    };
    Playback::new(route.clone(), std::time::Instant::now())?;
    Ok(route)
}
fn points(element: roxmltree::Node<'_, '_>, tag: &str) -> Result<Vec<Position>> {
    let mut points: Vec<Position> = Vec::new();
    for node in element.children().filter(|n| n.has_tag_name(tag)) {
        let number = |value: Option<&str>| {
            value
                .ok_or("missing GPX coordinate")?
                .trim()
                .parse::<f64>()
                .map_err(|_| "invalid GPX coordinate")
        };
        let mut point =
            Position::new(number(node.attribute("lat"))?, number(node.attribute("lon"))?);
        point.altitude = number(
            node.children().find(|n| n.has_tag_name("ele")).and_then(|n| n.text()).or(Some("0")),
        )?;
        point.validate()?;
        points.push(point);
        if points.len() > crate::route::MAX_POINTS {
            return Err("a route needs at most 100000 points".into());
        }
    }
    Ok(points)
}

pub(super) fn export(name: &str, plan: &Route) -> String {
    let name = name.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    let mut text = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<gpx version=\"1.1\" creator=\"JustLocation\" xmlns=\"http://www.topografix.com/GPX/1/1\"><trk><name>{name}</name><trkseg>\n"
    );
    for (i, p) in plan.points.iter().enumerate() {
        if plan.breaks.binary_search(&i).is_ok() {
            text.push_str("</trkseg><trkseg>\n");
        }
        text.push_str(&format!(
            "<trkpt lat=\"{}\" lon=\"{}\"><ele>{}</ele></trkpt>\n",
            p.latitude, p.longitude, p.altitude
        ));
    }
    text.push_str("</trkseg></trk></gpx>\n");
    text
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn namespaces_segments_precision_and_duplicate_points() {
        let text = r#"<g:gpx xmlns:g="http://www.topografix.com/GPX/1/1"><g:trk><g:name>A &amp; B</g:name><g:trkseg><g:trkpt lat="31.123456789" lon="121"/><g:trkpt lat="31.123456789" lon="121"/><g:trkpt lat="31.2" lon="121"><g:ele>-5</g:ele></g:trkpt></g:trkseg><g:trkseg><g:trkpt lat="32" lon="122"/><g:trkpt lat="32.1" lon="122"/></g:trkseg></g:trk></g:gpx>"#;
        let routes = parse(text, 2.0).unwrap();
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].1.points.len(), 5);
        assert_eq!(routes[0].1.breaks, vec![3]);
        assert_eq!(routes[0].0, "A & B");
        let roundtrip = parse(&export(&routes[0].0, &routes[0].1), 2.0).unwrap();
        assert_eq!(roundtrip[0].1.points, routes[0].1.points);
        assert_eq!(roundtrip[0].1.breaks, routes[0].1.breaks);
        assert!(parse("<gpx><rte><trkpt lat='NaN' lon='0'/></rte></gpx>", 2.0).is_err());
    }
}
