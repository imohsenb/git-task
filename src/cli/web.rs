use std::net::{TcpStream, ToSocketAddrs};
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{bail, Result};
use clap::{Args, Subcommand};
use serde::Serialize;

use crate::logger::Logger;
use crate::output;
use crate::prompt;
use crate::ui;
use crate::web::{install, paths, process, update};

const DEFAULT_HOST: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 4600;

#[derive(Args)]
pub struct WebArgs {
    #[command(subcommand)]
    action: WebAction,
}

#[derive(Subcommand)]
enum WebAction {
    /// Start the web UI server in the background (installs it first if needed)
    Start(StartArgs),
    /// Stop the background web UI server
    Stop(StopArgs),
    /// Show whether the web UI server is running
    Status(StatusArgs),
    /// Update git-task-web to the latest version, restarting it if it's currently running
    Upgrade(UpgradeArgs),
}

#[derive(Args)]
pub struct StartArgs {
    /// Port to serve on (defaults to git-task-web's own default, 4600)
    #[arg(long)]
    port: Option<u16>,
    /// Host/address to bind (defaults to git-task-web's own default, 127.0.0.1)
    #[arg(long)]
    host: Option<String>,
    /// Install git-task-web without an interactive prompt, if it isn't installed yet
    #[arg(short = 'y', long)]
    yes: bool,
}

#[derive(Args)]
pub struct StopArgs {}

#[derive(Args)]
pub struct StatusArgs {}

#[derive(Args)]
pub struct UpgradeArgs {}

#[derive(Serialize)]
struct WebStatusJson {
    running: bool,
    pid: Option<u32>,
    url: Option<String>,
    log: String,
}

#[derive(Serialize)]
struct UpgradeJson {
    from: Option<String>,
    to: Option<String>,
    restarted: bool,
}

pub fn run(args: WebArgs) -> Result<()> {
    match args.action {
        WebAction::Start(a) => start(a),
        WebAction::Stop(a) => stop(a),
        WebAction::Status(a) => status(a),
        WebAction::Upgrade(a) => upgrade(a),
    }
}

fn start(args: StartArgs) -> Result<()> {
    let state_path = paths::state_path()?;
    let log_path = paths::log_path()?;

    if let Some(state) = process::read_state(&state_path) {
        if process::is_alive(state.pid) {
            let url = format!("http://{}:{}", state.host, state.port);
            Logger::info(&format!("Already running at {url} (pid {})", state.pid), None, &[]);
            if output::is_json() {
                output::print_ok(WebStatusJson {
                    running: true,
                    pid: Some(state.pid),
                    url: Some(url),
                    log: log_path.display().to_string(),
                });
            }
            return Ok(());
        }
        // Stale state file from a crashed/killed process — clean it up and spawn fresh below.
        let _ = process::remove_state(&state_path);
    }

    if !install::is_installed()? {
        let proceed = if args.yes {
            true
        } else if output::is_json() {
            bail!("git-task-web isn't installed yet — re-run with --yes to install it non-interactively");
        } else if !prompt::is_interactive() {
            bail!("git-task-web isn't installed yet — re-run with --yes to install it");
        } else {
            ui::prompt_confirm("git-task-web isn't installed yet. Install it now via npm?", true)?
        };

        if !proceed {
            Logger::info("Skipped install. Re-run with --yes (or accept the prompt) when you're ready.", None, &[]);
            return Ok(());
        }

        Logger::info("Installing git-task-web via npm...", None, &[]);
        install::install()?;
        Logger::info("Installed.", None, &[]);
    } else {
        maybe_prompt_upgrade(args.yes)?;
    }

    let host = args.host.clone().unwrap_or_else(|| DEFAULT_HOST.to_string());
    let port = args.port.unwrap_or(DEFAULT_PORT);
    let (pid, ready) = spawn_and_wait(&state_path, &log_path, host.clone(), port)?;
    let url = format!("http://{host}:{port}");

    if output::is_json() {
        output::print_ok(WebStatusJson {
            running: ready,
            pid: Some(pid),
            url: ready.then_some(url),
            log: log_path.display().to_string(),
        });
    }
    Ok(())
}

/// The actual spawn-and-wait mechanics, shared by `start` and `upgrade`'s restart-after-upgrade
/// step — factored out so each caller prints its own single JSON envelope (`output::print_ok`
/// is "the only stdout write a command makes in JSON mode"; calling `start` itself from inside
/// `upgrade` would print two).
fn spawn_and_wait(state_path: &Path, log_path: &Path, host: String, port: u16) -> Result<(u32, bool)> {
    let cli_js = paths::cli_js_path()?;
    let pid = process::spawn(&cli_js, log_path, Some(port), Some(&host))?;
    process::write_state(state_path, &process::WebState { pid, host: host.clone(), port })?;

    let ready = wait_for_port(&host, port, Duration::from_secs(10));
    if ready {
        Logger::info(&format!("Started git-task-web at http://{host}:{port} (pid {pid})"), None, &[]);
    } else {
        Logger::warn(
            &format!("Spawned git-task-web (pid {pid}) but it didn't come up within 10s"),
            Some(&format!("check {}", log_path.display())),
            &[],
        );
    }
    Ok((pid, ready))
}

/// Checks npm for a newer git-task-web and, on a TTY, asks before upgrading. Never blocks
/// `start`: a network failure, a `--format json`/non-interactive caller, or a "no" answer all
/// fall through to starting whatever's already installed.
fn maybe_prompt_upgrade(yes: bool) -> Result<()> {
    let Some(current) = update::installed_version()? else { return Ok(()) };
    let Some(latest) = update::latest_version() else { return Ok(()) };
    if !update::is_newer(&current, &latest) {
        return Ok(());
    }

    let proceed = if yes {
        true
    } else if output::is_json() || !prompt::is_interactive() {
        Logger::warn(
            &format!("git-task-web {latest} is available (installed: {current})"),
            Some("run `git task web upgrade` to update"),
            &[],
        );
        false
    } else {
        ui::prompt_confirm(&format!("git-task-web {latest} is available (installed: {current}). Update now?"), false)?
    };

    if !proceed {
        return Ok(());
    }

    Logger::info("Upgrading git-task-web via npm...", None, &[]);
    install::install()?;
    Logger::info(&format!("Upgraded to {latest}."), None, &[]);
    Ok(())
}

fn stop(_args: StopArgs) -> Result<()> {
    let state_path = paths::state_path()?;
    let log_path = paths::log_path()?;

    let Some(state) = process::read_state(&state_path) else {
        Logger::info("Not running.", None, &[]);
        if output::is_json() {
            output::print_ok(not_running_json(&log_path));
        }
        return Ok(());
    };

    if !process::is_alive(state.pid) {
        let _ = process::remove_state(&state_path);
        Logger::info("Not running (cleared a stale state file).", None, &[]);
        if output::is_json() {
            output::print_ok(not_running_json(&log_path));
        }
        return Ok(());
    }

    process::stop(state.pid)?;
    process::remove_state(&state_path)?;
    Logger::info(&format!("Stopped git-task-web (pid {}).", state.pid), None, &[]);
    if output::is_json() {
        output::print_ok(not_running_json(&log_path));
    }
    Ok(())
}

fn status(_args: StatusArgs) -> Result<()> {
    let state_path = paths::state_path()?;
    let log_path = paths::log_path()?;

    let Some(state) = process::read_state(&state_path) else {
        Logger::info("Not running.", None, &[]);
        if output::is_json() {
            output::print_ok(not_running_json(&log_path));
        }
        return Ok(());
    };

    if !process::is_alive(state.pid) {
        let _ = process::remove_state(&state_path);
        Logger::info("Not running (cleared a stale state file).", None, &[]);
        if output::is_json() {
            output::print_ok(not_running_json(&log_path));
        }
        return Ok(());
    }

    let url = format!("http://{}:{}", state.host, state.port);
    Logger::info(&format!("Running at {url} (pid {}). Log: {}", state.pid, log_path.display()), None, &[]);
    if output::is_json() {
        output::print_ok(WebStatusJson {
            running: true,
            pid: Some(state.pid),
            url: Some(url),
            log: log_path.display().to_string(),
        });
    }
    Ok(())
}

fn upgrade(_args: UpgradeArgs) -> Result<()> {
    let state_path = paths::state_path()?;
    let log_path = paths::log_path()?;

    let running_state = process::read_state(&state_path).filter(|s| process::is_alive(s.pid));

    if let Some(state) = &running_state {
        Logger::info("Stopping git-task-web to upgrade...", None, &[]);
        process::stop(state.pid)?;
        process::remove_state(&state_path)?;
    }

    let from = update::installed_version()?;
    Logger::info("Installing the latest git-task-web via npm...", None, &[]);
    install::install()?;
    let to = update::installed_version()?;

    match (&from, &to) {
        (Some(f), Some(t)) if f == t => Logger::info(&format!("Already at the latest version ({t})."), None, &[]),
        (Some(_), Some(t)) => Logger::info(&format!("Upgraded to {t}."), None, &[]),
        (None, Some(t)) => Logger::info(&format!("Installed {t}."), None, &[]),
        _ => Logger::info("Upgraded.", None, &[]),
    }

    let restarted = if let Some(state) = running_state {
        Logger::info("Restarting git-task-web...", None, &[]);
        spawn_and_wait(&state_path, &log_path, state.host, state.port)?;
        true
    } else {
        false
    };

    if output::is_json() {
        output::print_ok(UpgradeJson { from, to, restarted });
    }
    Ok(())
}

fn not_running_json(log_path: &Path) -> WebStatusJson {
    WebStatusJson { running: false, pid: None, url: None, log: log_path.display().to_string() }
}

fn wait_for_port(host: &str, port: u16, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Ok(mut addrs) = (host, port).to_socket_addrs() {
            if let Some(addr) = addrs.next() {
                if TcpStream::connect_timeout(&addr, Duration::from_millis(300)).is_ok() {
                    return true;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    false
}
