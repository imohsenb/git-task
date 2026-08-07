use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};

use crate::web::paths;

pub fn is_installed() -> Result<bool> {
    Ok(paths::cli_js_path()?.exists())
}

/// Checks that `node`/`npm` are both on `PATH`, with an actionable error naming exactly what's
/// missing rather than surfacing npm's own much less clear failure mode.
fn require_prereqs() -> Result<()> {
    for bin in ["node", "npm"] {
        let found =
            Command::new(bin).arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok();
        if !found {
            bail!("`{bin}` isn't on PATH — install Node.js >= 20 from https://nodejs.org, then re-run this command");
        }
    }
    Ok(())
}

/// Installs git-task-web via `npm install --prefix <install_dir> git-task-web@latest`.
///
/// In text mode, npm's own progress is streamed straight to the terminal (inherited stdio) —
/// this is a foreground, user-invoked, one-time setup step, unlike `sync`'s fully-silent
/// background worker. In `--format json` mode that same output would land on the same stdout as
/// the JSON envelope and corrupt it, so it's captured to `web.log` instead.
pub fn install() -> Result<()> {
    require_prereqs()?;
    let dir = paths::install_dir()?;
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;

    let mut cmd = Command::new("npm");
    cmd.args(["install", "--prefix"]).arg(&dir).arg("git-task-web@latest");

    let status = if crate::output::is_json() {
        let log_path = paths::log_path()?;
        let log_out = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .with_context(|| format!("opening {}", log_path.display()))?;
        let log_err = log_out.try_clone().with_context(|| format!("opening {}", log_path.display()))?;
        cmd.stdout(log_out).stderr(log_err).status()
    } else {
        cmd.status()
    }
    .context("running `npm install`")?;

    if !status.success() {
        bail!("`npm install` failed (exit code {:?}) — see output above", status.code());
    }

    let cli_js = paths::cli_js_path()?;
    if !cli_js.exists() {
        bail!(
            "npm install succeeded but {} is missing — git-task-web's package layout may have changed",
            cli_js.display()
        );
    }

    Ok(())
}
