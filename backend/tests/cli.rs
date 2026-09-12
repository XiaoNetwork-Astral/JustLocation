use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    process::{Command, Output, Stdio},
};

fn run(directory: &std::path::Path, args: &[&str], input: Option<&str>) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_justlocationd"))
        .arg("--data-dir")
        .arg(directory)
        .arg("--json")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(input) = input {
        child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    }
    child.wait_with_output().unwrap()
}
fn json_output(output: Output) -> Value {
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(output.stderr.is_empty());
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn long_gpx_and_json_preserve_all_points_and_segments_in_small_catalogs() {
    let directory =
        std::env::temp_dir().join(format!("justlocation-cli-long-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let points: Vec<_> = (0..10000).map(|i| json!({"latitude":31.0+i as f64*0.00001,"longitude":121.,"altitude":0.,"accuracy":5.,"speed":0.,"bearing":0.})).collect();
    let plan =
        json!({"points":points,"breaks":[5000],"speed":2.,"repeat_count":1,"repeat_delay":0.});
    let saved =
        json_output(run(&directory, &["route", "import", "Long track"], Some(&plan.to_string())));
    let id = saved["id"].as_str().unwrap();
    assert_eq!(saved["point_count"], 10000);
    assert!(fs::metadata(directory.join("library.json")).unwrap().len() < 2048);
    let exported = json_output(run(&directory, &["route", "export", id], None));
    assert!(exported == plan, "route JSON round trip must preserve every coordinate and segment");
    let gpx = run(&directory, &["route", "export-gpx", id], None);
    assert!(gpx.status.success());
    let imported = json_output(run(
        &directory,
        &["route", "import-gpx", "--speed", "2"],
        Some(std::str::from_utf8(&gpx.stdout).unwrap()),
    ));
    let roundtrip = json_output(run(
        &directory,
        &["route", "export", imported[0]["id"].as_str().unwrap()],
        None,
    ));
    assert!(roundtrip == plan, "GPX round trip must retain all points and breaks");
    let page =
        json_output(run(&directory, &["route", "page", "--id", id, "--offset", "4992"], None));
    assert_eq!(page["points"].as_array().unwrap().len(), 128);
    assert_eq!(page["breaks"], json!([5000]));
    let backup = run(&directory, &["backup", "export"], None);
    assert!(backup.status.success());
    json_output(run(
        &directory,
        &["backup", "import", "--replace"],
        Some(std::str::from_utf8(&backup.stdout).unwrap()),
    ));
    let restored = json_output(run(&directory, &["route", "export", id], None));
    assert!(
        restored == plan,
        "backup must contain full route data rather than local file references"
    );
    for entry in fs::read_dir(directory.join("routes")).unwrap() {
        fs::remove_file(entry.unwrap().path()).unwrap();
    }
    fs::remove_dir(directory.join("routes")).unwrap();
    for entry in fs::read_dir(&directory).unwrap() {
        fs::remove_file(entry.unwrap().path()).unwrap();
    }
    fs::remove_dir(directory).unwrap();
}
#[test]
fn offline_commands_round_trip_metadata_routes_keys_and_fail_without_corrupting_data() {
    let directory = std::env::temp_dir().join(format!("justlocation-cli-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let saved = json_output(run(
        &directory,
        &["place", "save", "Test Place", "--lat", "-33.123456789", "--lon", "151.234567891"],
        None,
    ));
    let id = saved["id"].as_str().unwrap();
    assert_eq!(saved["latitude"], -33.123456789);
    assert_eq!(json_output(run(&directory, &["place", "pin", id, "true"], None))["pinned"], true);
    let exported = run(&directory, &["place", "export", id], None);
    assert!(exported.status.success());
    let imported = json_output(run(
        &directory,
        &["place", "import"],
        Some(std::str::from_utf8(&exported.stdout).unwrap()),
    ));
    assert_ne!(saved["id"], imported["id"]);
    assert_eq!(imported["pinned"], true);
    let path = directory.join("library.json");
    let original = fs::read(&path).unwrap();
    let invalid = run(&directory, &["place", "import"], Some("invalid"));
    assert_eq!(invalid.status.code(), Some(1));
    assert!(invalid.stdout.is_empty());
    assert_eq!(original, fs::read(&path).unwrap());
    let position = |lat| json!({"latitude":lat,"longitude":121.0,"altitude":0.0,"accuracy":5.0,"speed":0.0,"bearing":0.0});
    let plan = json!({"points":[position(31.0),position(31.001)],"speed":2.0,"repeat_count":2,"repeat_delay":1.0});
    let route = json_output(run(&directory, &["route", "import", "Walk"], Some(&plan.to_string())));
    assert_eq!(
        json_output(run(&directory, &["route", "export", route["id"].as_str().unwrap()], None)),
        plan
    );
    let bad = run(&directory, &["route", "start", "--input", "a", "--id", "b"], None);
    assert_eq!(bad.status.code(), Some(2));
    assert!(bad.stdout.is_empty());
    let key = "fixture-key-not-a-real-credential";
    let keys = run(&directory, &["maps", "key", "amap"], Some(key));
    assert!(keys.status.success());
    assert!(!String::from_utf8_lossy(&keys.stdout).contains(key));
    let auto =
        json_output(run(&directory, &["cells", "dataset", "auto", "true", "--mcc", "460"], None));
    assert_eq!(auto["settings"]["dataset_auto_update"], true);
    let disabled = json_output(run(&directory, &["cells", "dataset", "auto", "false"], None));
    assert_eq!(disabled["settings"]["dataset_mcc"], 460);
    assert_eq!(disabled["settings"]["dataset_auto_update"], false);
    let converted = json_output(run(
        &directory,
        &["coordinates", "--lat", "-33", "--lon", "151", "--to", "wgs84"],
        None,
    ));
    assert_eq!(converted["latitude"], -33.0);
    let backup = run(&directory, &["backup", "export"], None);
    assert!(backup.status.success());
    let imported = json_output(run(
        &directory,
        &["backup", "import"],
        Some(std::str::from_utf8(&backup.stdout).unwrap()),
    ));
    assert_eq!(imported["places"], 2);
    assert_eq!(json_output(run(&directory, &["place", "list"], None)).as_array().unwrap().len(), 4);
    let restored = json_output(run(
        &directory,
        &["backup", "import", "--replace"],
        Some(std::str::from_utf8(&backup.stdout).unwrap()),
    ));
    assert_eq!(restored["replaced"], true);
    assert_eq!(json_output(run(&directory, &["place", "list"], None)).as_array().unwrap().len(), 2);
    for entry in fs::read_dir(directory.join("routes")).unwrap() {
        fs::remove_file(entry.unwrap().path()).unwrap();
    }
    fs::remove_dir(directory.join("routes")).unwrap();
    for entry in fs::read_dir(&directory).unwrap() {
        fs::remove_file(entry.unwrap().path()).unwrap();
    }
    fs::remove_dir(directory).unwrap();
}
