#![forbid(unsafe_code)]
use serde::{Deserialize, Serialize};
pub const EVENT_SCHEMA: &str = include_str!("../../../schemas/event-v1.json");
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "provenance", rename_all = "snake_case", deny_unknown_fields)]
pub enum Measurement<T> {
    Reported {
        value: T,
        source: String,
    },
    Estimated {
        value: T,
        method: String,
        version: String,
    },
    Unavailable {
        reason: String,
    },
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Usage {
    pub uncached_input: Option<u64>,
    pub cache_read_input: Option<u64>,
    pub cache_write_input: Option<u64>,
    pub total_input: Option<u64>,
    pub visible_output: Option<u64>,
    pub reasoning_output: Option<u64>,
    pub total_output: Option<u64>,
    pub total_tokens: Option<u64>,
    pub reasoning_included_in_output: Option<bool>,
}
mod events;
pub use events::*;
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum UsageMode {
    Cumulative,
    Delta,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EventEnvelope {
    pub schema_version: u16,
    pub event_id: String,
    pub session_id: String,
    pub producer_id: String,
    pub producer_epoch: String,
    pub producer_sequence: Option<u64>,
    pub observed_at_unix_ms: i64,
    pub source_timestamp_unix_ms: Option<i64>,
    pub task_id: Option<String>,
    pub turn_id: Option<String>,
    pub correlation_id: Option<String>,
    pub visibility: Visibility,
    pub payload: Payload,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    Synthetic,
}
impl EventEnvelope {
    pub fn validate(&self) -> Result<(), &'static str> {
        static VALIDATOR: std::sync::OnceLock<jsonschema::Validator> = std::sync::OnceLock::new();
        let validator = VALIDATOR.get_or_init(|| {
            let schema: serde_json::Value =
                serde_json::from_str(EVENT_SCHEMA).expect("embedded event schema is JSON");
            jsonschema::validator_for(&schema).expect("embedded event schema is valid")
        });
        let value = serde_json::to_value(self).map_err(|_| "invalid event data")?;
        if !validator.is_valid(&value) {
            return Err("event violates schema-v1 metadata contract");
        }
        match &self.payload {
            Payload::TaskStarted { task_id } | Payload::TaskOutcomeRecorded { task_id, .. }
                if self.task_id.as_ref().is_some_and(|id| id != task_id) =>
            {
                return Err("conflicting task identity")
            }
            Payload::SessionResumed { previous_epoch }
                if *previous_epoch == self.producer_epoch =>
            {
                return Err("session resume must reference a different epoch")
            }
            Payload::AdapterCapabilityChanged {
                status: CapabilityStatus::Verified,
                evidence_ref: None,
                ..
            } => return Err("verified capability requires evidence reference"),
            _ => {}
        }
        Ok(())
    }
}
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    /// An explicit end marker was observed; task success is unknown.
    Ended,
    /// Import EOF is not a session end.
    Incomplete,
    /// Lifecycle markers cannot be reconciled across streams or restarts.
    Unknown,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskReport {
    pub task_id: String,
    pub outcome: Measurement<TaskOutcome>,
    pub oracle_version: Option<String>,
    pub evidence_kind: Option<EvidenceKind>,
    pub started_at_unix_ms: Option<i64>,
    pub ended_at_unix_ms: Option<i64>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Report {
    pub session_id: String,
    pub session_status: SessionStatus,
    /// Collector wall-clock observations, not durations. None means unavailable.
    pub started_at_unix_ms: Option<i64>,
    pub ended_at_unix_ms: Option<i64>,
    pub synthetic: bool,
    pub capture_mode: String,
    pub event_count: u64,
    pub request_count: u64,
    pub tool_count: u64,
    pub incomplete_requests: u64,
    pub incomplete_tools: u64,
    pub failed_requests: u64,
    pub failed_tools: u64,
    pub cancelled_requests: u64,
    pub cancelled_tools: u64,
    pub streaming_requests: u64,
    pub ambiguous_requests: u64,
    pub ambiguous_tools: u64,
    pub tasks: Vec<TaskReport>,
    pub event_counts: std::collections::BTreeMap<String, u64>,
    pub visibility_gaps: u64,
    pub dropped_events: u64,
    pub usage: Usage,
    pub requests_with_usage: u64,
    pub cost: Measurement<u64>,
    pub provenance: String,
}
impl Report {
    pub fn markdown(&self) -> String {
        let show = |x: Option<u64>| {
            x.map(|v| v.to_string())
                .unwrap_or_else(|| "unavailable (partial or missing coverage)".into())
        };
        let state = match self.session_status {
            SessionStatus::Ended => "ended (explicit marker; task success unknown)",
            SessionStatus::Incomplete => "incomplete (no end marker)",
            SessionStatus::Unknown => "unknown (ambiguous lifecycle markers)",
        };
        let timestamp = |value: Option<i64>| {
            value
                .map(|v| v.to_string())
                .unwrap_or_else(|| "unavailable".into())
        };
        let task_summary = self
            .tasks
            .iter()
            .map(|task| {
                let outcome = match &task.outcome {
                    Measurement::Reported { value, .. } => format!("{value:?}"),
                    _ => "unavailable".into(),
                };
                let evidence = match task.evidence_kind {
                    Some(EvidenceKind::Observed) => "observed",
                    Some(EvidenceKind::AgentReported) => "agent-reported",
                    Some(EvidenceKind::Inferred) => "inferred",
                    None => "unavailable",
                };
                format!(
                    "- Task `{}`: {}; evidence: {}; oracle: {}\n",
                    task.task_id,
                    outcome,
                    evidence,
                    task.oracle_version.as_deref().unwrap_or("unavailable")
                )
            })
            .collect::<String>();
        let task_summary = format!("Cancelled requests/tools: {}/{}; streaming requests: {}; ambiguous requests/tools: {}/{}; reported dropped events: {}.\n\nTasks: {}\n{}", self.cancelled_requests, self.cancelled_tools, self.streaming_requests, self.ambiguous_requests, self.ambiguous_tools, self.dropped_events, self.tasks.len(), task_summary);
        let event_summary = self
            .event_counts
            .iter()
            .map(|(kind, count)| format!("| {kind} | {count} |\n"))
            .collect::<String>();
        let event_summary =
            format!("\n| Observed event kind | Count |\n|---|---|\n{event_summary}");
        let lifecycle = format!("Session: {state}.\nObserved start/end (Unix ms): {}/{}. Missing timestamps are unavailable; wall-clock observations are not monotonic durations.\n", timestamp(self.started_at_unix_ms), timestamp(self.ended_at_unix_ms));
        format!("# Synthetic session `{}`\n\nCapture: metadata; mode: observe.\n{lifecycle}\nEvents: {}; requests: {}; tools: {}.\nIncomplete requests/tools: {}/{}; failed requests/tools: {}/{}; visibility gaps: {}.\n{task_summary}\n| Usage category | Reported tokens |\n|---|---|\n| Uncached input | {} |\n| Cache read input | {} |\n| Cache write input | {} |\n| Total input | {} |\n| Visible output | {} |\n| Reasoning output (subset when indicated) | {} |\n| Total output | {} |\n| Total tokens | {} |\n\nProvenance: {}. Requests with usage: {}/{}.\nCost unavailable: no verified price snapshot. No savings measured.\n{event_summary}", self.session_id,self.event_count,self.request_count,self.tool_count,self.incomplete_requests,self.incomplete_tools,self.failed_requests,self.failed_tools,self.visibility_gaps,show(self.usage.uncached_input),show(self.usage.cache_read_input),show(self.usage.cache_write_input),show(self.usage.total_input),show(self.usage.visible_output),show(self.usage.reasoning_output),show(self.usage.total_output),show(self.usage.total_tokens),self.provenance,self.requests_with_usage,self.request_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wire_round_trip_and_strict_privacy_boundary() {
        let raw = include_str!("../../../fixtures/cached.jsonl")
            .lines()
            .next()
            .unwrap();
        let event: EventEnvelope = serde_json::from_str(raw).unwrap();
        let encoded = serde_json::to_string(&event).unwrap();
        assert_eq!(event, serde_json::from_str(&encoded).unwrap());
        let mut v: serde_json::Value = serde_json::from_str(raw).unwrap();
        v["prompt"] = serde_json::json!("must not be stored");
        assert!(serde_json::from_value::<EventEnvelope>(v).is_err());
        let mut v: serde_json::Value = serde_json::from_str(raw).unwrap();
        v["payload"]["kind"] = serde_json::json!("future_kind");
        assert!(serde_json::from_value::<EventEnvelope>(v).is_err());
    }
}

#[cfg(test)]
mod conformance;
