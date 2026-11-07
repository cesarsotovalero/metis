#![cfg(target_os = "linux")]
use cs_core::EventEnvelope;
use cs_daemon::{
    protocol::{self, Operation, Reply, Request, Response},
    Client, Config, Server,
};
use std::{
    fs,
    io::{Read, Write},
    os::unix::{
        fs::{symlink, MetadataExt},
        net::UnixStream,
    },
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Barrier,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Harness {
    home: PathBuf,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<cs_daemon::Result<()>>>,
}
impl Harness {
    fn start(config: Config) -> Self {
        let home = std::env::temp_dir().join(format!(
            "mertis-ipc-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let server = Server::bind(&home, config).unwrap();
        Self {
            home,
            stop: server.shutdown_flag(),
            thread: Some(thread::spawn(move || server.serve())),
        }
    }
    fn raw(&self) -> UnixStream {
        let stream = UnixStream::connect(self.home.join("runtime/daemon.sock")).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        stream
    }
    fn client(&self) -> Client {
        Client::connect(&self.home, "ordinary", "epoch-1", Some("ordinary")).unwrap()
    }
    fn restart(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.thread.take().unwrap().join().unwrap().unwrap();
        let server = Server::bind(&self.home, Config::default()).unwrap();
        self.stop = server.shutdown_flag();
        self.thread = Some(thread::spawn(move || server.serve()));
    }
}
impl Drop for Harness {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.thread.take() {
            let _ = worker.join();
        }
        let _ = fs::remove_dir_all(&self.home);
    }
}
fn events() -> Vec<EventEnvelope> {
    include_str!("../../../fixtures/ordinary.jsonl")
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn hello(h: &Harness) -> Operation {
    Operation::Hello {
        protocol_version: 1,
        client_version: "test-1".into(),
        token: fs::read_to_string(h.home.join("runtime/daemon.token")).unwrap(),
        producer_id: "ordinary".into(),
        producer_epoch: "epoch-1".into(),
        session_id: Some("ordinary".into()),
        capabilities: vec![
            "health".into(),
            "event.append_batch".into(),
            "transform.evaluate".into(),
        ],
    }
}
fn exchange(stream: &mut UnixStream, operation: Operation) -> Response {
    protocol::write_frame(
        stream,
        &Request {
            request_id: "test-request".into(),
            operation,
        },
    )
    .unwrap();
    serde_json::from_slice(&protocol::read_frame(stream).unwrap()).unwrap()
}
fn code(response: Response, expected: &str) {
    assert!(!response.ok);
    assert!(response.result.is_none());
    let error = response.error.unwrap();
    assert_eq!(error.code, expected);
    assert!(!error.retryable);
    assert!(error.original_must_be_preserved);
}
#[test]
fn handshake_rejects_missing_bad_credentials_and_unknown_versions() {
    let h = Harness::start(Config::default());
    code(
        exchange(&mut h.raw(), Operation::Health {}),
        "hello_required",
    );
    let mut bad = hello(&h);
    if let Operation::Hello { token, .. } = &mut bad {
        *token = "0".repeat(64);
    }
    code(exchange(&mut h.raw(), bad), "unauthorized");
    let mut bad = hello(&h);
    if let Operation::Hello {
        protocol_version, ..
    } = &mut bad
    {
        *protocol_version = 99;
    }
    code(exchange(&mut h.raw(), bad), "unsupported_protocol");
    let response = exchange(&mut h.raw(), hello(&h));
    match response.result.unwrap() {
        Reply::Hello {
            capabilities,
            max_frame,
            protocol_version,
            ..
        } => {
            assert_eq!(protocol_version, 1);
            assert_eq!(max_frame, 8388608);
            assert!(capabilities.contains(&"health".into()));
            assert!(!capabilities.contains(&"transform.evaluate".into()));
        }
        _ => panic!("expected hello"),
    }
}
#[test]
fn atomic_session_binding_schema_and_accounting_rejection() {
    let h = Harness::start(Config::default());
    let mut client = h.client();
    let mut batch = events()[..2].to_vec();
    batch[1].session_id = "private-other-session".into();
    assert!(
        matches!(client.request(Operation::AppendBatch { events: batch }), Err(cs_daemon::Error::Remote { code }) if code == "binding_mismatch")
    );
    assert!(client.request(Operation::Snapshot {}).is_err());
    let mut bad = events()[..2].to_vec();
    bad[1].producer_id = "other-producer".into();
    assert!(client
        .request(Operation::AppendBatch { events: bad })
        .is_err());
    let mut bad = events()[..2].to_vec();
    bad[1].producer_epoch = "other-epoch".into();
    assert!(client
        .request(Operation::AppendBatch { events: bad })
        .is_err());
    let mut bad = events()[..2].to_vec();
    bad[1].schema_version = 2;
    assert!(
        matches!(client.request(Operation::AppendBatch { events: bad }), Err(cs_daemon::Error::Remote { code }) if code == "invalid_event")
    );
    let mut bad = events()[..2].to_vec();
    bad[1].payload = cs_core::Payload::UsageObserved {
        request_id: "r".into(),
        mode: cs_core::UsageMode::Cumulative,
        usage: cs_core::Usage {
            total_input: Some(1),
            cache_read_input: Some(2),
            cache_write_input: Some(0),
            ..Default::default()
        },
    };
    assert!(client
        .request(Operation::AppendBatch { events: bad })
        .is_err());
    assert!(client.request(Operation::Snapshot {}).is_err());
    let conn = rusqlite::Connection::open(h.home.join("ledger.sqlite3")).unwrap();
    let count: i64 = conn
        .query_row("SELECT count(*) FROM events", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
}
#[test]
fn append_batches_deduplicate_and_survive_restart() {
    let mut h = Harness::start(Config::default());
    {
        let mut client = h.client();
        for batch in events().chunks(128) {
            match client
                .request(Operation::AppendBatch {
                    events: batch.to_vec(),
                })
                .unwrap()
            {
                Reply::Appended { inserted } => assert_eq!(inserted, batch.len()),
                _ => panic!("expected append"),
            }
        }
        assert!(matches!(
            client
                .request(Operation::Append {
                    event: Box::new(events()[0].clone())
                })
                .unwrap(),
            Reply::Appended { inserted: 0 }
        ));
    }
    h.restart();
    let mut client = h.client();
    match client.request(Operation::Snapshot {}).unwrap() {
        Reply::Snapshot { report } => {
            assert_eq!(report.event_count, 501);
            assert_eq!(report.request_count, 100);
            assert_eq!(report.tool_count, 100);
            assert_eq!(report.usage.total_tokens, Some(12000));
            assert!(report.synthetic);
        }
        _ => panic!("expected snapshot"),
    }
}
#[test]
fn cap_batch_size_and_session_capabilities() {
    let h = Harness::start(Config {
        batch_events: 1,
        ..Config::default()
    });
    let mut client = h.client();
    assert!(
        matches!(client.request(Operation::AppendBatch { events: events()[..2].to_vec() }), Err(cs_daemon::Error::Remote { code }) if code == "invalid_batch_size")
    );
    let mut unbound = Client::connect(&h.home, "admin", "epoch-1", None).unwrap();
    assert!(
        matches!(unbound.request(Operation::Snapshot {}), Err(cs_daemon::Error::Remote { code }) if code == "capability_not_granted")
    );
    assert!(matches!(
        client
            .request(Operation::Append {
                event: Box::new(events()[0].clone())
            })
            .unwrap(),
        Reply::Appended { inserted: 1 }
    ));
}
#[test]
fn runtime_is_private_singleton_and_cleans_up_without_leaking_token() {
    let mut h = Harness::start(Config::default());
    for (name, mode) in [
        ("runtime", 0o700),
        ("runtime/daemon.sock", 0o600),
        ("runtime/daemon.token", 0o600),
        ("runtime/daemon.lock", 0o600),
        ("spool/daemon-loss.jsonl", 0o600),
    ] {
        assert_eq!(
            fs::metadata(h.home.join(name)).unwrap().mode() & 0o777,
            mode
        );
    }
    let original = fs::read(h.home.join("runtime/daemon.token")).unwrap();
    assert!(Server::bind(&h.home, Config::default()).is_err());
    assert!(original == fs::read(h.home.join("runtime/daemon.token")).unwrap());
    assert!(matches!(
        h.client().request(Operation::Health {}).unwrap(),
        Reply::Health { .. }
    ));
    h.stop.store(true, Ordering::Release);
    h.thread.take().unwrap().join().unwrap().unwrap();
    assert!(!h.home.join("runtime/daemon.sock").exists());
    assert!(!h.home.join("runtime/daemon.token").exists());
}
#[test]
fn unsafe_endpoint_symlinks_are_rejected_without_changing_target() {
    let mut h = Harness::start(Config::default());
    h.stop.store(true, Ordering::Release);
    h.thread.take().unwrap().join().unwrap().unwrap();
    let target = h.home.join("unrelated");
    fs::write(&target, "keep this file").unwrap();
    symlink(&target, h.home.join("runtime/daemon.token")).unwrap();
    assert!(Server::bind(&h.home, Config::default()).is_err());
    assert_eq!(fs::read_to_string(target).unwrap(), "keep this file");
    assert!(fs::symlink_metadata(h.home.join("runtime/daemon.token"))
        .unwrap()
        .file_type()
        .is_symlink());
}
#[test]
fn oversized_and_malformed_frames_are_safe_errors() {
    let h = Harness::start(Config::default());
    let mut stream = h.raw();
    stream.write_all(&8388609_u32.to_be_bytes()).unwrap();
    code(
        serde_json::from_slice(&protocol::read_frame(&mut stream).unwrap()).unwrap(),
        "invalid_frame",
    );
    let mut stream = h.raw();
    let raw = b"PRIVATE_BODY_NOT_JSON";
    stream.write_all(&(raw.len() as u32).to_be_bytes()).unwrap();
    stream.write_all(raw).unwrap();
    let result = protocol::read_frame(&mut stream).unwrap();
    assert!(!String::from_utf8_lossy(&result).contains("PRIVATE_BODY"));
    code(serde_json::from_slice(&result).unwrap(), "invalid_request");
}
#[test]
fn drip_fed_frames_expire_at_the_absolute_deadline() {
    let h = Harness::start(Config {
        timeout: Duration::from_millis(150),
        ..Config::default()
    });
    let mut stream = h.raw();
    let mut sender = stream.try_clone().unwrap();
    let start = Instant::now();
    stream.write_all(&100_u32.to_be_bytes()).unwrap();
    let drip = thread::spawn(move || {
        for _ in 0..20 {
            thread::sleep(Duration::from_millis(30));
            if sender.write_all(b" ").is_err() {
                break;
            }
        }
    });
    let response = protocol::read_frame(&mut stream).unwrap();
    code(serde_json::from_slice(&response).unwrap(), "invalid_frame");
    assert!(start.elapsed() < Duration::from_millis(500));
    drip.join().unwrap();
}
#[test]
fn connection_admission_is_bounded_and_observable() {
    let h = Harness::start(Config {
        connections: 1,
        timeout: Duration::from_millis(180),
        ..Config::default()
    });
    let mut admitted = h.raw();
    assert!(exchange(&mut admitted, hello(&h)).ok);
    let mut rejected = h.raw();
    let mut byte = [0];
    assert_eq!(rejected.read(&mut byte).unwrap(), 0);
    match exchange(&mut admitted, Operation::Health {})
        .result
        .unwrap()
    {
        Reply::Health {
            connection_rejections,
            ..
        } => assert!(connection_rejections >= 1),
        _ => panic!("expected health"),
    }
}
#[test]
fn locked_database_saturates_bounded_queue_and_records_loss() {
    let h = Harness::start(Config {
        connections: 16,
        queue_capacity: 1,
        timeout: Duration::from_millis(600),
        ..Config::default()
    });
    let lock = rusqlite::Connection::open(h.home.join("ledger.sqlite3")).unwrap();
    lock.execute_batch("BEGIN IMMEDIATE").unwrap();
    let clients = (0..8).map(|_| h.client()).collect::<Vec<_>>();
    let barrier = Arc::new(Barrier::new(8));
    let workers = clients
        .into_iter()
        .map(|mut client| {
            let barrier = barrier.clone();
            thread::spawn(move || {
                barrier.wait();
                client.request(Operation::Append {
                    event: Box::new(events()[0].clone()),
                })
            })
        })
        .collect::<Vec<_>>();
    let results = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect::<Vec<_>>();
    assert!(results.iter().all(Result::is_err));
    assert!(results
        .iter()
        .any(|r| matches!(r, Err(cs_daemon::Error::Remote { code }) if code == "queue_full")));
    lock.execute_batch("ROLLBACK").unwrap();
    let mut client = h.client();
    match client.request(Operation::Health {}).unwrap() {
        Reply::Health {
            queue_rejections,
            persistence_failures,
            ..
        } => {
            assert!(queue_rejections > 0);
            assert!(persistence_failures > 0);
        }
        _ => panic!("expected health"),
    }
    let notice = fs::read_to_string(h.home.join("spool/daemon-loss.jsonl")).unwrap();
    assert!(notice.contains("queue_full"));
    assert!(!notice.contains("ordinary"));
    assert!(notice.len() <= 65536);
}
