# git-task

A git-native task manager. Tasks live inside the repo as git objects under `refs/tasks/*` —
event-sourced operations, no working-tree files, full history, push/pull like any other ref.
A user-level config lets you register repos across machines and group them by project, so
`git task ls` can show tasks across every repo you work in without `cd`-ing into each one.

See [PLAN.md](PLAN.md) for the full design (storage model, workflow, automation, sync).

## Status

Core task store and cross-repo registration are implemented. Cross-repo `ls` (aggregating
across registered repos, not just the current one) is next.

## Install

```sh
cargo install --locked --path .
```

This installs two binaries from the same codebase:

- **`git-task`** — required exact name for git's subcommand dispatch; makes `git task ...` work.
- **`ght`** — a short alias for using the CLI standalone, without going through git: `ght new "title"`.

Both accept the same commands; pick whichever fits how you're invoking it.

## Usage

```sh
# inside a repo
git task new "Fix login timeout" --kind bug --priority high --label auth
ght new "Write onboarding docs"          # same thing, standalone form

git task show SRV-9057e58a               # KEY-hash address (see below)...
git task show 9057e58a                   #  ...or a bare hash prefix, same result
git task show SRV-9057e58a --format md   # or --format json

git task status SRV-9057e58a doing       # free-form status, no workflow lock-in
git task comment SRV-9057e58a "found the root cause"
git task comment SRV-9057e58a --edit 1 "revised note"
git task edit SRV-9057e58a --priority critical --assignee alice
git task log SRV-9057e58a                # full audit trail
git task ls --status doing --kind bug    # current repo, with filters
git task export --all --format md        # dump every task in the repo

# repo identity and required fields
git task key                             # show the repo's address key (derived or pinned)
git task key SRV                         # pin it, written to .gittask/config.toml (tracked)
git task fields                          # effective required-field schema (global + project)

# cross-repo registration
git task register                        # register under the repo dir name, default project "main"
git task register --project web          # register under an explicit project
git task repos                           # list all registered repos
git task projects                        # list projects and the repos grouped under them
git task unregister <name>                # remove a registration
```

If a required field (title/description always; others per config, see below) is missing and
you're at a terminal, `git task new` prompts for it. Piped or scripted (no TTY), it fails fast
listing what's missing instead of hanging.

## Addressing

Every task's real identity is the git object hash under `refs/tasks/<id>`. `KEY-<hash prefix>`
(e.g. `SRV-9057e58a`) is a readable display form — the `KEY-` part is stripped before lookup, so
it's purely cosmetic, never required, and a bare hash prefix always still works.

## Configuration

Two layers:

- **User-level** (personal, not shared) at `~/.config/git-task/config.toml` — registered repos,
  their project grouping, and global default field requirements.
- **Per-repo** (git-tracked, shared with every clone) at `.gittask/config.toml` — the repo's
  address key and field requirements that override the global defaults for this repo.

User-level config dir resolution order, same on Linux and macOS (deliberately XDG-style on
macOS too, not `~/Library/Application Support`):

1. `$GIT_TASK_CONFIG_DIR` (explicit override)
2. `$XDG_CONFIG_HOME/git-task`
3. `~/.config/git-task`

```toml
# ~/.config/git-task/config.toml
default_project = "main"

[repos.server]
path = "/abs/path/to/server"
project = "backend"

[fields.priority]
required = true
```

```toml
# .gittask/config.toml (in the repo, tracked in git)
key = "SRV"

[fields.priority]
required = false   # overrides the global default above, for this repo only
```

Only `priority`, `assignee`, and `due` are configurable as required; `title` and `description`
are always required.

## Roadmap

- Cross-repo `ls` (aggregating tasks across every registered repo, not just the current one)
- Epics, links, milestones
- Automation rules (global + per-project)
- `push`/`pull` sync with merge
