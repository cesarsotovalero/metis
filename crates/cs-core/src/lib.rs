#![forbid(unsafe_code)]
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "provenance", rename_all = "snake_case")]
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Payload {
    SessionStarted,
    SessionEnded,
    RequestStarted {
        request_id: String,
    },
    RequestCompleted {
        request_id: String,
    },
    RequestFailed {
        request_id: String,
    },
    UsageObserved {
        request_id: String,
        mode: UsageMode,
        usage: Usage,
    },
    ToolStarted {
        tool_id: String,
    },
    ToolCompleted {
        tool_id: String,
        exit_code: i32,
    },
    ToolFailed {
        tool_id: String,
    },
    VisibilityGap {
        code: String,
    },
}
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
        if self.schema_version != 1 {
            return Err("unsupported event schema");
        }
        if [
            &self.event_id,
            &self.session_id,
            &self.producer_id,
            &self.producer_epoch,
        ]
        .iter()
        .any(|s| s.is_empty() || s.len() > 128 || s.chars().any(char::is_control))
        {
            return Err("invalid opaque identity");
        }
        Ok(())
    }
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Report {
    pub session_id: String,
    pub synthetic: bool,
    pub capture_mode: String,
    pub event_count: u64,
    pub request_count: u64,
    pub tool_count: u64,
    pub incomplete_requests: u64,
    pub incomplete_tools: u64,
    pub failed_requests: u64,
    pub failed_tools: u64,
    pub visibility_gaps: u64,
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
        format!("# Synthetic session `{}`\n\nCapture: metadata; mode: observe.\n\nEvents: {}; requests: {}; tools: {}.\nIncomplete requests/tools: {}/{}; failed requests/tools: {}/{}; visibility gaps: {}.\n\n| Usage category | Reported tokens |\n|---|---|\n| Uncached input | {} |\n| Cache read input | {} |\n| Cache write input | {} |\n| Total input | {} |\n| Visible output | {} |\n| Reasoning output (subset when indicated) | {} |\n| Total output | {} |\n| Total tokens | {} |\n\nProvenance: {}. Requests with usage: {}/{}.\nCost unavailable: no verified price snapshot. No savings measured.\n", self.session_id,self.event_count,self.request_count,self.tool_count,self.incomplete_requests,self.incomplete_tools,self.failed_requests,self.failed_tools,self.visibility_gaps,show(self.usage.uncached_input),show(self.usage.cache_read_input),show(self.usage.cache_write_input),show(self.usage.total_input),show(self.usage.visible_output),show(self.usage.reasoning_output),show(self.usage.total_output),show(self.usage.total_tokens),self.provenance,self.requests_with_usage,self.request_count)
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
