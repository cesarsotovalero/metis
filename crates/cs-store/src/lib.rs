#![forbid(unsafe_code)]
use cs_core::{EventEnvelope, Measurement, Payload, Report, Usage};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
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
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;",
        )?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > 1 {
            return Err(Error::Contract("unsupported newer database schema"));
        }
        if version == 0 {
            conn.execute_batch(MIGRATION)?;
        }
        let recorded: String =
            conn.query_row("SELECT sql_text FROM migrations WHERE version=1", [], |r| {
                r.get(0)
            })?;
        // Store the full migration as a collision-free integrity record.
        if recorded.is_empty() {
            conn.execute(
                "UPDATE migrations SET sql_text=?1 WHERE version=1",
                [MIGRATION],
            )?;
        } else if recorded != MIGRATION {
            return Err(Error::Contract("migration integrity mismatch"));
        }
        Ok(Self { conn })
    }
    /// A single synchronous owner is the writer for this offline slice.
    /// Entire files commit atomically; errors never retain a partial import.
    pub fn import(&mut self, events: &[EventEnvelope]) -> Result<usize, Error> {
        let tx = self.conn.transaction()?;
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
        let mut requests: BTreeMap<String, (u8, Usage, bool)> = BTreeMap::new();
        let mut tools: BTreeMap<String, u8> = BTreeMap::new();
        let mut owners = BTreeMap::new();
        let mut count = 0;
        let mut gaps = 0;
        for row in rows {
            let e: EventEnvelope = serde_json::from_str(&row?)?;
            count += 1;
            let identity = match &e.payload {
                Payload::RequestStarted { request_id }
                | Payload::RequestCompleted { request_id }
                | Payload::RequestFailed { request_id }
                | Payload::UsageObserved { request_id, .. } => Some(("request", request_id)),
                Payload::ToolStarted { tool_id }
                | Payload::ToolCompleted { tool_id, .. }
                | Payload::ToolFailed { tool_id } => Some(("tool", tool_id)),
                _ => None,
            };
            if let Some((kind, key)) = identity {
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
                    requests.entry(request_id).or_default().0 = 1;
                }
                Payload::RequestFailed { request_id } => {
                    requests.entry(request_id).or_default().0 = 2;
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
                    tools.insert(tool_id, if exit_code == 0 { 1 } else { 2 });
                }
                Payload::ToolFailed { tool_id } => {
                    tools.insert(tool_id, 2);
                }
                Payload::VisibilityGap { .. } => gaps += 1,
                _ => {}
            }
        }
        let usage =
            cs_accounting::aggregate(&requests.values().map(|r| r.1.clone()).collect::<Vec<_>>())
                .map_err(Error::Contract)?;
        if materialize {
            conn.execute("DELETE FROM requests WHERE session_id=?1", [id])?;
            conn.execute("DELETE FROM tool_calls WHERE session_id=?1", [id])?;
            let status = |s| match s {
                1 => "completed",
                2 => "failed",
                _ => "incomplete",
            };
            for (key, (state, usage, _)) in &requests {
                conn.execute("INSERT INTO requests(id,session_id,status,normalized_usage_json) VALUES(?1,?2,?3,?4)",params![key,id,status(*state),serde_json::to_string(usage)?])?;
            }
            for (key, state) in &tools {
                conn.execute(
                    "INSERT INTO tool_calls(id,session_id,status) VALUES(?1,?2,?3)",
                    params![key, id, status(*state)],
                )?;
            }
        }
        Ok(Report{session_id:id.into(),synthetic:true,capture_mode:"metadata".into(),event_count:count,request_count:requests.len() as u64,tool_count:tools.len() as u64,incomplete_requests:requests.values().filter(|r|r.0==0).count() as u64,incomplete_tools:tools.values().filter(|s|**s==0).count() as u64,failed_requests:requests.values().filter(|r|r.0==2).count() as u64,failed_tools:tools.values().filter(|s|**s==2).count() as u64,visibility_gaps:gaps,requests_with_usage:requests.values().filter(|r|r.2).count() as u64,usage,cost:Measurement::Unavailable{reason:"no verified price snapshot".into()},provenance:"reported by synthetic fixture; canonical usage schema v1; no live provider verification".into()})
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
            .query_row("SELECT sql_text FROM migrations", [], |r| r.get(0))
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
