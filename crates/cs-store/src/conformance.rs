use super::*;
use cs_core::{EvidenceKind, TaskOutcome};

fn trace() -> Vec<EventEnvelope> {
    include_str!("../../../fixtures/event-contract.jsonl")
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn marker(kind: Payload, epoch: &str, sequence: u64) -> EventEnvelope {
    let mut e = trace()[0].clone();
    e.event_id = format!("{epoch}-{sequence}");
    e.producer_epoch = epoch.into();
    e.producer_sequence = Some(sequence);
    e.payload = kind;
    e
}
#[test]
fn every_event_family_persists_with_tasks_cancellations_and_provenance() {
    let mut events = trace();
    for event in &mut events {
        match &mut event.payload {
            Payload::ArtifactStored { capture_mode, .. } => {
                *capture_mode = cs_core::CaptureMode::FullLocal
            }
            Payload::PolicyActivated { mode, .. } => *mode = cs_core::PolicyMode::Conservative,
            Payload::AdapterCapabilityChanged {
                status,
                evidence_ref,
                ..
            } => {
                *status = cs_core::CapabilityStatus::Verified;
                *evidence_ref = Some("synthetic-evidence".into());
            }
            _ => {}
        }
    }
    let mut s = Store::open(Path::new(":memory:")).unwrap();
    // Reverse arrival and reimporting must not alter final state.
    let mut reversed = events.clone();
    reversed.reverse();
    s.import(&reversed).unwrap();
    assert_eq!(s.import(&events).unwrap(), 0);
    let report = s.inspect("contract").unwrap();
    assert_eq!(report.event_count, events.len() as u64);
    assert_eq!(report.event_counts.len(), events.len());
    assert_eq!(report.session_status, SessionStatus::Ended);
    assert_eq!(report.request_count, 3);
    assert_eq!(report.tool_count, 3);
    assert_eq!(report.cancelled_requests, 1);
    assert_eq!(report.cancelled_tools, 1);
    assert_eq!(report.failed_requests, 1);
    assert_eq!(report.failed_tools, 1);
    assert_eq!(report.visibility_gaps, 2);
    assert_eq!(report.dropped_events, 2);
    let exit: i32 = s
        .conn
        .query_row(
            "SELECT exit_code FROM tool_calls WHERE id='tool-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(exit, 0);
    assert_eq!(report.tasks.len(), 1);
    let task = &report.tasks[0];
    assert!(matches!(
        task.outcome,
        Measurement::Reported {
            value: TaskOutcome::Passed,
            ..
        }
    ));
    assert_eq!(task.evidence_kind, Some(EvidenceKind::Observed));
    assert_eq!(task.oracle_version.as_deref(), Some("oracle-v1"));
    let meta: i64 = s
        .conn
        .query_row("SELECT count(*) FROM event_metadata", [], |r| r.get(0))
        .unwrap();
    assert_eq!(meta, events.len() as i64);
    let epochs: i64 = s
        .conn
        .query_row("SELECT count(*) FROM session_epochs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(epochs, 2);
    let stored_outcome: String = s
        .conn
        .query_row("SELECT outcome_json FROM tasks", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Measurement<TaskOutcome>>(&stored_outcome).unwrap(),
        task.outcome
    );
    // Observations of full-local/policy/retrieval never grant storage or execution permissions.
    let capture: String = s
        .conn
        .query_row("SELECT capture_mode FROM sessions", [], |r| r.get(0))
        .unwrap();
    assert_eq!(capture, "metadata");
}
#[test]
fn late_resume_chain_and_ambiguous_lineage() {
    let start = marker(Payload::SessionStarted {}, "epoch-z", 0);
    let resume = marker(
        Payload::SessionResumed {
            previous_epoch: "epoch-z".into(),
        },
        "epoch-a",
        0,
    );
    let end = marker(Payload::SessionEnded {}, "epoch-a", 1);
    let mut s = Store::open(Path::new(":memory:")).unwrap();
    s.import(&[resume.clone(), end]).unwrap();
    assert_eq!(
        s.inspect("contract").unwrap().session_status,
        SessionStatus::Unknown
    );
    s.import(&[start]).unwrap();
    assert_eq!(
        s.inspect("contract").unwrap().session_status,
        SessionStatus::Ended
    );
    // A new resumed process invalidates the older process end until it ends itself.
    let next = marker(
        Payload::SessionResumed {
            previous_epoch: "epoch-a".into(),
        },
        "epoch-b",
        0,
    );
    s.import(&[next]).unwrap();
    let report = s.inspect("contract").unwrap();
    assert_eq!(report.session_status, SessionStatus::Incomplete);
    assert_eq!(report.ended_at_unix_ms, None);
    // Branching cannot be silently treated as a linear resume.
    let branch = marker(
        Payload::SessionResumed {
            previous_epoch: "epoch-z".into(),
        },
        "epoch-c",
        0,
    );
    s.import(&[branch]).unwrap();
    assert_eq!(
        s.inspect("contract").unwrap().session_status,
        SessionStatus::Unknown
    );
    let mut cyclic = Store::open(Path::new(":memory:")).unwrap();
    cyclic
        .import(&[
            marker(
                Payload::SessionResumed {
                    previous_epoch: "epoch-a".into(),
                },
                "epoch-z",
                0,
            ),
            resume,
        ])
        .unwrap();
    assert_eq!(
        cyclic.inspect("contract").unwrap().session_status,
        SessionStatus::Unknown
    );
}
#[test]
fn task_links_missing_outcomes_and_conflicts_are_explicit() {
    let mut s = Store::open(Path::new(":memory:")).unwrap();
    let mut request = marker(
        Payload::RequestStreaming {
            request_id: "r".into(),
        },
        "epoch-1",
        0,
    );
    request.task_id = Some("t".into());
    s.import(&[request.clone()]).unwrap();
    let r = s.inspect("contract").unwrap();
    assert_eq!(r.streaming_requests, 1);
    assert_eq!(r.incomplete_requests, 1);
    assert!(matches!(
        r.tasks[0].outcome,
        Measurement::Unavailable { .. }
    ));
    let link: String = s
        .conn
        .query_row("SELECT task_id FROM requests", [], |r| r.get(0))
        .unwrap();
    assert_eq!(link, "t");
    let mut conflict = request.clone();
    conflict.event_id = "conflict".into();
    conflict.producer_sequence = Some(1);
    conflict.task_id = Some("other-task".into());
    assert!(s.import(&[conflict]).is_err());
    assert_eq!(s.inspect("contract").unwrap().event_count, 1);
    let mut complete = request;
    complete.event_id = "done".into();
    complete.producer_sequence = Some(2);
    complete.payload = Payload::RequestCompleted {
        request_id: "r".into(),
    };
    s.import(&[complete.clone()]).unwrap();
    complete.event_id = "cancelled".into();
    complete.producer_sequence = Some(3);
    complete.payload = Payload::RequestCancelled {
        request_id: "r".into(),
    };
    s.import(&[complete]).unwrap();
    let r = s.inspect("contract").unwrap();
    assert_eq!(r.ambiguous_requests, 1);
    assert_eq!(r.cancelled_requests, 0);
    assert_eq!(r.incomplete_requests, 1);
    // A producer's success assertion remains attributed to that producer.
    let outcome = marker(
        Payload::TaskOutcomeRecorded {
            task_id: "t".into(),
            outcome: TaskOutcome::Passed,
            oracle_version: None,
            evidence_kind: EvidenceKind::AgentReported,
        },
        "epoch-1",
        4,
    );
    s.import(&[outcome]).unwrap();
    assert_eq!(
        s.inspect("contract").unwrap().tasks[0].evidence_kind,
        Some(EvidenceKind::AgentReported)
    );
    let second = marker(
        Payload::TaskOutcomeRecorded {
            task_id: "t".into(),
            outcome: TaskOutcome::Failed,
            oracle_version: None,
            evidence_kind: EvidenceKind::Observed,
        },
        "epoch-1",
        5,
    );
    s.import(&[second]).unwrap();
    assert!(matches!(
        s.inspect("contract").unwrap().tasks[0].outcome,
        Measurement::Unavailable { .. }
    ));
    assert!(s
        .conn
        .execute("UPDATE requests SET task_id='missing'", [])
        .is_err());
}
#[test]
fn schema_failure_rolls_back_new_events_and_materialized_views() {
    let mut s = Store::open(Path::new(":memory:")).unwrap();
    let mut events = trace();
    s.import(&events[..1]).unwrap();
    events.last_mut().unwrap().payload = Payload::VisibilityGap {
        code: "x".repeat(129),
    };
    assert!(s.import(&events).is_err());
    assert_eq!(s.inspect("contract").unwrap().event_count, 1);
    let count: i64 = s
        .conn
        .query_row("SELECT count(*) FROM event_metadata", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
}
#[test]
fn failed_upgrade_rolls_back_schema_integrity_and_version() {
    let path = std::env::temp_dir().join(format!(
        "mertis-failed-upgrade-{}.sqlite",
        std::process::id()
    ));
    {
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(MIGRATION).unwrap();
        conn.execute("UPDATE migrations SET sql_text=?1", [MIGRATION])
            .unwrap();
        conn.execute_batch(include_str!("../migrations/002.sql"))
            .unwrap();
        conn.execute(
            "UPDATE migrations SET sql_text=?1 WHERE version=2",
            [include_str!("../migrations/002.sql")],
        )
        .unwrap();
        conn.execute("INSERT INTO sessions(id) VALUES('bad')", [])
            .unwrap();
        // Shape-valid legacy usage that fails accounting during view backfill,
        // after all schema changes have already executed inside the transaction.
        let mut broken = marker(
            Payload::UsageObserved {
                request_id: "r".into(),
                mode: cs_core::UsageMode::Cumulative,
                usage: Usage {
                    total_input: Some(1),
                    cache_read_input: Some(2),
                    cache_write_input: Some(0),
                    ..Usage::default()
                },
            },
            "e",
            0,
        );
        broken.event_id = "bad".into();
        broken.session_id = "bad".into();
        broken.producer_id = "p".into();
        broken.validate().unwrap();
        conn.execute("INSERT INTO events(event_id,session_id,producer_id,epoch,observed_at,payload_json) VALUES('bad','bad','p','e',0,?1)", [serde_json::to_string(&broken).unwrap()]).unwrap();
    }
    assert!(Store::open(&path).is_err());
    let conn = Connection::open(&path).unwrap();
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, 2);
    let tables: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='event_metadata'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(tables, 0);
    let events: i64 = conn
        .query_row("SELECT count(*) FROM events", [], |r| r.get(0))
        .unwrap();
    assert_eq!(events, 1);
    drop(conn);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn v2_upgrade_backfills_task_links_without_changing_source_events() {
    let path =
        std::env::temp_dir().join(format!("mertis-v2-upgrade-{}.sqlite", std::process::id()));
    let mut e = marker(
        Payload::RequestStarted {
            request_id: "legacy-request".into(),
        },
        "epoch-1",
        0,
    );
    e.task_id = Some("legacy-task".into());
    let raw = serde_json::to_string(&e).unwrap();
    {
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(MIGRATION).unwrap();
        conn.execute("UPDATE migrations SET sql_text=?1", [MIGRATION])
            .unwrap();
        conn.execute_batch(include_str!("../migrations/002.sql"))
            .unwrap();
        conn.execute(
            "UPDATE migrations SET sql_text=?1 WHERE version=2",
            [include_str!("../migrations/002.sql")],
        )
        .unwrap();
        conn.execute("INSERT INTO sessions(id) VALUES('contract')", [])
            .unwrap();
        conn.execute("INSERT INTO events(event_id,session_id,producer_id,epoch,observed_at,payload_json) VALUES('legacy','contract','contract','epoch-1',0,?1)", [&raw]).unwrap();
    }
    for _ in 0..2 {
        let s = Store::open(&path).unwrap();
        let report = s.inspect("contract").unwrap();
        assert_eq!(report.event_count, 1);
        assert_eq!(report.tasks[0].task_id, "legacy-task");
        let saved: String = s
            .conn
            .query_row("SELECT payload_json FROM events", [], |r| r.get(0))
            .unwrap();
        assert_eq!(saved, raw);
        let task: String = s
            .conn
            .query_row("SELECT task_id FROM requests", [], |r| r.get(0))
            .unwrap();
        assert_eq!(task, "legacy-task");
        let version: i64 = s
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 3);
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn writer_lock_contention_obeys_the_configured_busy_timeout() {
    let path =
        std::env::temp_dir().join(format!("mertis-busy-writer-{}.sqlite", std::process::id()));
    let mut store = Store::open_with_timeout(&path, std::time::Duration::from_millis(100)).unwrap();
    let competing = Connection::open(&path).unwrap();
    competing.execute_batch("BEGIN IMMEDIATE").unwrap();
    let event = marker(Payload::SessionStarted {}, "epoch-1", 0);
    let started = std::time::Instant::now();
    assert!(store.import(std::slice::from_ref(&event)).is_err());
    let elapsed = started.elapsed();
    assert!(
        elapsed >= std::time::Duration::from_millis(50),
        "writer bypassed the configured lock wait"
    );
    assert!(elapsed < std::time::Duration::from_secs(2));
    competing.execute_batch("ROLLBACK").unwrap();
    assert_eq!(store.import(&[event]).unwrap(), 1);
    drop(competing);
    drop(store);
    std::fs::remove_file(path).unwrap();
}
