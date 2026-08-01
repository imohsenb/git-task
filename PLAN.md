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
- **Language:** Rust. Two binaries from one crate: **`git-task`** (exact name required for git's
  subcommand dispatch — `git task …`) and **`ght`** (short alias for standalone use — `ght …`).
  No Go installed.
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
Real identity is a short-hash prefix like git SHAs (`git task show 3f2a`). Display/UX layer adds a
**`KEY-<hash prefix>`** address (e.g. `SRV-9057e58a`) — `KEY` is the repo's configured key
(`.gittask/config.toml`, see below), purely cosmetic. Resolution strips a recognized `KEY-` prefix
(non-hex chars before the dash, hex remainder after) before doing the normal hash-prefix lookup, so
`SRV-9057e58a` and a bare `9057e58a` resolve identically — the key is never validated against the
repo's actual configured key, just recognized by shape. No sequence counter: a stored or derived
`#N` was considered and rejected — a stored counter conflicts across offline clones, and a
derived-from-project-grouping counter would show different numbers to different teammates since
project grouping is personal/user-level config, not shared. Concatenating the existing hash avoids
both problems entirely.

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

### Per-repo config (`.gittask/config.toml`, git-tracked)
Same file the automation section below already earmarked for per-project rules, extended to also carry:
```toml
key = "SRV"            # address key, see Addressing above; derived from repo dir name if unset

[fields.priority]
required = true        # overrides the global default below, for this repo only
```
Tracked in git so every clone/teammate sees the same key and field requirements — unlike the
user-level config, which is personal and would give different people different values.

### Required-field schema (global + per-project)
`title` and `description` are always required — not configurable. `priority`, `assignee`, and `due`
are optional by default; either config layer can mark them `required` via `[fields.<name>] required =
true` (global `config.toml` sets the default, `.gittask/config.toml` overrides per-repo, project wins
on conflict). `git task new` checks the merged requirement set: if run at a TTY, prompts (looping
until non-empty, bails clean on EOF) for whatever's missing; if not a TTY (piped/scripted), fails fast
listing the missing fields instead of hanging — important since this CLI is meant to work inside
automation too. `git task fields` shows the effective merged schema for the current repo.

### Cross-repo listing
- `git task ls` (anywhere) → **aggregate across all registered repos**, annotated by repo + project,
  regardless of cwd. If zero repos are registered, falls back to current-repo-only (so the zero-config
  single-repo workflow from phase 2 keeps working unchanged).
- Modifiers: `--here` (current repo only, ignoring the registry — errors if combined with
  `--repo`/`--project`), `--repo NAME`, `--project P` (both error clearly if nothing matches, rather
  than silently falling back), and filters `--status --assignee --label --kind --mine`. `--mine`
  resolves per-repo against that repo's own git identity (`user.name`/`user.email` — respects a
  local per-repo override, matches either field) rather than a fixed string, and conflicts with
  `--assignee`. (Not a full query language — just flags.)
- Aggregation opens each registered repo via `git2::Repository::open` (not `discover` — the stored
  path is already the resolved workdir), reads `refs/tasks/*`, folds, filters. A repo that fails to
  open (moved/deleted since registration) is skipped with a warning on stderr, not a hard failure.

### Automation engine (global + per-project) ✅
- **Global** personal rules: `~/.config/git-task/automation.toml`. **Per-project** shared rules:
  `[[rule]]` entries in the same `.gittask/config.toml` used for `key`/`fields` — **must be written
  after `key`/`[fields.*]` in the file**, since `ProjectConfig` skips serializing empty `rule`/`fields`
  (an emitted `rule = []` would collide with a later hand-written `[[rule]]` — TOML forbids
  redefining a key even across the `[]`-literal vs array-of-tables forms).
- Rule schema — event / condition / actions:
  ```toml
  [[rule]]
  name = "auto-triage-bugs"
  on   = "task.created"          # task.created | status.changed | comment.added | label.added | task.updated
  when = "kind == \"bug\""       # evalexpr condition against kind/status/priority/assignee/title
                                  #   (all strings, empty string for unset); omit for always-true
  do   = ["set_priority high", "add_label triage"]
                                  # set_priority/status/assignee/kind/due/milestone, add_label,
                                  # remove_label, add_comment — reuses the existing Operation set,
                                  # no new domain concepts for phase 5
  ```
- CLI commands call `automation::engine::run(repo, task_id, ops_just_written)` right after their own
  `store.create`/`store.append` (Store itself stays a dumb git-plumbing layer with no config/policy
  awareness — automation orchestration lives at the CLI layer, one line added per mutating command:
  `new`, `edit`, `status`, `comment`, `label` (new — see below), `epic`, `link`).
- Events are derived from the ops just written (`events_for`), not hand-declared per command — single
  source of truth shared between the initial trigger and cascaded events from automation's own ops.
- Engine runs a bounded event queue: pop an event, reload the task, evaluate every rule whose `on`
  matches against a fresh `evalexpr` context; a matching rule's actions are parsed to `Operation`s and
  appended as one op-package **attributed to a synthetic `git-task-automation` actor** (visible in
  `log`/`show`), and the ops just written enqueue their own follow-up events (e.g. `set_status` in an
  action re-triggers `status.changed`). **Loop guard:** a rule name can fire at most once per top-level
  command invocation (tracked in a `HashSet`) — this is what "a rule never re-fires from its own
  generated ops" means concretely, since a cascaded event that would re-match an already-fired rule is
  just skipped. A hard cap (20 cascaded events) backstops pathological configs (e.g. two different
  rules cycling each other) with a stderr warning instead of hanging.
- A rule whose `when` fails to parse/evaluate is skipped with a stderr warning, not a hard error —
  a typo in a rarely-used rule shouldn't block `git task new`. Same for an unrecognized action verb.
- `git task label <id> add|rm <label>` was added in this phase — it was in the original v1 command
  surface but never actually built in phase 2 (labels were only settable at creation via `new
  --label`), and the `label.added` event needs it to mean anything for existing tasks.
- `git task automation list` shows the effective global+project rules. `test`/`run` (dry-run a rule,
  retroactively re-run rules over existing tasks) are deliberately deferred — meaningfully more scope
  (simulate-without-writing, or re-evaluate every task in a repo) than "fire on mutation."

### Sync (manual, v1) ✅
Storage had to change to support this: op-chains were a strictly linear parent(0) walk (one parent
per commit), which can't represent two clones that both appended after the same tip — a real fork,
not a fast-forward. `Store::load` now walks the full reachable set via `git2::Revwalk` (dedups
automatically, handles any number of parents) instead of following a single parent pointer, and
flattens+sorts every op by **`(timestamp, commit oid, index within that commit's batch)`** before
folding — a total order that only depends on the shared, content-addressed set of reachable
commits, so it comes out identical no matter which side computed it. This one change is what makes
the rest of sync work with a single code path for both the plain linear case and real merges.

- `git task push [remote]` (default `origin`) — pushes every local task ref, but as **explicit
  `refs/tasks/<id>:refs/tasks/<id>` refspecs per task**, not the glob `refs/tasks/*:refs/tasks/*`
  from the original design: libgit2's push path rejects that glob (`class=Invalid`) even though
  plain `git push` accepts the identical string fine — a libgit2 push-specific quirk, not a real
  refspec problem, confirmed by testing the same string through both. Per-ref rejections (remote
  moved on, needs a `pull` first) are caught via the `push_update_reference` callback, since
  `Remote::push`'s `Result` only reports transport-level failures, not per-ref ones.
- `git task pull [remote]` — fetches `refs/tasks/*:refs/remote-tasks/<remote>/*` (the glob **is**
  fine here; wildcard fetch refspecs are the normal case in git, e.g. the default
  `refs/heads/*:refs/remotes/origin/*`, so this wasn't the same libgit2 quirk as push). For each
  fetched `refs/remote-tasks/<remote>/<id>`, `store/merge.rs::reconcile` picks one of: **new**
  (no local ref yet — create it), **up to date** (equal tips, or local already a descendant),
  **fast-forwarded** (`graph_descendant_of` says remote is strictly ahead — just move the ref), or
  **merged** (genuinely diverged — write a real two-parent merge commit with an **empty** ops.json,
  pure structural join, no content of its own; `load`'s deterministic sort does the actual
  reconciliation work when anyone reads the task afterward).
- No dedicated tasks-remote concept — pushing/fetching `refs/tasks/*` against the same remote as
  code doesn't clutter normal `git fetch`/`git pull`, since those only touch whatever refspecs are
  actually configured (typically just `refs/heads/*`), never our explicit ones. A user who wants a
  fully separate remote can just `git remote add` another one and pass its name.
- Verified end-to-end with a bare remote and two clones: push (new task) → pull (created locally) →
  fast-forward path → **both clones editing the same task without syncing, in different fields**
  (comment+priority vs. label+assignee) → one side's push accepted, the other's rejected with the
  correct "needs pull first" error → pull triggers a real merge commit containing every op from
  both sides → pushing that merge back and pulling it into the first clone converges to **byte-
  identical folded JSON state** on both sides, confirmed by diff. `git log --graph` on the task ref
  shows the actual merge topology.

### Packaging as a git subcommand (+ standalone alias)
Crate is a lib (`git_task`) plus two thin bins that both just call `git_task::run(bin_name)`:
`src/bin/git-task.rs` (name is load-bearing — git execs the literal `git-task` binary it finds on
PATH when you type `git task …`) and `src/bin/ght.rs` (same CLI, standalone). `run()` overrides
clap's `Command::name`/`bin_name` at runtime so `--help`/`--version`/usage lines show the entrypoint
actually invoked ("git task ..." vs "ght ..."), from one shared `Cli` definition.
`cargo install --path .` puts both on PATH. Ship `git task completions <shell>` (clap-generated) and
`--help/--version`.
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
  lib.rs                   # module tree + run(bin_name) shared by both bins
  bin/     git-task.rs ght.rs                # thin entrypoints, call git_task::run(...)
  cli/                    # one handler per subcommand: new, ls, show, edit, status,
                          #   comment, label, link, epic, log, key, fields, register,
                          #   repos, projects, push, pull, automation, config, completions
  domain/  task.rs op.rs fold.rs id.rs      # model, Operation enum, replay/fold, id prefix/display
  store/   mod.rs git_store.rs merge.rs     # git2-backed refs/tasks store + union/LWW merge
  config/  global.rs project.rs fields.rs   # user config, per-repo config, required-field schema
  automation/ engine.rs rules.rs            # event dispatch, evalexpr, actions, loop guard
  git/     repo.rs                          # open/discover repo, remotes, push/pull refspecs
  workflow.rs actor.rs prompt.rs render.rs
tests/                                       # integration (temp repos + temp config)
```

## Command surface (v1)
```
git task new "title" [--kind bug] [--desc …] [--assignee me] [--label x] [--priority high] [--parent EPIC] [--milestone m1] [--due DATE]
git task ls [--here|--repo NAME|--project P] [--status s --assignee a --label l --kind k --mine]
git task show <id> [--format text|md|json]   git task export [<id>|--all] --format md|json   git task log <id>
git task edit <id> [--title|--desc|--priority|--assignee|--kind|--milestone|--due …]
git task status <id> <state>  git task comment <id> "…"    git task label <id> add|rm <label>
git task link <id> add|rm blocks|relates|dup <other>        git task epic <id> add|rm <child>
git task key [NEWKEY]                     git task fields
git task register [name] [--project P]   git task unregister <name>   git task repos   git task projects
git task push [remote]        git task pull [remote]
git task automation list                  git task config …           git task completions <shell>
```

## Implementation phases
1. ✅ **Skeleton** — cargo project, clap dispatch, `git2` open/discover, `actor` from git config, config dir + env
   override. Milestone: `git task --version`, `git task register`, `git task repos`.
2. ✅ **Core store** — Operation enum, `fold`, `git_store` read/write op-chain under `refs/tasks/<id>`. Commands:
   `new, show, ls --here, edit, status, comment, log, export`. Default workflow.
2.5. ✅ **Addressing, required fields, dual binary** — `KEY-hash` display addressing; per-repo
   `.gittask/config.toml` (key + field schema); global+per-project required-field merge with
   TTY-gated interactive prompts on `new`; `git task key`/`git task fields`; split into
   `git-task`/`ght` binaries sharing one lib. Not in the original phase list — added mid-stream
   per user request, folded in here since it touches addressing/config used by everything after.
3. ✅ **Cross-repo** — global config repos/projects, aggregate `ls` + filters, `register/unregister/repos/projects`.
4. ✅ **Epics/links/milestones** — `SetParent`/`ClearParent`, `AddLink`/`RemoveLink` (dedup'd, both
   sides guarded against self-reference), `SetMilestone`. Commands: `epic <epic> add|rm <child>`,
   `link <id> add|rm <kind> <other>`, `--milestone`/`--parent` on `new`, `--milestone` on `edit`,
   `ls --parent` to list an epic's children. `show`/`log`/export render Parent/Links/Milestone with
   KEY-hash addressing throughout (render functions now take the repo `key` and resolve embedded
   ids themselves, not just the task's own id). No reverse lookup on `show` (an epic doesn't list
   its children inline — use `ls --parent <epic>`) and no distinct "sprint" entity — "sprints" in
   the original scope line landed as the `milestone` field only; a real sprint/cycle grouping
   wasn't asked for beyond that and would be new scope, not a gap in what was planned here.
5. ✅ **Automation** — engine (global `automation.toml` + `.gittask/config.toml`), `evalexpr` conditions,
   actions→ops, loop guard. Plus the missing `label` command (gap from phase 2, needed for the
   `label.added` event) and `automation list`. `test`/`run` deferred (see Automation section above).
6. ✅ **Sync** — `push`/`pull`, revwalk-based DAG `load` (replaces the old linear parent walk),
   real two-parent merge commits for divergent chains, deterministic op sort for convergence.
7. **Polish** — completions, table output, docs (README usage), tests.

## Verification
- **Unit:** fold correctness (create+edits → expected `Task`); merge union+LWW; id-prefix resolution; condition eval.
- **Integration** (`tests/`): create temp git repos in a `tempdir`, point `GIT_TASK_CONFIG_DIR` at a temp dir,
  run commands via the binary, assert `refs/tasks/*` created; register two temp repos and assert cross-repo `ls`
  aggregates both; push to a bare remote, clone elsewhere, `pull`, assert merged state.
- **Manual e2e:** `cargo install --path .`; in a repo `git task new` / `ls` / `status` / `comment`; register a
  2nd repo, `git task ls` cross-repo; add a `[[rule]]` and confirm it fires; `git task push` to a bare remote,
  clone, `git task pull`, verify tasks + a concurrent-edit merge resolve correctly.
