//! Background-process machinery for `git task web` (install, spawn/stop/status of the companion
//! git-task-web server) — kept out of `automation`/`sync` because it isn't triggered by any
//! event or op batch, just directly invoked by `cli::web`. Repo-agnostic: unlike `sync`'s
//! per-repo worker, this manages a single server per machine, so its state lives under
//! `config::global::data_dir()` rather than any repo's `<git-dir>/git-task/`.

pub mod install;
pub mod paths;
pub mod process;
pub mod update;
