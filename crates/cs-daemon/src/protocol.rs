use crate::{Error, Result};
use cs_core::{EventEnvelope, Report};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
pub const VERSION: u16 = 1;
pub const MAX_FRAME: usize = 8 * 1024 * 1024;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub request_id: String,
    pub operation: Operation,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "op", deny_unknown_fields)]
pub enum Operation {
    #[serde(rename = "hello")]
    Hello {
        protocol_version: u16,
        client_version: String,
        token: String,
        producer_id: String,
        producer_epoch: String,
        session_id: Option<String>,
        capabilities: Vec<String>,
    },
    #[serde(rename = "event.append")]
    Append { event: Box<EventEnvelope> },
    #[serde(rename = "event.append_batch")]
    AppendBatch { events: Vec<EventEnvelope> },
    #[serde(rename = "session.snapshot")]
    Snapshot {},
    #[serde(rename = "health")]
    Health {},
    #[serde(rename = "shutdown")]
    Shutdown {},
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub request_id: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Reply>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<SafeError>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Reply {
    Hello {
        protocol_version: u16,
        capabilities: Vec<String>,
        max_frame: usize,
        deadline_ms: u64,
        max_batch_events: usize,
    },
    Appended {
        inserted: usize,
    },
    Snapshot {
        report: Box<Report>,
    },
    Health {
        protocol_version: u16,
        mode: String,
        queue_rejections: u64,
        connection_rejections: u64,
        persistence_failures: u64,
        expired_requests: u64,
        spool_failures: u64,
    },
    Shutdown {},
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SafeError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub original_must_be_preserved: bool,
}
impl SafeError {
    pub fn new(code: &str) -> Self {
        Self {
            code: code.into(),
            message: "local observer request was not acknowledged; preserve original data".into(),
            retryable: false,
            original_must_be_preserved: true,
        }
    }
}
impl Response {
    pub fn success(request_id: String, result: Reply) -> Self {
        Self {
            request_id,
            ok: true,
            result: Some(result),
            error: None,
        }
    }
    pub fn failure(request_id: String, code: &str) -> Self {
        Self {
            request_id,
            ok: false,
            result: None,
            error: Some(SafeError::new(code)),
        }
    }
}
pub(crate) fn identity(value: &str) -> bool {
    !value.is_empty() && value.len() <= 128 && !value.chars().any(char::is_control)
}
/// Size validation precedes body allocation. Socket wrappers additionally enforce
/// one absolute deadline across all reads/writes in a frame.
pub fn read_frame(reader: &mut impl Read) -> Result<Vec<u8>> {
    let mut header = [0; 4];
    reader.read_exact(&mut header)?;
    let length = u32::from_be_bytes(header) as usize;
    if length == 0 || length > MAX_FRAME {
        return Err(Error::Contract("invalid IPC frame length"));
    }
    let mut bytes = vec![0; length];
    reader.read_exact(&mut bytes)?;
    Ok(bytes)
}
pub fn write_frame(writer: &mut impl Write, value: &impl Serialize) -> Result<()> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.is_empty() || bytes.len() > MAX_FRAME {
        return Err(Error::Contract("IPC response exceeds frame limit"));
    }
    writer.write_all(&(bytes.len() as u32).to_be_bytes())?;
    writer.write_all(&bytes)?;
    Ok(())
}
