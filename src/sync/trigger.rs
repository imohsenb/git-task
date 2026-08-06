use std::process::Stdio;

use git2::Repository;

use crate::automation::builtins;
use crate::config::automation_toggle;
use crate::config::global::GlobalConfig;
use crate::config::project::ProjectConfig;

/// Set (any value) to suppress `auto-sync` entirely — checked first, before any lock/spawn
/// logic. Set by default in the integration test harness (`tests/common::TestRepo::cmd`) so
/// tests never spawn real background processes; tests that specifically exercise auto-sync use
/// `cmd_with_auto_sync()` to opt back in.
const DISABLE_ENV: &str = "GIT_TASK_DISABLE_AUTO_SYNC";

/// Fires the `auto-sync` built-in: hands off to a detached background worker process and
/// returns immediately, never blocking the caller on the network. No feedback of any kind is
/// ever produced — disabled, no git repo, no remote, network failure, and success all look
/// identical to the invoking command (nothing printed, exit code unaffected), per the confirmed
/// design (see `automation::builtins::AUTO_SYNC`).
pub fn trigger(repo: &Repository) {
    if std::env::var_os(DISABLE_ENV).is_some() {
        return;
    }

    let global = GlobalConfig::load().map(|c| c.automation).unwrap_or_default();
    let project = ProjectConfig::load(repo).map(|c| c.automation).unwrap_or_default();
    if !automation_toggle::resolve_enabled(builtins::AUTO_SYNC, &global, &project) {
        return;
    }

    let Ok(exe) = std::env::current_exe() else { return };
    let git_dir = repo.path().to_path_buf();

    // Deliberately not `.wait()`'d — a spawned child is reparented (not killed) when this
    // process exits, so it keeps running after the triggering CLI command returns. Stdio is
    // fully discarded so it can never interleave with (or leak into) the parent's own output.
    let _ = std::process::Command::new(exe)
        .arg("__sync-worker")
        .arg(&git_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}
