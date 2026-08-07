use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{Context, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebState {
    pub pid: u32,
    pub host: String,
    pub port: u16,
}

/// Reads `<pid> <host> <port>` from `path`. `None` if the file is missing or malformed — a
/// corrupt/truncated file is treated the same as "not running," and cleaned up by the caller.
pub fn read_state(path: &Path) -> Option<WebState> {
    let text = fs::read_to_string(path).ok()?;
    let mut parts = text.split_whitespace();
    let pid = parts.next()?.parse().ok()?;
    let host = parts.next()?.to_string();
    let port = parts.next()?.parse().ok()?;
    Some(WebState { pid, host, port })
}

pub fn write_state(path: &Path, state: &WebState) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, format!("{} {} {}", state.pid, state.host, state.port))
}

pub fn remove_state(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// Spawns `node <cli_js>` detached — reparented (not killed) when this process exits, same as
/// `sync::trigger`'s spawn — redirecting stdout+stderr to `log_path` (kept, unlike `sync`'s fully
/// nulled streams: this is a user-visible long-lived server whose output is worth keeping for
/// `stop`/`status` follow-up debugging). Returns the child's PID so the caller can persist it.
pub fn spawn(cli_js: &Path, log_path: &Path, port: Option<u16>, host: Option<&str>) -> Result<u32> {
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let log_out = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .with_context(|| format!("opening {}", log_path.display()))?;
    let log_err: File = log_out.try_clone().with_context(|| format!("opening {}", log_path.display()))?;

    let mut cmd = Command::new("node");
    cmd.arg(cli_js).stdin(Stdio::null()).stdout(log_out).stderr(log_err);
    if let Some(port) = port {
        cmd.env("GIT_TASK_WEB_PORT", port.to_string());
    }
    if let Some(host) = host {
        cmd.env("GIT_TASK_WEB_HOST", host);
    }

    // A plain `spawn()` leaves the child in this process's process group/session — fine for
    // `sync::trigger`'s short-lived worker (it finishes before anyone notices), fatal for a
    // long-lived server: the invoking shell exiting (or, e.g., a job-control/terminal-close
    // SIGHUP) can take the whole group down, child included. `setsid()` in the child before
    // `exec` gives it its own session so it survives the parent CLI process exiting, same as
    // real daemonization.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
    }

    let child = cmd.spawn().context("spawning `node` for git-task-web")?;
    Ok(child.id())
}

/// Signal 0 sends nothing — `kill(2)` still validates that a process with this PID exists (and
/// is signalable by us), which is exactly the liveness check needed for a stale-PID-file check.
#[cfg(unix)]
pub fn is_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

#[cfg(not(unix))]
pub fn is_alive(_pid: u32) -> bool {
    false
}

/// `SIGTERM`, then escalate to `SIGKILL` if it hasn't exited within 5s.
#[cfg(unix)]
pub fn stop(pid: u32) -> io::Result<()> {
    unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while is_alive(pid) && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    if is_alive(pid) {
        unsafe { libc::kill(pid as libc::pid_t, libc::SIGKILL) };
    }
    Ok(())
}

#[cfg(not(unix))]
pub fn stop(_pid: u32) -> io::Result<()> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "stopping the web server isn't supported on this platform yet"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("web.state");
        assert_eq!(read_state(&path), None);

        let state = WebState { pid: 4242, host: "127.0.0.1".to_string(), port: 4600 };
        write_state(&path, &state).unwrap();
        assert_eq!(read_state(&path), Some(state));

        remove_state(&path).unwrap();
        assert_eq!(read_state(&path), None);
    }

    #[test]
    fn corrupt_state_file_reads_as_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("web.state");
        fs::write(&path, "not-a-pid").unwrap();
        assert_eq!(read_state(&path), None);
    }

    #[test]
    fn remove_state_missing_file_is_ok() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("web.state");
        assert!(remove_state(&path).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn current_process_is_alive() {
        assert!(is_alive(std::process::id()));
    }

    #[cfg(unix)]
    #[test]
    fn exited_process_is_not_alive() {
        let mut child = Command::new("true").spawn().expect("spawn `true`");
        let pid = child.id();
        child.wait().unwrap();
        assert!(!is_alive(pid));
    }
}
