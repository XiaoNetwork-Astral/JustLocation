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
                for (i, segment) in segments.iter().enumerate() {
                    let name = if segments.len() > 1 {
                        format!("{name} ({})", i + 1)
                    } else {
                        name.to_owned()
                    };
                    routes.push((name, plan(*segment, "trkpt", speed)?));
                }
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
        if points
            .last()
            .is_none_or(|p| p.latitude != point.latitude || p.longitude != point.longitude)
        {
            points.push(point);
        }
    }
    let route = Route { points, speed, repeat_count: 1, repeat_delay: 0.0 };
    Playback::new(route.clone(), std::time::Instant::now())?;
    Ok(route)
}
pub(super) fn export(name: &str, plan: &Route) -> String {
    let name = name.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    let mut text = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<gpx version=\"1.1\" creator=\"JustLocation\" xmlns=\"http://www.topografix.com/GPX/1/1\"><rte><name>{name}</name>\n"
    );
    for p in &plan.points {
        text.push_str(&format!(
            "<rtept lat=\"{}\" lon=\"{}\"><ele>{}</ele></rtept>\n",
            p.latitude, p.longitude, p.altitude
        ));
    }
    text.push_str("</rte></gpx>\n");
    text
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn namespaces_segments_precision_and_duplicate_points() {
        let text = r#"<g:gpx xmlns:g="http://www.topografix.com/GPX/1/1"><g:trk><g:name>A &amp; B</g:name><g:trkseg><g:trkpt lat="31.123456789" lon="121"/><g:trkpt lat="31.123456789" lon="121"/><g:trkpt lat="31.2" lon="121"><g:ele>-5</g:ele></g:trkpt></g:trkseg><g:trkseg><g:trkpt lat="32" lon="122"/><g:trkpt lat="32.1" lon="122"/></g:trkseg></g:trk></g:gpx>"#;
        let routes = parse(text, 2.0).unwrap();
        assert_eq!(routes.len(), 2);
        assert_eq!(routes[0].0, "A & B (1)");
        let roundtrip = parse(&export(&routes[0].0, &routes[0].1), 2.0).unwrap();
        assert_eq!(roundtrip[0].1.points, routes[0].1.points);
        assert!(parse("<gpx><rte><rtept lat='NaN' lon='0'/></rte></gpx>", 2.0).is_err());
    }
}
