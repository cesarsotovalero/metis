use std::{path::PathBuf, process::Command};
fn cli(home: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_contextscope"))
        .arg("--home")
        .arg(home)
        .args(args)
        .output()
        .unwrap()
}
#[test]
fn offline_import_report_and_atomic_malformed_input() {
    let home = std::env::temp_dir().join(format!("contextscope-cli-{}", std::process::id()));
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/ordinary.jsonl");
    let import = cli(
        &home,
        &[
            "import",
            "--agent",
            "fixture",
            "--path",
            fixture.to_str().unwrap(),
        ],
    );
    assert!(
        import.status.success(),
        "{}",
        String::from_utf8_lossy(&import.stderr)
    );
    let output = cli(&home, &["inspect", "ordinary", "--json"]);
    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["synthetic"], true);
    assert_eq!(report["usage"]["total_input"], 10000);
    let output = cli(&home, &["inspect", "ordinary"]);
    assert!(String::from_utf8(output.stdout)
        .unwrap()
        .contains("Synthetic session"));
    let bad = home.join("bad.jsonl");
    let mut data = std::fs::read_to_string(fixture).unwrap();
    data.push_str("{bad\n");
    std::fs::write(&bad, data).unwrap();
    assert!(!cli(
        &home,
        &[
            "import",
            "--agent",
            "fixture",
            "--path",
            bad.to_str().unwrap()
        ]
    )
    .status
    .success());
    let output = cli(&home, &["inspect", "ordinary", "--json"]);
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["event_count"], 501);
    assert!(!cli(&home, &["inspect", "absent", "--json"])
        .status
        .success());
    std::fs::remove_dir_all(home).unwrap();
}
