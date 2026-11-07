#![cfg(target_os = "linux")]
use std::{
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
struct Running {
    child: Child,
    home: PathBuf,
}
impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.home);
    }
}
#[test]
fn daemon_start_status_stop_round_trip() {
    let home = std::env::temp_dir().join(format!("mertis-daemon-cli-{}", std::process::id()));
    let child = Command::new(env!("CARGO_BIN_EXE_mertis"))
        .arg("--home")
        .arg(&home)
        .args(["daemon", "start"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut running = Running { child, home };
    let deadline = Instant::now() + Duration::from_secs(4);
    while !running.home.join("runtime/daemon.sock").exists() {
        assert!(
            running.child.try_wait().unwrap().is_none(),
            "daemon start exited before creating its socket"
        );
        assert!(Instant::now() < deadline, "daemon did not become ready");
        std::thread::sleep(Duration::from_millis(10));
    }
    let status = Command::new(env!("CARGO_BIN_EXE_mertis"))
        .arg("--home")
        .arg(&running.home)
        .args(["daemon", "status"])
        .output()
        .unwrap();
    assert!(
        status.status.success(),
        "{}",
        String::from_utf8_lossy(&status.stderr)
    );
    let health: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(health["protocol_version"], 1);
    assert_eq!(health["mode"], "observe");
    let stop = Command::new(env!("CARGO_BIN_EXE_mertis"))
        .arg("--home")
        .arg(&running.home)
        .args(["daemon", "stop"])
        .output()
        .unwrap();
    assert!(stop.status.success());
    while running.child.try_wait().unwrap().is_none() {
        assert!(Instant::now() < deadline, "daemon did not stop");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!running.home.join("runtime/daemon.sock").exists());
    assert!(!running.home.join("runtime/daemon.token").exists());
}

fn start(label: &str) -> Running {
    let home = std::env::temp_dir().join(format!("mertis-daemon-{label}-{}", std::process::id()));
    let child = Command::new(env!("CARGO_BIN_EXE_mertis"))
        .arg("--home")
        .arg(&home)
        .args(["daemon", "start"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut running = Running { child, home };
    let deadline = Instant::now() + Duration::from_secs(4);
    while !running.home.join("runtime/daemon.sock").exists() {
        assert!(running.child.try_wait().unwrap().is_none());
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    // Socket creation precedes the ready database writer; authenticated status
    // confirms readiness before this test sends signals or kills the process.
    let status = Command::new(env!("CARGO_BIN_EXE_mertis"))
        .arg("--home")
        .arg(&running.home)
        .args(["daemon", "status"])
        .output()
        .unwrap();
    assert!(status.status.success());
    running
}
#[test]
fn sigterm_shuts_down_cleanly_and_removes_endpoints() {
    let mut running = start("sigterm");
    assert!(Command::new("kill")
        .args(["-TERM", &running.child.id().to_string()])
        .status()
        .unwrap()
        .success());
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        if let Some(status) = running.child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!running.home.join("runtime/daemon.sock").exists());
    assert!(!running.home.join("runtime/daemon.token").exists());
}
#[test]
fn explicit_restart_recovers_stale_endpoints_after_a_crash() {
    let mut running = start("crash");
    let old = std::fs::read(running.home.join("runtime/daemon.token")).unwrap();
    running.child.kill().unwrap();
    running.child.wait().unwrap();
    assert!(running.home.join("runtime/daemon.sock").exists());
    running.child = Command::new(env!("CARGO_BIN_EXE_mertis"))
        .arg("--home")
        .arg(&running.home)
        .args(["daemon", "start"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        let status = Command::new(env!("CARGO_BIN_EXE_mertis"))
            .arg("--home")
            .arg(&running.home)
            .args(["daemon", "status"])
            .output()
            .unwrap();
        if status.status.success() {
            break;
        }
        assert!(running.child.try_wait().unwrap().is_none());
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(old != std::fs::read(running.home.join("runtime/daemon.token")).unwrap());
    assert!(Command::new(env!("CARGO_BIN_EXE_mertis"))
        .arg("--home")
        .arg(&running.home)
        .args(["daemon", "stop"])
        .output()
        .unwrap()
        .status
        .success());
}
