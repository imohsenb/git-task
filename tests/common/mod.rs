// Each integration test file compiles this module as its own separate crate, so
// whatever any single file doesn't use looks "dead" to that compilation even though
// a sibling file does use it.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;

use assert_cmd::Command;
use tempfile::TempDir;

enum ConfigDir {
    Owned(TempDir),
    Shared(PathBuf),
}

impl ConfigDir {
    fn path(&self) -> &Path {
        match self {
            ConfigDir::Owned(dir) => dir.path(),
            ConfigDir::Shared(path) => path,
        }
    }
}

/// A throwaway git repo + isolated `GIT_TASK_CONFIG_DIR`, wired so every `cmd()`
/// invocation runs the real compiled `git-task` binary as a subprocess with its
/// own env — no global state, safe under parallel test execution.
pub struct TestRepo {
    dir: TempDir,
    config_dir: ConfigDir,
}

impl TestRepo {
    pub fn new() -> Self {
        Self::init(ConfigDir::Owned(tempfile::tempdir().expect("tempdir")))
    }

    /// Like `new`, but points at a config dir shared with other `TestRepo`s — needed for
    /// cross-repo registration tests, where "register" has to land in one common config.
    pub fn new_with_shared_config(config_dir: &Path) -> Self {
        Self::init(ConfigDir::Shared(config_dir.to_path_buf()))
    }

    fn init(config_dir: ConfigDir) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = Self { dir, config_dir };
        repo.git(&["init", "-q"]);
        repo.git(&["config", "user.name", "Test User"]);
        repo.git(&["config", "user.email", "test@example.com"]);
        repo
    }

    /// A repo cloned from `bare`, with its own identity — for multi-clone sync tests.
    pub fn clone_from(bare: &Path, user_name: &str) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let config_dir = ConfigDir::Owned(tempfile::tempdir().expect("tempdir"));
        let status = StdCommand::new("git")
            .args(["clone", "-q"])
            .arg(bare)
            .arg(dir.path())
            .status()
            .expect("git clone");
        assert!(status.success(), "git clone failed");
        let repo = Self { dir, config_dir };
        repo.git(&["config", "user.name", user_name]);
        repo.git(&["config", "user.email", &format!("{user_name}@example.com")]);
        repo
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    pub fn git(&self, args: &[&str]) {
        let status = StdCommand::new("git")
            .args(args)
            .current_dir(self.path())
            .status()
            .expect("running git");
        assert!(status.success(), "git {args:?} failed");
    }

    /// A ready-to-configure `git-task` invocation in this repo, with an isolated config dir.
    /// `auto-sync` is disabled by default (see `GIT_TASK_DISABLE_AUTO_SYNC` below) — use
    /// `cmd_with_auto_sync()` for tests that specifically exercise it.
    pub fn cmd(&self) -> Command {
        let mut cmd = Command::cargo_bin("git-task").expect("git-task binary");
        cmd.current_dir(self.path());
        cmd.env("GIT_TASK_CONFIG_DIR", self.config_dir.path());
        cmd.env("GIT_TASK_DISABLE_AUTO_SYNC", "1");
        cmd
    }

    /// Like `cmd()`, but with the `auto-sync` built-in left enabled — for tests that need it to
    /// actually spawn its background worker (`sync::trigger`/`sync::worker`).
    pub fn cmd_with_auto_sync(&self) -> Command {
        let mut cmd = self.cmd();
        cmd.env_remove("GIT_TASK_DISABLE_AUTO_SYNC");
        cmd
    }

    /// A `git-task` invocation NOT bound to this repo's directory — for cross-repo `ls`,
    /// which is meant to be run from anywhere once repos are registered.
    pub fn cmd_from(&self, cwd: &Path) -> Command {
        let mut cmd = Command::cargo_bin("git-task").expect("git-task binary");
        cmd.current_dir(cwd);
        cmd.env("GIT_TASK_CONFIG_DIR", self.config_dir.path());
        cmd.env("GIT_TASK_DISABLE_AUTO_SYNC", "1");
        cmd
    }

    /// Like `cmd()`, but with git's global/system config hidden — `HOME` points at a
    /// directory that doesn't exist (no `.gitconfig` to find) and `GIT_CONFIG_SYSTEM` is
    /// redirected to `/dev/null` — so a test can reliably observe "no git identity configured
    /// at all" regardless of whatever's actually set up on the machine running the tests.
    pub fn cmd_no_global_identity(&self) -> Command {
        let mut cmd = self.cmd();
        cmd.env("HOME", "/nonexistent-git-task-test-home");
        cmd.env("GIT_CONFIG_SYSTEM", "/dev/null");
        cmd.env("GIT_CONFIG_GLOBAL", "/nonexistent-git-task-test-home/.gitconfig");
        cmd
    }

    /// Runs a `git-task` command that's expected to fail under `--format json` and returns the
    /// parsed stdout document — unlike `run_err` (which returns stderr and is for text-mode
    /// failures), a JSON failure's payload is on stdout, and the process still exits 1.
    pub fn run_err_json(&self, args: &[&str]) -> serde_json::Value {
        let output = self.cmd().args(args).output().expect("running git-task");
        assert!(!output.status.success(), "git-task {args:?} unexpectedly succeeded");
        let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("stdout was not exactly one JSON document: {e}\n{stdout}"))
    }

    /// Runs a `git-task` command and returns stdout, panicking (with stderr) on failure.
    pub fn run(&self, args: &[&str]) -> String {
        let output = self.cmd().args(args).output().expect("running git-task");
        assert!(
            output.status.success(),
            "git-task {args:?} failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        String::from_utf8(output.stdout).expect("utf8 stdout")
    }

    /// Runs a `git-task` command expected to fail, returning stderr.
    pub fn run_err(&self, args: &[&str]) -> String {
        let output = self.cmd().args(args).output().expect("running git-task");
        assert!(!output.status.success(), "git-task {args:?} unexpectedly succeeded");
        String::from_utf8(output.stderr).expect("utf8 stderr")
    }

    /// Extracts the `KEY-hash` address from a line like "created SRV-abc123 — Title".
    pub fn extract_id(output: &str) -> String {
        output
            .split_whitespace()
            .find(|tok| match tok.split_once('-') {
                Some((prefix, suffix)) => {
                    !prefix.is_empty()
                        && prefix.chars().all(|c| c.is_ascii_alphanumeric())
                        && !suffix.is_empty()
                        && suffix.chars().all(|c| c.is_ascii_hexdigit())
                }
                None => false,
            })
            .unwrap_or_else(|| panic!("no id found in output: {output:?}"))
            .to_string()
    }
}

/// Runs `git-task` from an arbitrary directory that need not be a git repo (or even exist
/// yet) — needed for `clone`, which creates its own target directory as part of the command.
pub fn run_in(cwd: &Path, config_dir: &Path, args: &[&str]) -> String {
    let mut cmd = Command::cargo_bin("git-task").expect("git-task binary");
    cmd.current_dir(cwd);
    cmd.env("GIT_TASK_CONFIG_DIR", config_dir);
    cmd.env("GIT_TASK_DISABLE_AUTO_SYNC", "1");
    let output = cmd.args(args).output().expect("running git-task");
    assert!(
        output.status.success(),
        "git-task {args:?} failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    String::from_utf8(output.stdout).expect("utf8 stdout")
}
