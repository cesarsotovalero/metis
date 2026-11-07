use crate::{Error, Result};
use nix::{
    sys::socket::{getsockopt, sockopt::PeerCredentials},
    unistd::geteuid,
};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::{
        fs::{DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

fn private_dir(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) => {
            if !meta.is_dir() || meta.uid() != geteuid().as_raw() || meta.mode() & 0o777 != 0o700 {
                return Err(Error::Contract(
                    "daemon directories must be owner-only directories",
                ));
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(path)?;
        }
        Err(e) => return Err(e.into()),
    }
    Ok(())
}
pub(crate) fn private_file(path: &Path, create: bool) -> Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(create)
        .truncate(false)
        .mode(0o600)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
        .open(path)?;
    let meta = file.metadata()?;
    if !meta.is_file()
        || meta.uid() != geteuid().as_raw()
        || meta.mode() & 0o777 != 0o600
        || meta.nlink() != 1
    {
        return Err(Error::Contract("daemon files must be private owner files"));
    }
    Ok(file)
}
pub(crate) struct Runtime {
    pub listener: UnixListener,
    pub token: String,
    pub spool: PathBuf,
    root: PathBuf,
    _lock: File,
}
struct Cleanup {
    root: PathBuf,
    armed: bool,
}
impl Drop for Cleanup {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_file(self.root.join("daemon.sock"));
            let _ = fs::remove_file(self.root.join("daemon.token"));
        }
    }
}
impl Runtime {
    pub fn bind(home: &Path) -> Result<Self> {
        private_dir(home)?;
        let root = home.join("runtime");
        private_dir(&root)?;
        let spool = home.join("spool");
        private_dir(&spool)?;
        let lock = private_file(&root.join("daemon.lock"), true)?;
        lock.try_lock()
            .map_err(|_| Error::Contract("daemon already running or runtime lock unavailable"))?;
        for (name, socket) in [("daemon.sock", true), ("daemon.token", false)] {
            let path = root.join(name);
            match fs::symlink_metadata(&path) {
                Ok(meta) => {
                    if meta.uid() != geteuid().as_raw()
                        || meta.mode() & 0o077 != 0
                        || (socket && !meta.file_type().is_socket())
                        || (!socket && (!meta.is_file() || meta.nlink() != 1))
                    {
                        return Err(Error::Contract("unsafe stale daemon endpoint"));
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        let mut cleanup = Cleanup {
            root: root.clone(),
            armed: true,
        };
        for name in ["daemon.sock", "daemon.token"] {
            match fs::remove_file(root.join(name)) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        let mut secret = [0_u8; 32];
        getrandom::fill(&mut secret).map_err(|_| Error::Contract("OS randomness unavailable"))?;
        let token: String = secret.iter().map(|b| format!("{b:02x}")).collect();
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(root.join("daemon.token"))?;
        file.write_all(token.as_bytes())?;
        file.sync_all()?;
        let listener = UnixListener::bind(root.join("daemon.sock"))?;
        fs::set_permissions(root.join("daemon.sock"), fs::Permissions::from_mode(0o600))?;
        listener.set_nonblocking(true)?;
        cleanup.armed = false;
        Ok(Self {
            listener,
            token,
            spool,
            root,
            _lock: lock,
        })
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        drop(Cleanup {
            root: self.root.clone(),
            armed: true,
        });
    }
}
pub(crate) fn owner_peer(stream: &UnixStream) -> bool {
    getsockopt(stream, PeerCredentials).is_ok_and(|peer| peer.uid() == geteuid().as_raw())
}
pub(crate) fn token(home: &Path) -> Result<String> {
    private_dir(home)?;
    private_dir(&home.join("runtime"))?;
    let file = private_file(&home.join("runtime/daemon.token"), false)?;
    let mut value = String::new();
    file.take(65).read_to_string(&mut value)?;
    if value.len() != 64 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::Contract("invalid daemon credential file"));
    }
    Ok(value)
}
/// A fresh remaining socket timeout is applied before every system call, based
/// on one shared deadline. A sender cannot extend the frame by dripping bytes.
pub(crate) struct DeadlineStream<'a> {
    pub stream: &'a mut UnixStream,
    pub deadline: Instant,
}
impl DeadlineStream<'_> {
    fn remaining(&self) -> std::io::Result<Duration> {
        self.deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::TimedOut, "IPC deadline exceeded")
            })
    }
}
impl Read for DeadlineStream<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.stream.set_read_timeout(Some(self.remaining()?))?;
        self.stream.read(buf)
    }
}
impl Write for DeadlineStream<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.stream.set_write_timeout(Some(self.remaining()?))?;
        self.stream.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.stream.flush()
    }
}
