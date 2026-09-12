use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn executable_imports_shared_codes_without_a_daemon() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_justlocationd"))
        .args(["scode", "import", "--without-cells"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(include_bytes!("fixtures/address.scode")).unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    assert!(result.stderr.is_empty());
    let value: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(value["from"], 2);
    assert_ne!(value["id"], "fixture-original");
    assert!(value.get("nearbyCells").is_none());
    assert_eq!(value["nearbyWifis"][0]["SSID"], "测试网络");
}
