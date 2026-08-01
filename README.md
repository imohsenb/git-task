# git-task

A git-native task manager. Tasks live inside the repo as git objects under `refs/tasks/*` —
event-sourced operations, no working-tree files, full history, push/pull like any other ref.
A user-level config lets you register repos across machines and group them by project, so
`git task ls` can show tasks across every repo you work in without `cd`-ing into each one.

See [PLAN.md](PLAN.md) for the full design (storage model, workflow, automation, sync).

## Status

Feature-complete for v1: core task store, cross-repo registration/`ls`, epics/links/milestones,
automation rules, and `push`/`pull` sync are all implemented (see [PLAN.md](PLAN.md)). Remaining
work is polish — shell completions, a proper test suite.

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
git task label SRV-9057e58a add urgent
git task label SRV-9057e58a rm urgent
git task log SRV-9057e58a                # full audit trail
git task export --all --format md        # dump every task in the repo

# epics, links, milestones
git task new "Design new UI" --kind story --parent SRV-epic --milestone v2.0
git task epic SRV-epic add SRV-child      # make a task a child of an epic
git task epic SRV-epic rm SRV-child       # remove it again
git task link SRV-1 add blocks SRV-2      # blocks | relates | dup
git task link SRV-1 rm blocks SRV-2
git task ls --parent SRV-epic             # list an epic's children

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

# listing — from anywhere, no cd required
git task ls                              # aggregates every registered repo (falls back to the
                                          #   current repo if nothing is registered yet)
git task ls --project backend            # only repos in one project group
git task ls --repo web                   # only one named repo
git task ls --here                       # only the current repo, ignoring the registry
git task ls --status doing --kind bug --mine   # filters compose; --mine matches your git identity

# automation
git task automation list                 # effective global + per-repo rules

# sync
git task push                            # push refs/tasks/* to "origin" (or a named remote)
git task pull                            # fetch + reconcile: new / fast-forward / real merge
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

# [[rule]] entries must come after key/[fields.*] — see Automation below
[[rule]]
name = "auto-triage-bugs"
on = "task.created"
when = "kind == \"bug\""
do = ["set_priority high", "add_label triage"]
```

Only `priority`, `assignee`, and `due` are configurable as required; `title` and `description`
are always required.

## Automation

Rules run after every mutation (`new`, `edit`, `status`, `comment`, `label`, `epic`, `link`).
Global rules (personal, apply to every repo) live in `~/.config/git-task/automation.toml`;
per-repo rules (shared, git-tracked) are `[[rule]]` entries in `.gittask/config.toml` — put them
**after** `key`/`[fields.*]` in the file, since an empty `rule`/`fields` section is omitted when
git-task itself writes the file, and a later `[[rule]]` block can't redefine a key TOML already
saw as `rule = []`.

```toml
[[rule]]
name = "auto-triage-bugs"
on   = "task.created"     # task.created | status.changed | comment.added | label.added | task.updated
when = "kind == \"bug\""  # evalexpr, against kind/status/priority/assignee/title (strings,
                           #   empty string if unset); omit `when` for an unconditional rule
do   = ["set_priority high", "add_label triage"]
                           # set_priority/status/assignee/kind/due/milestone <value>,
                           # add_label/remove_label <value>, add_comment "text"
```

Actions run as their own git-task-automation-attributed op-package (visible in `git task log`).
A rule can fire at most once per command — its own generated ops can cascade into other rules
(e.g. an action's `set_status` re-triggers `status.changed`), but never back into itself, and a
misconfigured `when`/action is skipped with a warning rather than blocking the command.

## Sync

`git task push [remote]` and `git task pull [remote]` (default `origin`) move `refs/tasks/*`
to/from a normal git remote — no dedicated task server, no separate remote required. Since these
use explicit refspecs, a plain `git fetch`/`git pull`/`git push` never touches task refs at all.

Pull reconciles each task independently: a task new to you is created outright, one where the
remote is strictly ahead fast-forwards, and one that was edited on both sides gets a real
two-parent git merge commit (carrying no changes of its own — the two branches' full histories are
still there). Reading a task always walks its *entire* reachable history and re-derives the state
by sorting every operation by timestamp, so which side happened to perform the merge doesn't
matter — everyone converges on the same result. If your side has diverged from the remote,
`git task push` is rejected (same as a non-fast-forward branch push) — run `git task pull` first.

## Roadmap

- Shell completions, integration test suite
