use std::{fs, path::Path, process::Command};

#[test]
fn exported_trip_is_the_viewer_recording() {
    let output = Command::new(env!("CARGO_BIN_EXE_elevator-sil"))
        .output()
        .unwrap();
    assert!(output.status.success());
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../viz/recordings/single-trip/timeline.json");
    assert_eq!(output.stdout, fs::read(path).unwrap());
}
