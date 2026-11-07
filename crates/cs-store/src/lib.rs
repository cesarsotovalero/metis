#![forbid(unsafe_code)]
use cs_core::{EventEnvelope, Measurement, Payload, Report, SessionStatus, Usage};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
mod reconcile;
use reconcile::{CallState, Task};
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database failure: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("invalid event data")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Contract(&'static str),
}
pub struct Store {
    conn: Connection,
}
const MIGRATION: &str = include_str!("../migrations/001.sql");
impl Store {
    pub fn open(path: &Path) -> Result<Self, Error> {
        Self::open_with_timeout(path, std::time::Duration::from_secs(5))
    }
    /// Bound SQLite lock waits for request-driven writers.
    pub fn open_with_timeout(path: &Path, timeout: std::time::Duration) -> Result<Self, Error> {
        let mut conn = Connection::open(path)?;
        conn.busy_timeout(timeout)?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > 3 {
            return Err(Error::Contract("unsupported newer database schema"));
        }
        conn.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;",
        )?;
        // Schema changes, integrity records, and view backfills commit together.
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        for (number, sql) in [
            (1, MIGRATION),
            (2, include_str!("../migrations/002.sql")),
            (3, include_str!("../migrations/003.sql")),
        ] {
            if version < number {
                let body = sql
                    .trim()
                    .strip_prefix("BEGIN IMMEDIATE;")
                    .and_then(|sql| sql.trim().strip_suffix("COMMIT;"))
                    .ok_or(Error::Contract("invalid migration transaction boundary"))?;
                tx.execute_batch(body)?;
            }
            let recorded: String = tx.query_row(
                "SELECT sql_text FROM migrations WHERE version=?1",
                [number],
                |r| r.get(0),
            )?;
            if version < number && recorded.is_empty() {
                tx.execute(
                    "UPDATE migrations SET sql_text=?1 WHERE version=?2",
                    params![sql, number],
                )?;
            } else if recorded != sql {
                return Err(Error::Contract("migration integrity mismatch"));
            }
        }
        if version < 3 {
            // Backfill existing ledgers from immutable source events after the additive migration.
            let mut stmt = tx.prepare("SELECT id FROM sessions")?;
            let ids = stmt
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            for id in ids {
                Self::inspect_connection(&tx, &id, true)?;
            }
        }
        tx.commit()?;
        Ok(Self { conn })
    }
    /// A single synchronous owner is the writer for this offline slice.
    /// Entire files commit atomically; errors never retain a partial import.
    pub fn import(&mut self, events: &[EventEnvelope]) -> Result<usize, Error> {
        // Acquire the write reservation before reading duplicate identities.
        // Upgrading a deferred read transaction can bypass SQLite's busy wait.
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let mut inserted = 0;
        for e in events {
            e.validate().map_err(Error::Contract)?;
            let source_sequence = e
                .producer_sequence
                .map(i64::try_from)
                .transpose()
                .map_err(|_| Error::Contract("producer sequence exceeds SQLite integer range"))?;
            let json = serde_json::to_string(e)?;
            let old:Option<String>=tx.query_row("SELECT payload_json FROM events WHERE event_id=?1 OR (producer_id=?2 AND epoch=?3 AND source_seq=?4)",params![e.event_id,e.producer_id,e.producer_epoch,source_sequence],|r|r.get(0)).optional()?;
            if let Some(old) = old {
                let mut previous: EventEnvelope = serde_json::from_str(&old)?;
                previous.event_id.clone_from(&e.event_id);
                if previous != *e {
                    return Err(Error::Contract("conflicting duplicate event identity"));
                }
                continue;
            }
            tx.execute(
                "INSERT INTO sessions(id) VALUES(?1) ON CONFLICT DO NOTHING",
                [&e.session_id],
            )?;
            tx.execute("INSERT INTO events(event_id,session_id,producer_id,epoch,source_seq,observed_at,payload_json) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![e.event_id,e.session_id,e.producer_id,e.producer_epoch,source_sequence,e.observed_at_unix_ms,json])?;
            tx.execute("INSERT INTO event_metadata(event_id,kind,visibility,source_timestamp) VALUES(?1,?2,'synthetic',?3)", params![e.event_id,e.payload.kind(),e.source_timestamp_unix_ms])?;
            inserted += 1;
        }
        // Validate the complete reconciled session, including previously imported deltas.
        for id in events
            .iter()
            .map(|e| &e.session_id)
            .collect::<std::collections::BTreeSet<_>>()
        {
            Self::inspect_connection(&tx, id, true)?;
        }
        tx.commit()?;
        // Reports fold immutable events rather than trusting arrival-order views.
        Ok(inserted)
    }
    pub fn inspect(&self, id: &str) -> Result<Report, Error> {
        Self::inspect_connection(&self.conn, id, false)
    }
    fn inspect_connection(conn: &Connection, id: &str, materialize: bool) -> Result<Report, Error> {
        if conn
            .query_row("SELECT id FROM sessions WHERE id=?1", [id], |r| {
                r.get::<_, String>(0)
            })
            .optional()?
            .is_none()
        {
            return Err(Error::Contract("session not found"));
        }
        let mut stmt=conn.prepare("SELECT payload_json FROM events WHERE session_id=?1 ORDER BY producer_id,epoch,source_seq IS NULL,source_seq,collector_seq")?;
        let rows = stmt.query_map([id], |r| r.get::<_, String>(0))?;
        use std::collections::BTreeMap;
        let mut requests: BTreeMap<String, (CallState, Usage, bool)> = BTreeMap::new();
        let mut tools: BTreeMap<String, CallState> = BTreeMap::new();
        let mut owners = BTreeMap::new();
        let mut count = 0;
        let mut gaps = 0;
        let mut dropped_events = 0_u64;
        let mut exit_codes = BTreeMap::new();
        let mut lifecycle = Vec::new();
        let mut tasks: BTreeMap<String, Task> = BTreeMap::new();
        let mut call_tasks = BTreeMap::new();
        let mut epochs = BTreeMap::new();
        let mut event_counts = BTreeMap::new();
        for row in rows {
            let e: EventEnvelope = serde_json::from_str(&row?)?;
            count += 1;
            *event_counts
                .entry(e.payload.kind().to_string())
                .or_insert(0) += 1;
            let epoch = (e.producer_id.clone(), e.producer_epoch.clone());
            let previous = if let Payload::SessionResumed { previous_epoch } = &e.payload {
                Some(previous_epoch.clone())
            } else {
                None
            };
            let entry = epochs.entry(epoch).or_insert(None);
            if previous.is_some() {
                *entry = previous;
            }
            if matches!(
                e.payload,
                Payload::SessionStarted { .. }
                    | Payload::SessionResumed { .. }
                    | Payload::SessionEnded { .. }
            ) {
                lifecycle.push(e.clone());
            }
            if let Some(task_id) = &e.task_id {
                tasks.entry(task_id.clone()).or_default();
            }
            if let Payload::TaskStarted { task_id } | Payload::TaskOutcomeRecorded { task_id, .. } =
                &e.payload
            {
                tasks.entry(task_id.clone()).or_default().observe(&e);
            }
            let identity = match &e.payload {
                Payload::RequestStarted { request_id }
                | Payload::RequestCompleted { request_id }
                | Payload::RequestFailed { request_id }
                | Payload::RequestStreaming { request_id }
                | Payload::RequestCancelled { request_id }
                | Payload::UsageObserved { request_id, .. } => Some(("request", request_id)),
                Payload::ToolStarted { tool_id }
                | Payload::ToolCompleted { tool_id, .. }
                | Payload::ToolFailed { tool_id }
                | Payload::ToolCancelled { tool_id } => Some(("tool", tool_id)),
                _ => None,
            };
            if let Some((kind, key)) = identity {
                if let Some(task_id) = &e.task_id {
                    if call_tasks
                        .insert((kind, key.clone()), task_id.clone())
                        .is_some_and(|old| old != *task_id)
                    {
                        return Err(Error::Contract("conflicting call task identity"));
                    }
                }
                let owner = (e.producer_id, e.producer_epoch);
                if owners
                    .insert((kind, key.clone()), owner.clone())
                    .is_some_and(|old| old != owner)
                {
                    return Err(Error::Contract(
                        "cross-producer/epoch call reconciliation unsupported",
                    ));
                }
            }
            match e.payload {
                Payload::RequestStarted { request_id } => {
                    requests.entry(request_id).or_default();
                }
                Payload::RequestCompleted { request_id } => {
                    requests
                        .entry(request_id)
                        .or_default()
                        .0
                        .observe(CallState::Completed);
                }
                Payload::RequestFailed { request_id } => {
                    requests
                        .entry(request_id)
                        .or_default()
                        .0
                        .observe(CallState::Failed);
                }
                Payload::RequestStreaming { request_id } => {
                    requests
                        .entry(request_id)
                        .or_default()
                        .0
                        .observe(CallState::Streaming);
                }
                Payload::RequestCancelled { request_id } => {
                    requests
                        .entry(request_id)
                        .or_default()
                        .0
                        .observe(CallState::Cancelled);
                }
                Payload::UsageObserved {
                    request_id,
                    mode,
                    usage,
                } => {
                    let r = requests.entry(request_id).or_default();
                    cs_accounting::apply(&mut r.1, usage, mode).map_err(Error::Contract)?;
                    r.2 = true;
                }
                Payload::ToolStarted { tool_id } => {
                    tools.entry(tool_id).or_default();
                }
                Payload::ToolCompleted { tool_id, exit_code } => {
                    exit_codes
                        .entry(tool_id.clone())
                        .and_modify(|old| {
                            if *old != Some(exit_code) {
                                *old = None;
                            }
                        })
                        .or_insert(Some(exit_code));
                    tools
                        .entry(tool_id)
                        .or_default()
                        .observe(if exit_code == 0 {
                            CallState::Completed
                        } else {
                            CallState::Failed
                        });
                }
                Payload::ToolFailed { tool_id } => {
                    tools.entry(tool_id).or_default().observe(CallState::Failed);
                }
                Payload::ToolCancelled { tool_id } => {
                    tools
                        .entry(tool_id)
                        .or_default()
                        .observe(CallState::Cancelled);
                }
                Payload::VisibilityGap { .. } => gaps += 1,
                Payload::QueueOverflow {
                    dropped_events: count,
                } => {
                    gaps += 1;
                    dropped_events = dropped_events
                        .checked_add(count)
                        .ok_or(Error::Contract("dropped event count overflow"))?;
                }
                _ => {}
            }
        }
        let (session_status, started_at, ended_at) = reconcile::session(&lifecycle);
        let tasks = tasks
            .into_iter()
            .map(|(id, task)| task.report(id))
            .collect::<Vec<_>>();
        let usage =
            cs_accounting::aggregate(&requests.values().map(|r| r.1.clone()).collect::<Vec<_>>())
                .map_err(Error::Contract)?;
        if materialize {
            let session_state = match session_status {
                SessionStatus::Ended => "ended",
                SessionStatus::Incomplete => "incomplete",
                SessionStatus::Unknown => "unknown",
            };
            conn.execute(
                "UPDATE sessions SET status=?1,started_at=?2,ended_at=?3 WHERE id=?4",
                params![session_state, started_at, ended_at, id],
            )?;
            conn.execute("DELETE FROM requests WHERE session_id=?1", [id])?;
            conn.execute("DELETE FROM tool_calls WHERE session_id=?1", [id])?;
            conn.execute("DELETE FROM tasks WHERE session_id=?1", [id])?;
            conn.execute("DELETE FROM session_epochs WHERE session_id=?1", [id])?;
            for task in &tasks {
                let evidence_kind = task.evidence_kind.map(|kind| {
                    serde_json::to_value(kind)
                        .expect("enum serializes")
                        .as_str()
                        .unwrap()
                        .to_string()
                });
                conn.execute("INSERT INTO tasks(session_id,id,outcome_json,oracle_version,evidence_kind,started_at,ended_at) VALUES(?1,?2,?3,?4,?5,?6,?7)", params![id,task.task_id,serde_json::to_string(&task.outcome)?,task.oracle_version,evidence_kind,task.started_at_unix_ms,task.ended_at_unix_ms])?;
            }
            for ((producer, epoch), previous) in epochs {
                conn.execute("INSERT INTO session_epochs(session_id,producer_id,epoch,previous_epoch) VALUES(?1,?2,?3,?4)", params![id,producer,epoch,previous])?;
            }
            for (key, (state, usage, _)) in &requests {
                conn.execute("INSERT INTO requests(id,session_id,status,normalized_usage_json,task_id) VALUES(?1,?2,?3,?4,?5)",params![key,id,state.as_str(),serde_json::to_string(usage)?,call_tasks.get(&("request",key.clone()))])?;
            }
            for (key, state) in &tools {
                conn.execute(
                    "INSERT INTO tool_calls(id,session_id,status,task_id,exit_code) VALUES(?1,?2,?3,?4,?5)",
                    params![
                        key,
                        id,
                        state.as_str(),
                        call_tasks.get(&("tool", key.clone())),
                        exit_codes.get(key).copied().flatten().filter(|_| *state != CallState::Unknown)
                    ],
                )?;
            }
        }
        Ok(Report {
            session_id: id.into(), session_status,
            started_at_unix_ms: started_at, ended_at_unix_ms: ended_at,
            synthetic: true, capture_mode: "metadata".into(), event_count: count,
            request_count: requests.len() as u64, tool_count: tools.len() as u64,
            incomplete_requests: requests.values().filter(|r| r.0.incomplete()).count() as u64,
            incomplete_tools: tools.values().filter(|s| s.incomplete()).count() as u64,
            failed_requests: requests.values().filter(|r| r.0 == CallState::Failed).count() as u64,
            failed_tools: tools.values().filter(|s| **s == CallState::Failed).count() as u64,
            cancelled_requests: requests.values().filter(|r| r.0 == CallState::Cancelled).count() as u64,
            cancelled_tools: tools.values().filter(|s| **s == CallState::Cancelled).count() as u64,
            streaming_requests: requests.values().filter(|r| r.0 == CallState::Streaming).count() as u64,
            ambiguous_requests: requests.values().filter(|r| r.0 == CallState::Unknown).count() as u64,
            ambiguous_tools: tools.values().filter(|s| **s == CallState::Unknown).count() as u64,
            tasks, event_counts, visibility_gaps: gaps, dropped_events,
            requests_with_usage: requests.values().filter(|r| r.2).count() as u64,
            usage, cost: Measurement::Unavailable { reason: "no verified price snapshot".into() },
            provenance: "reported by synthetic fixture; canonical usage schema v1; no live provider verification".into(),
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(name: &str) -> Vec<EventEnvelope> {
        std::fs::read_to_string(format!("../../fixtures/{name}.jsonl"))
            .unwrap()
            .lines()
            .map(|s| serde_json::from_str(s).unwrap())
            .collect()
    }
    #[test]
    fn dedup_and_100_turn_totals() {
        let mut s = Store::open(Path::new(":memory:")).unwrap();
        let events = fixture("ordinary");
        assert_eq!(s.import(&events).unwrap(), 501);
        assert_eq!(s.import(&events).unwrap(), 0);
        let r = s.inspect("ordinary").unwrap();
        assert_eq!(r.request_count, 100);
        assert_eq!(r.usage.total_input, Some(10000));
        assert_eq!(r.usage.total_output, Some(2000));
        assert_eq!(r.incomplete_requests, 0);
    }
    #[test]
    fn cumulative_and_interruption() {
        let mut s = Store::open(Path::new(":memory:")).unwrap();
        s.import(&fixture("cached")).unwrap();
        assert_eq!(s.inspect("cached").unwrap().usage.total_input, Some(150));
        s.import(&fixture("interrupted")).unwrap();
        let r = s.inspect("interrupted").unwrap();
        assert_eq!(r.incomplete_requests, 1);
        assert_eq!(r.incomplete_tools, 1);
        assert_eq!(r.usage.total_input, None);
    }
    #[test]
    fn malformed_rollback_and_conflict() {
        let mut s = Store::open(Path::new(":memory:")).unwrap();
        let mut e = fixture("cached");
        e.last_mut().unwrap().schema_version = 99;
        assert!(s.import(&e).is_err());
        assert!(s.inspect("cached").is_err());
        let e = fixture("cached");
        s.import(&e).unwrap();
        let mut conflict = e[0].clone();
        conflict.observed_at_unix_ms += 1;
        assert!(s.import(&[conflict]).is_err());
        assert!(serde_json::from_str::<EventEnvelope>("{}").is_err());
    }
    #[test]
    fn migration_integrity_immutability_and_materialized_records() {
        let mut s = Store::open(Path::new(":memory:")).unwrap();
        s.import(&fixture("cached")).unwrap();
        assert!(s
            .conn
            .execute("UPDATE events SET observed_at=0", [])
            .is_err());
        assert!(s.conn.execute("DELETE FROM events", []).is_err());
        let count: i64 = s
            .conn
            .query_row(
                "SELECT count(*) FROM requests WHERE status='completed'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
        assert!(s
            .conn
            .execute(
                "INSERT INTO tool_calls(id,session_id,status) VALUES('a','missing','incomplete')",
                []
            )
            .is_err());
        let sql: String = s
            .conn
            .query_row("SELECT sql_text FROM migrations WHERE version=1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(sql, MIGRATION);
    }
    #[test]
    fn late_events_reconcile_and_source_duplicates_dedup() {
        let mut s = Store::open(Path::new(":memory:")).unwrap();
        let events = fixture("cached");
        s.import(&events[4..]).unwrap();
        s.import(&events[..4]).unwrap();
        let mut duplicate = events[1].clone();
        duplicate.event_id = "other-local-id".into();
        assert_eq!(s.import(&[duplicate]).unwrap(), 0);
        let r = s.inspect("cached").unwrap();
        assert_eq!(r.incomplete_requests, 0);
        assert_eq!(r.usage.total_input, Some(150));
        let mut other = events[1].clone();
        other.event_id = "second-producer".into();
        other.producer_id = "other".into();
        assert!(s.import(&[other]).is_err());
    }
    #[test]
    fn lifecycle_late_events_missing_markers_and_ambiguity() {
        let mut s = Store::open(Path::new(":memory:")).unwrap();
        let events = fixture("lifecycle");
        // End-only imports remain observable without inventing a start time.
        s.import(&events[1..]).unwrap();
        let r = s.inspect("lifecycle").unwrap();
        assert_eq!(r.session_status, SessionStatus::Ended);
        assert_eq!(r.started_at_unix_ms, None);
        s.import(&events[..1]).unwrap();
        assert_eq!(s.import(&events).unwrap(), 0);
        let r = s.inspect("lifecycle").unwrap();
        assert_eq!(r.session_status, SessionStatus::Ended);
        assert_eq!(r.started_at_unix_ms, Some(events[0].observed_at_unix_ms));
        assert_eq!(r.ended_at_unix_ms, Some(events[1].observed_at_unix_ms));
        let stored: (String, i64, i64) = s
            .conn
            .query_row(
                "SELECT status,started_at,ended_at FROM sessions WHERE id='lifecycle'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            stored,
            (
                "ended".into(),
                events[0].observed_at_unix_ms,
                events[1].observed_at_unix_ms
            )
        );
        s.import(&fixture("ordinary")).unwrap();
        assert_eq!(
            s.inspect("ordinary").unwrap().session_status,
            SessionStatus::Incomplete
        );
        // Wall-clock regressions do not override authoritative producer sequence.
        let mut regressed = events.clone();
        regressed[1].observed_at_unix_ms = 0;
        let mut other = Store::open(Path::new(":memory:")).unwrap();
        other.import(&regressed).unwrap();
        assert_eq!(
            other.inspect("lifecycle").unwrap().session_status,
            SessionStatus::Ended
        );
        // Cross-stream end markers are ambiguous even if timestamps look ordered.
        let mut end = events[1].clone();
        end.event_id = "other-end".into();
        end.producer_epoch = "epoch-2".into();
        s.import(&[end]).unwrap();
        let r = s.inspect("lifecycle").unwrap();
        assert_eq!(r.session_status, SessionStatus::Unknown);
        assert_eq!(r.started_at_unix_ms, None);
        assert_eq!(r.ended_at_unix_ms, None);
        // Without producer sequence, arrival order cannot resolve start vs end.
        let mut unsequenced = events;
        for e in &mut unsequenced {
            e.producer_sequence = None;
        }
        let mut other = Store::open(Path::new(":memory:")).unwrap();
        other.import(&unsequenced).unwrap();
        assert_eq!(
            other.inspect("lifecycle").unwrap().session_status,
            SessionStatus::Unknown
        );
    }
    #[test]
    fn conflicting_lifecycle_markers_do_not_claim_an_end() {
        let events = fixture("lifecycle");
        let mut reversed = events.clone();
        reversed[0].producer_sequence = Some(2);
        let mut repeated = events.clone();
        let mut extra = events[0].clone();
        extra.event_id = "restart".into();
        extra.producer_sequence = Some(2);
        repeated.push(extra);
        for trace in [reversed, repeated] {
            let mut s = Store::open(Path::new(":memory:")).unwrap();
            s.import(&trace).unwrap();
            let report = s.inspect("lifecycle").unwrap();
            assert_eq!(report.session_status, SessionStatus::Unknown);
            assert_eq!(report.ended_at_unix_ms, None);
        }
        let mut s = Store::open(Path::new(":memory:")).unwrap();
        // A call without lifecycle events still creates a session, with no invented times.
        s.import(&fixture("ordinary")[1..2]).unwrap();
        let report = s.inspect("ordinary").unwrap();
        assert_eq!(report.session_status, SessionStatus::Incomplete);
        assert_eq!(report.started_at_unix_ms, None);
        assert_eq!(report.ended_at_unix_ms, None);
    }
    #[test]
    fn additive_lifecycle_migration_preserves_v1_events() {
        let p = std::env::temp_dir().join(format!("mertis-migrate-{}.sqlite", std::process::id()));
        let events = fixture("lifecycle");
        {
            let conn = Connection::open(&p).unwrap();
            conn.execute_batch(MIGRATION).unwrap();
            conn.execute("UPDATE migrations SET sql_text=?1", [MIGRATION])
                .unwrap();
            conn.execute("INSERT INTO sessions(id) VALUES('lifecycle')", [])
                .unwrap();
            for e in &events {
                conn.execute("INSERT INTO events(event_id,session_id,producer_id,epoch,source_seq,observed_at,payload_json) VALUES(?1,?2,?3,?4,?5,?6,?7)",
                    params![e.event_id,e.session_id,e.producer_id,e.producer_epoch,e.producer_sequence.map(|v| i64::try_from(v).unwrap()),e.observed_at_unix_ms,serde_json::to_string(e).unwrap()]).unwrap();
            }
        }
        for _ in 0..2 {
            let s = Store::open(&p).unwrap();
            let r = s.inspect("lifecycle").unwrap();
            assert_eq!(r.session_status, SessionStatus::Ended);
            assert_eq!(r.event_count, 2);
            let state: String = s
                .conn
                .query_row("SELECT status FROM sessions", [], |r| r.get(0))
                .unwrap();
            assert_eq!(state, "ended");
            assert!(s.conn.execute("DELETE FROM events", []).is_err());
        }
        std::fs::remove_file(p).unwrap();
    }
    #[test]
    fn restart_and_newer_schema() {
        let p = std::env::temp_dir().join(format!("mertis-test-{}.sqlite", std::process::id()));
        {
            let mut s = Store::open(&p).unwrap();
            s.import(&fixture("cached")).unwrap();
        }
        let s = Store::open(&p).unwrap();
        assert_eq!(s.inspect("cached").unwrap().usage.total_input, Some(150));
        s.conn.execute_batch("PRAGMA user_version=99").unwrap();
        drop(s);
        assert!(Store::open(&p).is_err());
        std::fs::remove_file(p).unwrap();
    }
}

#[cfg(test)]
mod conformance;
