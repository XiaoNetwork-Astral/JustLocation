//! Reverse geocoding and Google place results, with explicit coordinate boundaries.
use crate::{
    Position,
    cell_providers::HttpRequest,
    coordinates::{CoordinateSystem, convert},
    maps::MapProvider,
};
use serde_json::{Value, json};

pub fn link(provider: MapProvider, position: &Position) -> Result<Value, String> {
    position.validate()?;
    if provider != MapProvider::Google {
        return Err("external map links currently support google only".into());
    }
    Ok(
        json!({"provider":provider,"url":format!("https://www.google.com/maps/search/?api=1&query={:.8}%2C{:.8}",position.latitude,position.longitude),"requires_key":false}),
    )
}

pub fn request(
    provider: MapProvider,
    key: &str,
    position: &Position,
) -> Result<HttpRequest, String> {
    position.validate()?;
    let gcj = convert(position, CoordinateSystem::Wgs84, CoordinateSystem::Gcj02)?;
    let (url, query): (&str, Vec<(&str, String)>) = match provider {
        MapProvider::Amap => (
            "https://restapi.amap.com/v3/geocode/regeo",
            vec![
                ("key", key.into()),
                ("location", format!("{:.6},{:.6}", gcj.longitude, gcj.latitude)),
                ("extensions", "all".into()),
                ("output", "JSON".into()),
            ],
        ),
        MapProvider::Tencent => (
            "https://apis.map.qq.com/ws/geocoder/v1/",
            vec![
                ("key", key.into()),
                ("location", format!("{:.6},{:.6}", gcj.latitude, gcj.longitude)),
                ("get_poi", "1".into()),
                ("output", "json".into()),
            ],
        ),
        MapProvider::Baidu => (
            "https://api.map.baidu.com/reverse_geocoding/v3/",
            vec![
                ("ak", key.into()),
                ("location", format!("{:.8},{:.8}", position.latitude, position.longitude)),
                ("coordtype", "wgs84ll".into()),
                ("ret_coordtype", "gcj02ll".into()),
                ("extensions_poi", "1".into()),
                ("output", "json".into()),
            ],
        ),
        MapProvider::Google => (
            "https://maps.googleapis.com/maps/api/geocode/json",
            vec![
                ("key", key.into()),
                ("latlng", format!("{:.8},{:.8}", position.latitude, position.longitude)),
            ],
        ),
    };
    Ok(HttpRequest {
        url: url.into(),
        query: query.into_iter().map(|(k, v)| (k.into(), v)).collect(),
        body: None,
        bearer: None,
    })
}
fn coordinate(lat: &Value, lon: &Value, system: CoordinateSystem) -> Result<Position, String> {
    let point = Position::new(
        lat.as_f64().ok_or("missing result latitude")?,
        lon.as_f64().ok_or("missing result longitude")?,
    );
    convert(&point, system, CoordinateSystem::Wgs84).map_err(str::to_owned)
}
fn text(value: &Value) -> &str {
    value.as_str().unwrap_or("")
}
fn components(value: &Value) -> Value {
    let mut fields = serde_json::Map::new();
    for key in [
        "country",
        "nation",
        "province",
        "city",
        "district",
        "adcode",
        "citycode",
        "township",
        "town",
        "street",
        "street_number",
        "country_code",
    ] {
        if value[key].is_string() || value[key].is_number() {
            fields.insert(key.into(), value[key].clone());
        }
    }
    for key in ["street", "number"] {
        if value["streetNumber"][key].is_string() {
            fields.insert(key.into(), value["streetNumber"][key].clone());
        }
    }
    Value::Object(fields)
}

pub fn parse(provider: MapProvider, input: &Position, data: &Value) -> Result<Value, String> {
    input.validate()?;
    let mut addresses = Vec::new();
    let mut pois = Vec::new();
    if provider == MapProvider::Google {
        if data["status"] != "OK" && data["status"] != "ZERO_RESULTS" {
            return Err("Google geocoding rejected the request; check API permissions, billing, coverage and quota".into());
        }
        for item in data["results"].as_array().ok_or("missing reverse geocoding results")? {
            let p = &item["geometry"]["location"];
            let position = coordinate(&p["lat"], &p["lng"], CoordinateSystem::Wgs84)?;
            let parts =
                item["address_components"].as_array().ok_or("missing address components")?;
            let parts:Vec<_>=parts.iter().map(|part|json!({"long_name":text(&part["long_name"]),"short_name":text(&part["short_name"]),"types":part["types"]})).collect();
            addresses.push(json!({"id":text(&item["place_id"]),"address":item["formatted_address"].as_str().ok_or("missing formatted address")?,"position":position,"components":parts,"types":item["types"]}));
        }
    } else {
        let (success, result) = if provider == MapProvider::Amap {
            (data["status"] == "1", &data["regeocode"])
        } else {
            (data["status"] == 0, &data["result"])
        };
        if !success {
            return Err(
                "reverse geocoding rejected the request; check key permissions, coverage and quota"
                    .into(),
            );
        }
        if !result.is_object() {
            return Err("missing reverse geocoding result".into());
        }
        let component = if provider == MapProvider::Tencent {
            &result["address_component"]
        } else {
            &result["addressComponent"]
        };
        let address = if provider == MapProvider::Tencent {
            &result["address"]
        } else {
            &result["formatted_address"]
        };
        let position = if provider == MapProvider::Amap {
            None
        } else {
            Some(coordinate(
                &result["location"]["lat"],
                &result["location"]["lng"],
                CoordinateSystem::Gcj02,
            )?)
        };
        let mut parts = components(component);
        if provider == MapProvider::Tencent && result["ad_info"]["adcode"].is_string() {
            parts["adcode"] = result["ad_info"]["adcode"].clone();
        }
        addresses.push(json!({"id":"","address":address.as_str().ok_or("missing formatted address")?,"position":position,"components":parts}));
        if let Some(items) = result.get("pois") {
            for item in items.as_array().ok_or("invalid reverse geocoding POI list")? {
                let position = match provider {
                    MapProvider::Amap => {
                        let (lon, lat) = item["location"]
                            .as_str()
                            .and_then(|s| s.split_once(','))
                            .ok_or("missing POI coordinates")?;
                        coordinate(
                            &json!(lat.parse::<f64>().map_err(|_| "invalid POI latitude")?),
                            &json!(lon.parse::<f64>().map_err(|_| "invalid POI longitude")?),
                            CoordinateSystem::Gcj02,
                        )?
                    }
                    MapProvider::Tencent => coordinate(
                        &item["location"]["lat"],
                        &item["location"]["lng"],
                        CoordinateSystem::Gcj02,
                    )?,
                    MapProvider::Baidu => coordinate(
                        &item["point"]["y"],
                        &item["point"]["x"],
                        CoordinateSystem::Gcj02,
                    )?,
                    MapProvider::Google => unreachable!(),
                };
                pois.push(json!({"id":text(item.get("id").unwrap_or(&item["uid"])),"name":text(if provider==MapProvider::Tencent {&item["title"]} else {&item["name"]}),"address":text(item.get("address").unwrap_or(&item["addr"])),"position":position}));
            }
        }
    }
    Ok(
        json!({"provider":provider,"coordinate_system":"wgs84","position":input,"addresses":addresses,"pois":pois,"attribution":if provider==MapProvider::Google {"Google Maps"} else {""}}),
    )
}

pub fn google_places(data: &Value) -> Result<Vec<Value>, String> {
    if !data.is_object() || data.get("error").is_some() {
        return Err(
            "Google Places rejected the request; check API permissions, billing and quota".into()
        );
    }
    let Some(items) = data.get("places") else {
        return Ok(Vec::new());
    };
    items.as_array().ok_or("invalid Google place list")?.iter().take(20).map(|item| {
        let location=&item["location"];
        let position=coordinate(&location["latitude"],&location["longitude"],CoordinateSystem::Wgs84)?;
        Ok(json!({"id":text(&item["id"]),"name":item["displayName"]["text"].as_str().ok_or("missing Google place name")?,"address":text(&item["formattedAddress"]),"position":position,"attributions":item.get("attributions").cloned().unwrap_or(json!([]))}))
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reverse_results_keep_query_separate_and_convert_pois() {
        let query = Position::new(31.2304, 121.4737);
        let location = json!({"lat":31.22845773757727,"lng":121.47822305927693});
        for (provider, data) in [
            (
                MapProvider::Amap,
                json!({"status":"1","regeocode":{"formatted_address":"Shanghai","addressComponent":{"country":"China","province":"Shanghai","city":[],"adcode":"310000"},"pois":[{"id":"a","name":"Point","location":"121.47822305927693,31.22845773757727"}]}}),
            ),
            (
                MapProvider::Tencent,
                json!({"status":0,"result":{"location":location,"address":"Shanghai","address_component":{"nation":"China","province":"Shanghai","city":"Shanghai"},"ad_info":{"adcode":"310000"},"pois":[{"id":"a","title":"Point","location":location}]}}),
            ),
            (
                MapProvider::Baidu,
                json!({"status":0,"result":{"location":location,"formatted_address":"Shanghai","addressComponent":{"country":"China","province":"Shanghai","city":"Shanghai","adcode":"310000"},"pois":[{"uid":"a","name":"Point","point":{"x":location["lng"],"y":location["lat"]}}]}}),
            ),
        ] {
            let request = request(provider, "private-key", &query).unwrap();
            assert!(request.body.is_none());
            if provider == MapProvider::Baidu {
                assert!(request.query.contains(&("coordtype".into(), "wgs84ll".into())));
            }
            let result = parse(provider, &query, &data).unwrap();
            assert_eq!(result["position"], json!(query));
            assert_eq!(result["addresses"][0]["components"]["adcode"], "310000");
            let point: Position =
                serde_json::from_value(result["pois"][0]["position"].clone()).unwrap();
            assert!(crate::route::distance(&query, &point) < 0.01);
            if provider == MapProvider::Amap {
                assert!(result["addresses"][0]["position"].is_null());
            }
        }
    }
    #[test]
    fn google_keeps_wgs84_attribution_and_distinguishes_empty_from_failure() {
        let query = Position::new(-33.8568, 151.2153);
        let data = json!({"status":"OK","results":[{"place_id":"a","formatted_address":"Sydney","address_components":[{"long_name":"Australia","short_name":"AU","types":["country"]}],"geometry":{"location":{"lat":-33.8568,"lng":151.2153}},"types":["premise"]}]});
        let result = parse(MapProvider::Google, &query, &data).unwrap();
        assert_eq!(result["addresses"][0]["position"], json!(query));
        assert_eq!(result["attribution"], "Google Maps");
        assert_eq!(result["addresses"][0]["components"][0]["short_name"], "AU");
        assert!(
            parse(MapProvider::Google, &query, &json!({"status":"ZERO_RESULTS","results":[]}))
                .unwrap()["addresses"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(
            !parse(
                MapProvider::Google,
                &query,
                &json!({"status":"REQUEST_DENIED","error_message":"private-key"})
            )
            .unwrap_err()
            .contains("private-key")
        );
        let places=google_places(&json!({"places":[{"id":"a","displayName":{"text":"Sydney"},"location":{"latitude":-33.8568,"longitude":151.2153},"attributions":[{"provider":"example"}]}]})).unwrap();
        assert_eq!(places[0]["position"], json!(query));
        assert_eq!(places[0]["attributions"][0]["provider"], "example");
        assert!(google_places(&json!({})).unwrap().is_empty());
        assert!(google_places(&json!({"error":{"message":"private-key"}})).is_err());
        assert!(google_places(&json!({"places":[{"displayName":{"text":"bad"},"location":{"latitude":999,"longitude":0}}]})).is_err());
        let url = link(MapProvider::Google, &query).unwrap();
        assert_eq!(
            url["url"],
            "https://www.google.com/maps/search/?api=1&query=-33.85680000%2C151.21530000"
        );
        assert_eq!(url["requires_key"], false);
    }
}
