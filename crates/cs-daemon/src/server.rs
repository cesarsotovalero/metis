use crate::{
    protocol::{self, Operation, Reply, Request, Response},
    runtime::{owner_peer, DeadlineStream, Runtime},
    writer::{self, Health, Job, Work, Writer},
    Config, Error, Result,
};
use std::{
    os::unix::net::UnixStream,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::SyncSender,
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub struct Server {
    runtime: Runtime,
    writer: Option<Writer>,
    config: Config,
    stop: Arc<AtomicBool>,
    health: Arc<Health>,
}
impl Server {
    pub fn bind(home: &Path, config: Config) -> Result<Self> {
        config.validate()?;
        let runtime = Runtime::bind(home)?;
        let health = Health::open(&runtime.spool.join("daemon-loss.jsonl"))?;
        let writer = Writer::start(home, &config, health.clone())?;
        Ok(Self {
            runtime,
            writer: Some(writer),
            config,
            stop: Arc::new(AtomicBool::new(false)),
            health,
        })
    }
    pub fn shutdown_flag(&self) -> Arc<AtomicBool> {
        self.stop.clone()
    }
    pub fn serve(mut self) -> Result<()> {
        let sender = self.writer.as_ref().expect("writer started").sender.clone();
        let mut workers: Vec<JoinHandle<()>> = Vec::new();
        let mut failure = None;
        while !self.stop.load(Ordering::Acquire) {
            let mut i = 0;
            while i < workers.len() {
                if workers[i].is_finished() {
                    if workers.swap_remove(i).join().is_err() {
                        failure =
                            Some(Error::Contract("daemon client worker stopped unexpectedly"));
                        self.stop.store(true, Ordering::Release);
                    }
                } else {
                    i += 1;
                }
            }
            match self.runtime.listener.accept() {
                Ok((stream, _)) => {
                    if workers.len() >= self.config.connections {
                        self.health
                            .connection_rejections
                            .fetch_add(1, Ordering::Relaxed);
                        continue;
                    }
                    let context = Context {
                        sender: sender.clone(),
                        stop: self.stop.clone(),
                        health: self.health.clone(),
                        config: self.config.clone(),
                        token: self.runtime.token.clone(),
                    };
                    match thread::Builder::new()
                        .name("mertis-client".into())
                        .spawn(move || context.handle(stream))
                    {
                        Ok(worker) => workers.push(worker),
                        Err(error) => {
                            failure = Some(error.into());
                            break;
                        }
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5))
                }
                Err(e) => {
                    failure = Some(e.into());
                    break;
                }
            }
        }
        self.stop.store(true, Ordering::Release);
        for worker in workers {
            if worker.join().is_err() {
                failure = Some(Error::Contract("daemon client worker stopped unexpectedly"));
            }
        }
        drop(sender);
        self.writer.take().expect("writer started").finish()?;
        self.health.finish_spool(self.config.timeout)?;
        match failure {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}
// If a bound server is dropped before serve, close the channel and join its owner.
impl Drop for Server {
    fn drop(&mut self) {
        if let Some(writer) = self.writer.take() {
            let _ = writer.finish();
        }
        let _ = self.health.finish_spool(self.config.timeout);
    }
}
struct Context {
    sender: SyncSender<Job>,
    stop: Arc<AtomicBool>,
    health: Arc<Health>,
    config: Config,
    token: String,
}
struct Binding {
    producer: String,
    epoch: String,
    session: Option<String>,
    capabilities: Vec<String>,
}
impl Context {
    fn send(&self, stream: &mut UnixStream, response: &Response) -> Result<()> {
        protocol::write_frame(
            &mut DeadlineStream {
                stream,
                deadline: Instant::now() + self.config.timeout,
            },
            response,
        )
    }
    fn handle(&self, mut stream: UnixStream) {
        if !owner_peer(&stream) {
            return;
        }
        let mut binding = None;
        while !self.stop.load(Ordering::Acquire) {
            let bytes = match protocol::read_frame(&mut DeadlineStream {
                stream: &mut stream,
                deadline: Instant::now() + self.config.timeout,
            }) {
                Ok(bytes) => bytes,
                Err(_) => {
                    let _ = self.send(
                        &mut stream,
                        &Response::failure("unavailable".into(), "invalid_frame"),
                    );
                    return;
                }
            };
            let request: Request = match serde_json::from_slice(&bytes) {
                Ok(request) => request,
                Err(_) => {
                    let _ = self.send(
                        &mut stream,
                        &Response::failure("unavailable".into(), "invalid_request"),
                    );
                    return;
                }
            };
            if !protocol::identity(&request.request_id) {
                let _ = self.send(
                    &mut stream,
                    &Response::failure("unavailable".into(), "invalid_request_id"),
                );
                return;
            }
            let response = match self.apply(request.operation, &mut binding) {
                Ok(reply) => Response::success(request.request_id, reply),
                Err(code) => Response::failure(request.request_id, code),
            };
            if self.send(&mut stream, &response).is_err() || binding.is_none() {
                return;
            }
        }
    }
    fn apply(
        &self,
        operation: Operation,
        binding: &mut Option<Binding>,
    ) -> std::result::Result<Reply, &'static str> {
        if let Operation::Hello {
            protocol_version,
            client_version,
            token,
            producer_id,
            producer_epoch,
            session_id,
            capabilities,
        } = operation
        {
            if binding.is_some() {
                return Err("already_authenticated");
            }
            if token.len() != self.token.len()
                || token
                    .bytes()
                    .zip(self.token.bytes())
                    .fold(0u8, |v, (a, b)| v | (a ^ b))
                    != 0
            {
                return Err("unauthorized");
            }
            if protocol_version != protocol::VERSION {
                return Err("unsupported_protocol");
            }
            if !protocol::identity(&client_version)
                || !protocol::identity(&producer_id)
                || !protocol::identity(&producer_epoch)
                || session_id
                    .as_ref()
                    .is_some_and(|id| !protocol::identity(id))
                || capabilities.len() > 8
                || capabilities.iter().any(|s| !protocol::identity(s))
            {
                return Err("invalid_hello");
            }
            let granted = capabilities
                .into_iter()
                .filter(|cap| match cap.as_str() {
                    "health" | "shutdown" => true,
                    "event.append" | "event.append_batch" | "session.snapshot" => {
                        session_id.is_some()
                    }
                    _ => false,
                })
                .collect::<Vec<_>>();
            *binding = Some(Binding {
                producer: producer_id,
                epoch: producer_epoch,
                session: session_id,
                capabilities: granted.clone(),
            });
            return Ok(Reply::Hello {
                protocol_version: protocol::VERSION,
                capabilities: granted,
                max_frame: protocol::MAX_FRAME,
                deadline_ms: self.config.timeout.as_millis() as u64,
                max_batch_events: self.config.batch_events,
            });
        }
        let binding = binding.as_ref().ok_or("hello_required")?;
        let capability = match operation {
            Operation::Append { .. } => "event.append",
            Operation::AppendBatch { .. } => "event.append_batch",
            Operation::Snapshot { .. } => "session.snapshot",
            Operation::Health { .. } => "health",
            Operation::Shutdown { .. } => "shutdown",
            Operation::Hello { .. } => unreachable!(),
        };
        if !binding.capabilities.iter().any(|s| s == capability) {
            return Err("capability_not_granted");
        }
        let work = match operation {
            Operation::Append { event } => Work::Append(vec![*event]),
            Operation::AppendBatch { events } => Work::Append(events),
            Operation::Snapshot {} => {
                Work::Snapshot(binding.session.clone().ok_or("session_required")?)
            }
            Operation::Health {} => return Ok(self.health.snapshot()),
            Operation::Shutdown {} => {
                self.stop.store(true, Ordering::Release);
                return Ok(Reply::Shutdown {});
            }
            Operation::Hello { .. } => unreachable!(),
        };
        if let Work::Append(events) = &work {
            if events.is_empty() || events.len() > self.config.batch_events {
                return Err("invalid_batch_size");
            }
            for event in events {
                if Some(&event.session_id) != binding.session.as_ref()
                    || event.producer_id != binding.producer
                    || event.producer_epoch != binding.epoch
                {
                    return Err("binding_mismatch");
                }
                if event.validate().is_err() {
                    return Err("invalid_event");
                }
            }
        }
        writer::dispatch(
            &self.sender,
            work,
            Instant::now() + self.config.timeout,
            &self.health,
        )
        .map_err(|e| match e.code.as_str() {
            "queue_full" => "queue_full",
            "deadline_exceeded" => "deadline_exceeded",
            "writer_unavailable" => "writer_unavailable",
            _ => "persistence_failed",
        })
    }
}
