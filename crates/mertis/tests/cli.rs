use std::{path::PathBuf, process::Command};
fn cli(home: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_mertis"))
        .arg("--home")
        .arg(home)
        .args(args)
        .output()
        .unwrap()
}
#[test]
fn offline_import_report_and_atomic_malformed_input() {
    let home = std::env::temp_dir().join(format!("mertis-cli-{}", std::process::id()));
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
    assert_eq!(report["session_status"], "incomplete");
    assert_eq!(report["started_at_unix_ms"], 1700000000000_i64);
    assert!(report["ended_at_unix_ms"].is_null());
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

#[test]
fn complete_event_contract_and_bounded_privacy_rejection() {
    let home = std::env::temp_dir().join(format!("mertis-contract-cli-{}", std::process::id()));
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/event-contract.jsonl");
    let result = cli(
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
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let result = cli(&home, &["inspect", "contract", "--json"]);
    assert!(result.status.success());
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["session_status"], "ended");
    assert_eq!(report["cancelled_requests"], 1);
    assert_eq!(report["cancelled_tools"], 1);
    assert_eq!(report["tasks"][0]["outcome"]["value"], "passed");
    assert_eq!(report["tasks"][0]["evidence_kind"], "observed");
    assert_eq!(report["event_counts"]["retrieval_unavailable"], 1);
    let count = report["event_count"].clone();
    let markdown = cli(&home, &["inspect", "contract"]);
    let markdown = String::from_utf8(markdown.stdout).unwrap();
    assert!(markdown.contains("Task `task-1`: Passed"));
    assert!(markdown.contains("representation_rejected"));
    let bad = home.join("bad.jsonl");
    let mut value: serde_json::Value = serde_json::from_str(
        std::fs::read_to_string(fixture)
            .unwrap()
            .lines()
            .next()
            .unwrap(),
    )
    .unwrap();
    value["payload"]["prompt"] = serde_json::json!("PRIVATE_CONTENT_MUST_NOT_LEAK");
    let private = serde_json::to_string(&value).unwrap();
    let mut invalid_utf8 = private.as_bytes().to_vec();
    invalid_utf8.push(0xff);
    for content in [private.into_bytes(), vec![b'x'; 65537], invalid_utf8] {
        std::fs::write(&bad, content).unwrap();
        let result = cli(
            &home,
            &[
                "import",
                "--agent",
                "fixture",
                "--path",
                bad.to_str().unwrap(),
            ],
        );
        assert!(!result.status.success());
        assert!(!String::from_utf8_lossy(&result.stderr).contains("PRIVATE_CONTENT"));
    }
    let result = cli(&home, &["inspect", "contract", "--json"]);
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["event_count"], count);
    // A sparse oversized fixture fails before any input is allocated.
    let file = std::fs::File::create(&bad).unwrap();
    file.set_len(32 * 1024 * 1024 + 1).unwrap();
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
    std::fs::remove_dir_all(home).unwrap();
}
