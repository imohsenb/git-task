# git-task

A git-native task manager. Tasks live inside the repo as git objects under `refs/tasks/*` —
no working-tree files, full history, push/pull like any other ref. A user-level config lets
you register repos across machines and group them by project, so `git task ls` can show tasks
across every repo you work in without `cd`-ing into each one.

See [PLAN.md](PLAN.md) for the full design (storage model, workflow, automation, sync).

## Status

Phase 1 (skeleton) is implemented: CLI dispatch, user-level config, and cross-repo
registration. Task storage itself (`new`, `show`, `edit`, `status`, `comment`, …) lands in
the next phase.

## Install

```sh
cargo install --locked --path .
```

This puts `git-task` on your `PATH`, which makes `git task ...` work as a git subcommand.

## Usage

```sh
# inside a repo
git task register                  # register under the repo dir name, default project "main"
git task register --project web    # register under an explicit project

git task repos                     # list all registered repos
git task projects                  # list projects and the repos grouped under them
git task unregister <name>         # remove a registration
```

## Configuration

User-level config lives at `~/.config/git-task/config.toml` on both Linux and macOS
(deliberately XDG-style on macOS too, not `~/Library/Application Support`).

Resolution order:

1. `$GIT_TASK_CONFIG_DIR` (explicit override)
2. `$XDG_CONFIG_HOME/git-task`
3. `~/.config/git-task`

```toml
default_project = "main"

[repos.server]
path = "/abs/path/to/server"
project = "backend"
```

## Roadmap

- Core task store: event-sourced operations under `refs/tasks/<id>`, `new`/`show`/`edit`/
  `status`/`comment`/`log`
- Cross-repo `ls` with filters
- Epics, links, milestones
- Automation rules (global + per-project)
- `push`/`pull` sync with merge
