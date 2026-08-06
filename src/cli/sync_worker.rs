use std::path::PathBuf;

use clap::Args;

/// Hidden entrypoint for the detached background `auto-sync` worker (`__sync-worker`). Not part
/// of the public CLI surface (`#[command(hide = true)]` in `cli::mod`) — spawned only by
/// `sync::trigger`, never invoked directly by a user or documented anywhere.
#[derive(Args)]
pub struct SyncWorkerArgs {
    /// This repo's `.git` directory, as captured by `sync::trigger` at spawn time.
    git_dir: PathBuf,
}

/// No `Result`, no output: this runs detached from any terminal with nothing waiting on either.
/// See `sync::worker::run_once` for the actual pull/push/lock logic.
pub fn run(args: SyncWorkerArgs) {
    crate::sync::worker::run_once(&args.git_dir);
}
