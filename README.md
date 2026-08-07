# git-task

![git-task](assets/banner.png)

A git-native task manager. Tasks live inside the repo as git objects under `refs/tasks/*` —
event-sourced, no working-tree files, full history, synced with `push`/`pull` like any other ref.

## Get Started

**Homebrew** (macOS arm64 / Apple Silicon, Linux arm64+x86_64):

```sh
brew tap imohsenb/tap
brew install git-task
```

**From source** (fallback — any platform, requires a Rust toolchain):

```sh
git clone https://github.com/<you>/git-task && cd git-task && ./install.sh
# or: cargo install --locked --path .
```

Both install two binaries from the same codebase — `git-task` (so `git task ...` works) and
`gtask` (a standalone alias, e.g. `gtask new "title"`). Pick whichever fits how you invoke it.

Then, inside any repo:

```sh
git task new "Fix login timeout" --kind bug --priority high
git task ls
git task show <id>                # id is printed by `new`/`ls`, e.g. SRV-9057e58a
git task status <id> doing
git task comment <id> "found the root cause"
```

That's the core loop. Everything below is reference for when you need more.

## Common commands

```sh
git task new "title" --kind bug --priority high --label auth --parent SRV-epic
git task show <id>                       # boxed detail view
git task show <id> --markdown            # markdown instead
git task show <id> --format json         # single JSON document (see "JSON output" below)
git task edit <id> --priority critical --assignee alice@example.com
git task edit <id>                       # no flags: interactive prompts
git task label <id> add urgent
git task label <id> rm urgent
git task log <id>                        # full audit trail
git task delete <id>                     # soft delete (syncs, hidden from ls by default)
git task export --all --format md        # dump every task
git task whoami                          # identity a write would be attributed to
```

`<id>` accepts either the display form (`SRV-9057e58a`) or a bare hash prefix (`9057e58a`) — the
`KEY-` part is cosmetic and stripped before lookup.

## Epics and links

```sh
git task epic SRV-epic add SRV-child          # make a task a child of an epic
git task epic SRV-epic rm SRV-child
git task link SRV-1 add blocks SRV-2          # blocks | relates | dup
git task link SRV-1 rm blocks SRV-2
git task ls --parent SRV-epic                 # an epic's same-repo children
git task show SRV-epic                        # full detail, incl. every child
```

Both also work across repos with `--repo <name|path|url>` (e.g.
`git task link SRV-1 add blocks LB-abc123 --repo backend`) — the target repo doesn't need to
share git history. `epic --repo` additionally requires both repos registered under the same
project (see below) and opens the target to confirm the epic exists; `link --repo` just records
the reference without validating it (except a URL, stored verbatim).

## Working across repos

Register repos so `git task ls` aggregates across all of them without `cd`-ing:

```sh
git task register                        # default project "main"
git task register --project web
git task repos                           # list registered repos
git task projects                        # repos grouped by project
git task ls                              # aggregates every registered repo
git task ls --project backend            # only one project
git task ls --mine --status doing        # filters compose
```

## Sync

```sh
git task clone <url> [dir]               # tasks only, no source checkout
git task push                            # push refs/tasks/* to "origin" (or a named remote)
git task pull                            # fetch + reconcile
```

`pull` handles each task independently: new tasks are created, remote-ahead tasks fast-forward,
and tasks edited on both sides get a real two-parent merge commit — no data is lost, everyone
converges on the same result regardless of which side merged. If your side has diverged, `push`
is rejected until you `pull` first, same as a normal git branch.

`git task clone` fetches only `refs/tasks/*` into a fresh repo — handy for a PM or stakeholder who
just wants to read/triage tasks without the source checkout. It sets up `origin`, so `push`/`pull`
work right away.

## Deleting tasks

- **`git task delete <id>`** — soft delete. Recorded as an event, syncs to every peer on their
  next `pull`. `ls` hides deleted tasks by default (`--deleted` to include them). No `restore`.
- **`git task drop <id> --force`** — hard delete. Removes the local ref outright, no history
  entry, local-only (a peer's `pull` brings the task right back). Add `--remote [name]` to also
  delete it on that remote.

## Configuration

Two layers:

- **User-level** (personal, not shared): `~/.config/git-task/config.toml` — registered repos,
  their project grouping, default field requirements.
- **Per-repo** (shared, travels with every clone): event-sourced under `refs/tasks/config`, edited
  only via `git task config`, never by hand.

```sh
git task config show                     # key, resolved required fields, rules
git task config key SRV                  # pin the address key
git task config field priority required  # this repo only; also: assignee, due
```

## Automation

Rules run after every mutation and can auto-set fields, add labels, or comment based on
conditions:

```sh
git task config rule add \
  --name auto-triage-bugs \
  --on task.created \
  --when 'kind == "bug"' \
  --do 'set_priority high' --do 'add_label triage'

git task config rule add --global   # save to ~/.config/git-task/automation.toml instead
git task config rule list           # global + per-repo, resolved
```

`--on`: `task.created | status.changed | comment.added | label.added | task.updated`. `--when` is
an evalexpr condition over `kind`/`status`/`priority`/`assignee`/`title`. `--do` (repeatable):
`set_<field> <value>`, `add_label`/`remove_label <value>`, `add_comment "text"`, and similar for
versions. A rule fires at most once per command; its ops can cascade into other rules but never
back into itself.

## JSON output

Every command accepts `--format json`. Output is a single JSON document on stdout — safe to pipe
into `serde_json`/`JSON.parse`/etc:

```jsonc
// success
{ "ok": true, "command": "new", "version": "1.0.0", "data": { /* command-specific */ } }

// failure — still on stdout, process still exits 1
{ "ok": false, "command": "show", "version": "1.0.0",
  "error": { "kind": "not_found", "message": "no task matching 'deadbeef'" } }
```

`error.kind`: `not_a_repo`, `identity_missing`, `not_found`, `ambiguous_id`, `validation`,
`conflict`, `rejected`, `remote`, `io`, `internal`. Tasks in JSON responses are enriched with
`display_id`/`key` and resolved `*_name` fields (`assignee_name`, etc.) beyond the plain domain
model.

## Agent skills

`git task skills install` teaches a coding agent (e.g. Claude Code) this CLI — addressing,
`--format json`, task CRUD, config/automation, cross-repo sync — from `SKILL.md` files embedded in
the binary:

```sh
git task skills install            # best-effort install into known agent dirs (e.g. ~/.claude/skills)
git task skills install --project  # + this repo's .claude/skills, so it travels with the repo via git
```

## Development

```sh
cargo build
cargo test                              # unit + integration tests (real temp repos, real binary)
git task completions bash > ...         # also: zsh, fish, powershell, elvish
```

`git task --help` (bare) is intercepted by git itself and runs `man git-task` — without a man page
installed that just prints an error. `git task -h` / `git task help` / `git task <cmd> --help` all
work regardless. Run `git task man --install` once to fix the bare form too.
