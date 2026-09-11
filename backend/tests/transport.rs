use base64::{Engine, engine::general_purpose::STANDARD};
use justlocation_backend::transport::decode_request;

#[test]
fn accepts_one_utf8_json_request_and_rejects_extra_frames() {
    let json = r#"{"version":1,"op":"status"}"#;
    assert_eq!(decode_request(&STANDARD.encode(json)).unwrap(), json);
    assert!(decode_request("not base64!").is_err());
    assert!(decode_request(&STANDARD.encode([0xff])).is_err());
    assert!(decode_request(&STANDARD.encode(format!("{json}\n{json}"))).is_err());
    assert!(decode_request(&STANDARD.encode("a".repeat(65_537))).is_err());
}
