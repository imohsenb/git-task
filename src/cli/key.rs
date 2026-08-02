use anyhow::Result;

// Alias for `git task config key` — kept for muscle memory. The address key now lives in the
// event-sourced config ref (`refs/tasks/config`), edited via `crate::cli::config`.
pub use crate::cli::config::KeyArgs;

pub fn run(args: KeyArgs) -> Result<()> {
    crate::cli::config::run_key(args)
}
