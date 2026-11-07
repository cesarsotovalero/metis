use crate::{
    protocol::{Reply, SafeError},
    runtime::private_file,
    Config, Error, Result,
};
use cs_core::EventEnvelope;
use cs_store::Store;
use std::{
    fs::File,
    io::{Seek, SeekFrom, Write},
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, SyncSender, TrySendError},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Instant,
};

pub(crate) struct Health {
    pub queue_rejections: AtomicU64,
    pub connection_rejections: AtomicU64,
    pub persistence_failures: AtomicU64,
    pub expired_requests: AtomicU64,
    pub spool_failures: Arc<AtomicU64>,
    spool: Arc<Mutex<Option<File>>>,
    notices: SyncSender<String>,
    spool_stop: Arc<AtomicBool>,
    spool_worker: Mutex<Option<JoinHandle<()>>>,
}
impl Health {
    pub fn open(path: &Path) -> Result<Arc<Self>> {
        let (notices, receiver) = mpsc::sync_channel::<String>(64);
        let health = Arc::new(Self {
            queue_rejections: AtomicU64::new(0),
            connection_rejections: AtomicU64::new(0),
            persistence_failures: AtomicU64::new(0),
            expired_requests: AtomicU64::new(0),
            spool_failures: Arc::new(AtomicU64::new(0)),
            spool: Arc::new(Mutex::new(Some(private_file(path, true)?))),
            notices,
            spool_stop: Arc::new(AtomicBool::new(false)),
            spool_worker: Mutex::new(None),
        });
        let sink = health.spool.clone();
        let failures = health.spool_failures.clone();
        let stop = health.spool_stop.clone();
        let worker = thread::Builder::new()
            .name("mertis-loss-spool".into())
            .spawn(move || loop {
                let line = match receiver.recv_timeout(std::time::Duration::from_millis(10)) {
                    Ok(line) => line,
                    Err(mpsc::RecvTimeoutError::Timeout) if !stop.load(Ordering::Acquire) => {
                        continue
                    }
                    Err(_) => break,
                };
                let write = || -> std::io::Result<()> {
                    let mut guard = sink
                        .lock()
                        .map_err(|_| std::io::Error::other("spool lock poisoned"))?;
                    let file = guard
                        .as_mut()
                        .ok_or_else(|| std::io::Error::other("spool unavailable"))?;
                    if file.metadata()?.len() + line.len() as u64 > 65536 {
                        file.set_len(0)?;
                    }
                    file.seek(SeekFrom::End(0))?;
                    file.write_all(line.as_bytes())?;
                    file.sync_data()
                };
                if write().is_err() {
                    failures.fetch_add(1, Ordering::Relaxed);
                }
            })?;
        *health.spool_worker.lock().expect("new spool worker mutex") = Some(worker);
        Ok(health)
    }
    pub fn snapshot(&self) -> Reply {
        let count = |v: &AtomicU64| v.load(Ordering::Relaxed);
        Reply::Health {
            protocol_version: 1,
            mode: "observe".into(),
            queue_rejections: count(&self.queue_rejections),
            connection_rejections: count(&self.connection_rejections),
            persistence_failures: count(&self.persistence_failures),
            expired_requests: count(&self.expired_requests),
            spool_failures: count(&self.spool_failures),
        }
    }
    pub fn loss(&self, code: &str) {
        // Admission never waits for disk or the sink mutex. The bounded spool
        // channel can itself overflow; that condition is observable in health.
        let line = format!(
            "{{\"code\":\"{code}\",\"health\":{}}}\n",
            serde_json::to_string(&self.snapshot()).expect("health counters serialize")
        );
        if self.notices.try_send(line).is_err() {
            self.spool_failures.fetch_add(1, Ordering::Relaxed);
        }
    }
    pub fn finish_spool(&self, timeout: std::time::Duration) -> Result<()> {
        self.spool_stop.store(true, Ordering::Release);
        let worker = self
            .spool_worker
            .lock()
            .map_err(|_| Error::Contract("spool worker lock poisoned"))?
            .take();
        if let Some(worker) = worker {
            let deadline = Instant::now() + timeout;
            while !worker.is_finished() && Instant::now() < deadline {
                thread::sleep(std::time::Duration::from_millis(1));
            }
            if !worker.is_finished() {
                self.spool_failures.fetch_add(1, Ordering::Relaxed);
                // Detach a stalled sink rather than blocking daemon shutdown.
                return Err(Error::Contract(
                    "loss spool did not drain before shutdown deadline",
                ));
            }
            worker
                .join()
                .map_err(|_| Error::Contract("loss spool stopped unexpectedly"))?;
        }
        Ok(())
    }
}

pub(crate) enum Work {
    Append(Vec<EventEnvelope>),
    Snapshot(String),
}
pub(crate) struct Job {
    pub work: Work,
    pub deadline: Instant,
    pub reply: SyncSender<std::result::Result<Reply, SafeError>>,
}
pub(crate) struct Writer {
    pub sender: SyncSender<Job>,
    worker: JoinHandle<()>,
}
impl Writer {
    pub fn start(home: &Path, config: &Config, health: Arc<Health>) -> Result<Self> {
        let (sender, receiver) = mpsc::sync_channel::<Job>(config.queue_capacity);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let path = home.join("ledger.sqlite3");
        let busy = config.timeout.min(std::time::Duration::from_millis(250));
        let worker = thread::Builder::new()
            .name("mertis-writer".into())
            .spawn(move || {
                let mut store = match Store::open_with_timeout(&path, busy) {
                    Ok(store) => {
                        let _ = ready_tx.send(true);
                        store
                    }
                    Err(_) => {
                        let _ = ready_tx.send(false);
                        return;
                    }
                };
                while let Ok(job) = receiver.recv() {
                    if Instant::now() >= job.deadline {
                        health.expired_requests.fetch_add(1, Ordering::Relaxed);
                        health.loss("expired_before_write");
                        let _ = job.reply.send(Err(SafeError::new("deadline_exceeded")));
                        continue;
                    }
                    let mut result = match job.work {
                        Work::Append(events) => store
                            .import(&events)
                            .map(|inserted| Reply::Appended { inserted }),
                        Work::Snapshot(id) => store.inspect(&id).map(|report| Reply::Snapshot {
                            report: Box::new(report),
                        }),
                    }
                    .map_err(|_| SafeError::new("persistence_failed"));
                    if result.is_err() {
                        health.persistence_failures.fetch_add(1, Ordering::Relaxed);
                        health.loss("persistence_failed");
                    }
                    if Instant::now() >= job.deadline {
                        health.expired_requests.fetch_add(1, Ordering::Relaxed);
                        health.loss("completed_after_deadline");
                        result = Err(SafeError::new("deadline_exceeded"));
                    }
                    let _ = job.reply.send(result);
                }
            })?;
        if ready_rx.recv().unwrap_or(false) {
            Ok(Self { sender, worker })
        } else {
            let _ = worker.join();
            Err(Error::Contract("daemon database initialization failed"))
        }
    }
    pub fn finish(self) -> Result<()> {
        drop(self.sender);
        self.worker
            .join()
            .map_err(|_| Error::Contract("daemon writer stopped unexpectedly"))
    }
}
pub(crate) fn dispatch(
    sender: &SyncSender<Job>,
    work: Work,
    deadline: Instant,
    health: &Health,
) -> std::result::Result<Reply, SafeError> {
    let (reply, rx) = mpsc::sync_channel(1);
    match sender.try_send(Job {
        work,
        deadline,
        reply,
    }) {
        Ok(()) => {}
        Err(TrySendError::Full(_)) => {
            health.queue_rejections.fetch_add(1, Ordering::Relaxed);
            health.loss("queue_full");
            return Err(SafeError::new("queue_full"));
        }
        Err(TrySendError::Disconnected(_)) => {
            health.persistence_failures.fetch_add(1, Ordering::Relaxed);
            health.loss("writer_unavailable");
            return Err(SafeError::new("writer_unavailable"));
        }
    }
    match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => {
            // The writer alone counts expired requests, whether discarded from
            // the queue or completed late. Dispatch only emits a loss notice.
            health.loss("dispatch_timeout");
            Err(SafeError::new("deadline_exceeded"))
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            health.persistence_failures.fetch_add(1, Ordering::Relaxed);
            health.loss("writer_unavailable");
            Err(SafeError::new("writer_unavailable"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::mpsc, time::Duration};
    fn home(suffix: &str) -> std::path::PathBuf {
        let path =
            std::env::temp_dir().join(format!("mertis-writer-{}-{suffix}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        path
    }
    fn event() -> EventEnvelope {
        serde_json::from_str(
            include_str!("../../../fixtures/ordinary.jsonl")
                .lines()
                .next()
                .unwrap(),
        )
        .unwrap()
    }
    #[test]
    fn loss_reporting_returns_while_the_spool_sink_is_stalled() {
        let home = home("stalled-spool");
        let health = Health::open(&home.join("loss.jsonl")).unwrap();
        let blocked_sink = health.spool.lock().unwrap();
        let (tx, rx) = mpsc::sync_channel(1);
        let cloned = health.clone();
        let worker = thread::spawn(move || {
            cloned.loss("queue_full");
            tx.send(()).unwrap();
        });
        let returned = rx.recv_timeout(Duration::from_millis(100));
        drop(blocked_sink);
        worker.join().unwrap();
        assert!(
            returned.is_ok(),
            "loss reporting blocked on stalled disk sink"
        );
        drop(health);
        std::fs::remove_dir_all(home).unwrap();
    }
    #[test]
    fn spool_rotation_and_write_failure_are_bounded_and_observable() {
        let home = home("spool-rotation");
        let path = home.join("loss.jsonl");
        let health = Health::open(&path).unwrap();
        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(&vec![b'x'; 65535])
            .unwrap();
        health.loss("queue_full");
        health.finish_spool(Duration::from_secs(1)).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.len() <= 65536);
        assert!(
            serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["code"] == "queue_full"
        );
        drop(health);
        let health = Health::open(&path).unwrap();
        // A real read-only descriptor causes an OS write failure, standing in for
        // the unavailable/full disk boundary without a fake filesystem.
        *health.spool.lock().unwrap() = Some(File::open(&path).unwrap());
        health.loss("persistence_failed");
        health.finish_spool(Duration::from_secs(1)).unwrap();
        assert_eq!(health.spool_failures.load(Ordering::Relaxed), 1);
        drop(health);
        std::fs::remove_dir_all(home).unwrap();
    }
    #[test]
    fn a_stalled_spool_has_bounded_admission_and_shutdown() {
        let home = home("spool-admission");
        let health = Health::open(&home.join("loss.jsonl")).unwrap();
        let stalled = health.spool.lock().unwrap();
        for _ in 0..256 {
            health.loss("queue_full");
        }
        assert!(health.spool_failures.load(Ordering::Relaxed) > 0);
        let start = Instant::now();
        assert!(health.finish_spool(Duration::from_millis(20)).is_err());
        assert!(start.elapsed() < Duration::from_millis(250));
        drop(stalled);
        drop(health);
        std::fs::remove_dir_all(home).unwrap();
    }
    #[test]
    fn expired_queued_work_never_commits_and_counts_once() {
        let home = home("expired-queue");
        let health = Health::open(&home.join("loss.jsonl")).unwrap();
        let writer = Writer::start(&home, &Config::default(), health.clone()).unwrap();
        let db = rusqlite::Connection::open(home.join("ledger.sqlite3")).unwrap();
        db.execute_batch("BEGIN IMMEDIATE").unwrap();
        let (reply, first) = mpsc::sync_channel(1);
        writer
            .sender
            .send(Job {
                work: Work::Append(vec![event()]),
                deadline: Instant::now() + Duration::from_secs(2),
                reply,
            })
            .unwrap();
        let expired = dispatch(
            &writer.sender,
            Work::Append(vec![event()]),
            Instant::now() - Duration::from_secs(1),
            &health,
        );
        assert_eq!(expired.unwrap_err().code, "deadline_exceeded");
        assert!(first.recv_timeout(Duration::from_secs(2)).unwrap().is_err());
        writer.finish().unwrap();
        db.execute_batch("ROLLBACK").unwrap();
        let count: i64 = db
            .query_row("SELECT count(*) FROM events", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
        assert_eq!(
            health.expired_requests.load(Ordering::Relaxed),
            1,
            "a timed-out queued request was counted twice"
        );
        drop(health);
        drop(db);
        std::fs::remove_dir_all(home).unwrap();
    }
}
