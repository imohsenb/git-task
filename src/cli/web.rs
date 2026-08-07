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
use crate::web::{install, paths, process};

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

#[derive(Serialize)]
struct WebStatusJson {
    running: bool,
    pid: Option<u32>,
    url: Option<String>,
    log: String,
}

pub fn run(args: WebArgs) -> Result<()> {
    match args.action {
        WebAction::Start(a) => start(a),
        WebAction::Stop(a) => stop(a),
        WebAction::Status(a) => status(a),
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
    }

    let host = args.host.clone().unwrap_or_else(|| DEFAULT_HOST.to_string());
    let port = args.port.unwrap_or(DEFAULT_PORT);
    let cli_js = paths::cli_js_path()?;

    let pid = process::spawn(&cli_js, &log_path, args.port, args.host.as_deref())?;
    process::write_state(&state_path, &process::WebState { pid, host: host.clone(), port })?;

    let ready = wait_for_port(&host, port, Duration::from_secs(10));
    let url = format!("http://{host}:{port}");

    if ready {
        Logger::info(&format!("Started git-task-web at {url} (pid {pid})"), None, &[]);
    } else {
        Logger::warn(
            &format!("Spawned git-task-web (pid {pid}) but it didn't come up within 10s"),
            Some(&format!("check {}", log_path.display())),
            &[],
        );
    }

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
