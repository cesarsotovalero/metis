use crate::{
    protocol::{self, Operation, Reply, Request, Response},
    runtime::{self, DeadlineStream},
    Error, Result,
};
use std::{
    os::unix::net::UnixStream,
    path::Path,
    time::{Duration, Instant},
};
/// An owner-authenticated connection bound to one producer epoch and session.
/// Requests are never retried automatically, including after timeouts.
pub struct Client {
    stream: UnixStream,
    timeout: Duration,
    next_id: u64,
}
impl Client {
    pub fn connect(
        home: &Path,
        producer: &str,
        epoch: &str,
        session: Option<&str>,
    ) -> Result<Self> {
        let token = runtime::token(home)?;
        let stream = UnixStream::connect(home.join("runtime/daemon.sock"))?;
        if !runtime::owner_peer(&stream) {
            return Err(Error::Contract("daemon peer is not the current owner"));
        }
        let mut client = Self {
            stream,
            timeout: Duration::from_secs(2),
            next_id: 0,
        };
        let hello = client.request(Operation::Hello {
            protocol_version: protocol::VERSION,
            client_version: env!("CARGO_PKG_VERSION").into(),
            token,
            producer_id: producer.into(),
            producer_epoch: epoch.into(),
            session_id: session.map(str::to_string),
            capabilities: [
                "event.append",
                "event.append_batch",
                "session.snapshot",
                "health",
                "shutdown",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
        })?;
        match hello {
            Reply::Hello {
                protocol_version: 1,
                deadline_ms,
                ..
            } if deadline_ms > 0 && deadline_ms <= 30000 => {
                client.timeout = Duration::from_millis(deadline_ms);
                Ok(client)
            }
            _ => Err(Error::Contract("invalid daemon handshake response")),
        }
    }
    pub fn request(&mut self, operation: Operation) -> Result<Reply> {
        let request_id = self.next_id.to_string();
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or(Error::Contract("IPC request identity exhausted"))?;
        let deadline = Instant::now() + self.timeout;
        protocol::write_frame(
            &mut DeadlineStream {
                stream: &mut self.stream,
                deadline,
            },
            &Request {
                request_id: request_id.clone(),
                operation,
            },
        )?;
        let bytes = protocol::read_frame(&mut DeadlineStream {
            stream: &mut self.stream,
            deadline: Instant::now() + self.timeout,
        })?;
        let response: Response = serde_json::from_slice(&bytes)?;
        if response.request_id != request_id {
            return Err(Error::Contract("IPC response identity mismatch"));
        }
        match (response.ok, response.result, response.error) {
            (true, Some(reply), None) => Ok(reply),
            (false, None, Some(error)) if protocol::identity(&error.code) => {
                Err(Error::Remote { code: error.code })
            }
            _ => Err(Error::Contract("invalid IPC response envelope")),
        }
    }
}
