# git-task — git-native task manager (Rust)

## Context

Greenfield repo (`git-task`, only a README). Goal: a **git subcommand** `git task …` that manages Jira-like
tasks stored **inside the repo as git objects** under `refs/tasks/*` — so tasks travel with the repo, are
push/pull-able, versioned, and never touch the working tree. A **user-level config** registers repos and groups
them by **project**, enabling **cross-repo** task listing from anywhere (`git task ls` without `cd`-ing into a
repo). Supports **automation rules** (global + per-project) and Jira-style workflow, comments/history, and
epics/links.

Prior art that validates the model: **git-bug** and **git-appraise** store entities as git objects under a
custom ref namespace. We follow git-bug's **event-sourced operation-chain** model.

### Decisions (confirmed with user)
- **Language:** Rust (single `git-task` binary → git auto-dispatches `git task`). No Go installed.
- **Storage:** Event-sourced op-chain under `refs/tasks/<id>` (distributed, mergeable, full audit trail).
- **v1 scope:** Core+workflow · Comments+history · Epics/links/sprints. *Deferred:* JQL query language, kanban board, auto-sync hooks, attachments, web UI.
- **Sync:** Manual `git task push/pull` in v1.
- Baseline from the original ask (not optional): git-subcommand packaging, user-level registration, cross-repo `ls`, project grouping, automation engine.

## Architecture

### Storage: event-sourced ops under `refs/tasks/<id>`
- **All data lives inside `.git`** — refs under `.git/refs/tasks/*` (or packed-refs), op contents in
  `.git/objects`. The working tree stays clean; nothing is checked out. (Plain files under `.git/tasks/`
  were rejected: not pushable, not cloned, no history/merge.)
- Each task = an **entity** identified by an `Id` = hash of its creation op-package.
- `refs/tasks/<id>` points to a **chain of commits**; each commit = one op-package (the ops produced by a single
  command). Commit tree holds `ops.json` (array of operations) + optional media. Parent = previous op-package.
- **State is derived** by folding the ordered op-chain into a `Task` (fold = replay). Comments and the audit
  trail fall out for free: comments are `AddComment` ops; history is the op log itself.
- **Operations** (`serde`-tagged enum): `CreateTask`, `SetTitle`, `SetDescription`, `SetKind`, `SetStatus`,
  `SetPriority`, `SetAssignee`, `AddLabel`/`RemoveLabel`, `AddComment`/`EditComment`, `SetParent` (epic/subtask),
  `AddLink`/`RemoveLink` (blocks/relates/duplicates), `SetMilestone`, `SetDueDate`. Every op carries
  `author` (from git `user.name`/`user.email`) + unix `timestamp`.
- **Merge** (needed for pull): union the op-commits from both sides, re-fold with deterministic order
  (timestamp, then op-hash tiebreak). Scalar fields → last-writer-wins; labels/comments/links → union/append.

### Domain model (derived `Task`)
`id, title, description, kind (Bug|Story|Task|Epic|Subtask), status, priority, assignee, reporter, labels,
parent, links, milestone, due, comments, created, updated, history`.

**Workflow (fully flexible)**: states are user-defined per project — any names, any count. Transitions are
optional: define allowed edges to enforce a workflow, or omit them for free-form (any→any). A default
(`todo → doing → review → done`, plus `blocked`) ships only as a starting point. `git task status` enforces
transitions when defined.

### Addressing
Short-hash prefix like git SHAs (`git task show 3f2a`). Optional per-repo sequential alias `#N` derived from
creation order (nicety; may land in a later phase).

### Config & registration (user-level)
- Dir: `${XDG_CONFIG_HOME:-~/.config}/git-task/`, overridable via env `GIT_TASK_CONFIG_DIR` (used by tests to
  avoid touching real config). Use the `directories`/`etcetera` crate + env override.
- `config.toml` — registered repos + project grouping:
  ```toml
  default_project = "main"          # used when register omits --project
  [repos.server] path = "/abs/server"  project = "backend"
  [repos.web]    path = "/abs/web"     project = "backend"
  [repos.mktg]   path = "/abs/mktg"    project = "growth"
  ```
- `git task register [name] [--project P]` (run in a repo) records abs path under `name`
  (default = repo dir name), grouped by `P` (**default project = `main`**). Plus `unregister`, `repos`, `projects`.
- **Config format = TOML** (matches git/Cargo, no whitespace/`Norway` pitfalls, explicit types). `serde`
  supports YAML too — swap is a one-liner if nested rules justify it later.

### Cross-repo listing
- `git task ls` (anywhere) → **aggregate across all registered repos**, annotated by repo + project.
- Modifiers: `--here`/`--local` (current repo only), `--repo NAME`, `--project P`, and basic filters
  `--status --assignee --label --kind --mine`. (Not a full query language — just flags.)
- Aggregation opens each registered repo via `git2`, reads `refs/tasks/*`, folds, filters, prints a table.

### Automation engine (global + per-project)
- **Global** personal rules: `…/git-task/automation.toml`. **Per-project** shared rules: committed
  `.gittask/config.toml` in the repo (workflow + rules). *(Alternative — a pushable `refs/tasks-config` ref —
  is a follow-up.)*
- Rule schema — event / condition / actions:
  ```toml
  [[rule]]
  name = "auto-triage-bugs"
  on   = "task.created"          # task.created|status.changed|comment.added|label.added|task.updated
  when = "kind == 'bug'"          # condition via `evalexpr`
  do   = ["set_priority high", "add_label triage"]
  ```
- Engine runs after each mutation; **actions emit ops** (attributed to an `automation` actor) folded into the
  same/next op-package. Loop guard: a rule never re-fires from its own generated ops; cap iterations.

### Sync (manual, v1)
- `git task push [remote]` → refspec `refs/tasks/*:refs/tasks/*`.
- `git task pull [remote]` → fetch into `refs/remote-tasks/<remote>/*`, then per-task **union/LWW merge** into
  local `refs/tasks/*`. Start with fast-forward + union merge; document conflict edge cases.
- Support a dedicated tasks remote so tasks don't clutter normal fetches.

### Packaging as a git subcommand
Binary `git-task`; `cargo install --path .` → `~/.cargo/bin/git-task` on PATH → `git task …` works. Ship
`git task completions <shell>` (clap-generated) and `--help/--version`.
**Platforms:** Linux + macOS (Rust + `git2`/libgit2 are cross-platform; XDG config path works on both).
Windows is a later target.

## Crates
`clap` (derive+completions) · `git2` (mature libgit2 bindings: objects, refs, transactions, push/pull) ·
`serde`+`serde_json` (ops) · `toml` (config) · `directories` (config dir) · `time` or `chrono` ·
`anyhow`+`thiserror` · `evalexpr` (rule conditions) · `comfy-table`/`tabled` (ls output).
*(Note: `gix` is pure-Rust but its write/push API is less settled than `git2`; prefer `git2` for v1.)*

## Project layout
```
Cargo.toml
src/
  main.rs                 # clap dispatch
  cli/                    # one handler per subcommand: new, ls, show, edit, status,
                          #   comment, label, link, epic, log, register, repos, projects,
                          #   push, pull, automation, config, completions
  domain/  task.rs op.rs fold.rs id.rs      # model, Operation enum, replay/fold, id prefix
  store/   mod.rs git_store.rs merge.rs     # git2-backed refs/tasks store + union/LWW merge
  config/  global.rs project.rs             # user config (repos/projects) + project config
  automation/ engine.rs rules.rs            # event dispatch, evalexpr, actions, loop guard
  git/     repo.rs                          # open/discover repo, remotes, push/pull refspecs
  workflow.rs actor.rs
tests/                                       # integration (temp repos + temp config)
```

## Command surface (v1)
```
git task new "title" [--kind bug] [--desc …] [--assignee me] [--label x] [--priority high] [--parent EPIC] [--milestone m1] [--due DATE]
git task ls [--here|--repo NAME|--project P] [--status s --assignee a --label l --kind k --mine]
git task show <id> [--format text|md|json]   git task export [<id>|--all] --format md|json   git task log <id>
git task edit <id> [--title|--desc|--priority|--assignee|--kind|--milestone|--due …]
git task status <id> <state>  git task comment <id> "…"    git task label <id> add|rm <label>
git task link <id> blocks|relates|dup <other>              git task epic <id> add|rm <child>
git task register [name] [--project P]   git task unregister <name>   git task repos   git task projects
git task push [remote]        git task pull [remote]
git task automation list|test|run         git task config …           git task completions <shell>
```

## Implementation phases
1. **Skeleton** — cargo project, clap dispatch, `git2` open/discover, `actor` from git config, config dir + env
   override. Milestone: `git task --version`, `git task register`, `git task repos`.
2. **Core store** — Operation enum, `fold`, `git_store` read/write op-chain under `refs/tasks/<id>`. Commands:
   `new, show, ls --here, edit, status, comment, log`. Default workflow.
3. **Cross-repo** — global config repos/projects, aggregate `ls` + filters, `register/unregister/repos/projects`.
4. **Epics/links/sprints** — `SetParent`, `AddLink/RemoveLink`, milestones; `epic`, `link` commands.
5. **Automation** — engine (global + `.gittask/config.toml`), `evalexpr` conditions, actions→ops, loop guard.
6. **Sync** — `push`/`pull` refspecs + per-task union/LWW `merge`.
7. **Polish** — completions, table output, docs (README usage), tests.

## Verification
- **Unit:** fold correctness (create+edits → expected `Task`); merge union+LWW; id-prefix resolution; condition eval.
- **Integration** (`tests/`): create temp git repos in a `tempdir`, point `GIT_TASK_CONFIG_DIR` at a temp dir,
  run commands via the binary, assert `refs/tasks/*` created; register two temp repos and assert cross-repo `ls`
  aggregates both; push to a bare remote, clone elsewhere, `pull`, assert merged state.
- **Manual e2e:** `cargo install --path .`; in a repo `git task new` / `ls` / `status` / `comment`; register a
  2nd repo, `git task ls` cross-repo; add a `[[rule]]` and confirm it fires; `git task push` to a bare remote,
  clone, `git task pull`, verify tasks + a concurrent-edit merge resolve correctly.
