use std::path::PathBuf;

use anyhow::Result;

use crate::config::global;

/// Where git-task-web gets installed (`npm install --prefix`), under the shared global data
/// directory.
pub fn install_dir() -> Result<PathBuf> {
    Ok(global::data_dir()?.join("web"))
}

/// The installed entrypoint npm lands the package's `bin` target at. Existence of this file *is*
/// "is it installed" — checked fresh each call rather than tracked as separate state that could
/// go stale.
pub fn cli_js_path() -> Result<PathBuf> {
    Ok(install_dir()?.join("node_modules").join("git-task-web").join("dist").join("server").join("cli.js"))
}

/// `<pid> <host> <port>` of the running server, written by `start`, read by `stop`/`status`.
pub fn state_path() -> Result<PathBuf> {
    Ok(global::data_dir()?.join("web.state"))
}

/// Combined stdout+stderr of the spawned server (and, in `--format json` mode, of the `npm
/// install` step too — see `install::install`). Kept, unlike the fully-silent `sync` worker,
/// since this is a user-invoked, long-lived, debuggable process.
pub fn log_path() -> Result<PathBuf> {
    Ok(global::data_dir()?.join("web.log"))
}
