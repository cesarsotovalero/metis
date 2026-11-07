#![forbid(unsafe_code)]
use std::{path::Path, time::Duration};
#[cfg(target_os = "linux")]
mod client;
pub mod protocol;
#[cfg(target_os = "linux")]
mod runtime;
#[cfg(target_os = "linux")]
mod server;
#[cfg(target_os = "linux")]
mod writer;
#[cfg(target_os = "linux")]
pub use {client::Client, server::Server};

pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("local daemon I/O failure: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Contract(&'static str),
    #[error("daemon rejected request: {code}")]
    Remote { code: String },
    #[error("invalid local IPC message")]
    Json(#[from] serde_json::Error),
}
#[derive(Clone)]
pub struct Config {
    pub connections: usize,
    pub queue_capacity: usize,
    pub batch_events: usize,
    pub timeout: Duration,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            connections: 16,
            queue_capacity: 64,
            batch_events: 128,
            timeout: Duration::from_secs(2),
        }
    }
}
impl Config {
    #[cfg(target_os = "linux")]
    fn validate(&self) -> Result<()> {
        if self.connections == 0
            || self.connections > 64
            || self.queue_capacity == 0
            || self.queue_capacity > 256
            || self.batch_events == 0
            || self.batch_events > 128
            || self.timeout.is_zero()
            || self.timeout > Duration::from_secs(30)
        {
            return Err(Error::Contract("invalid daemon limits"));
        }
        Ok(())
    }
}
#[cfg(target_os = "linux")]
pub fn run(home: &Path, config: Config) -> Result<()> {
    let server = Server::bind(home, config)?;
    let stop = server.shutdown_flag();
    let sigint = signal_hook::flag::register(signal_hook::consts::SIGINT, stop.clone())?;
    let sigterm = match signal_hook::flag::register(signal_hook::consts::SIGTERM, stop) {
        Ok(id) => id,
        Err(error) => {
            signal_hook::low_level::unregister(sigint);
            return Err(error.into());
        }
    };
    let result = server.serve();
    signal_hook::low_level::unregister(sigint);
    signal_hook::low_level::unregister(sigterm);
    result
}
#[cfg(not(target_os = "linux"))]
pub fn run(_home: &Path, _config: Config) -> Result<()> {
    Err(Error::Contract(
        "daemon IPC is currently supported on Linux only",
    ))
}
