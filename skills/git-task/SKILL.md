---
name: git-task
description: Use when working in a repo that tracks work with git-task — creating, showing, listing, editing, commenting on, labeling, linking, or deleting tasks. Tasks live as git objects under refs/tasks/*, managed only through the `git task` / `gtask` CLI, never by hand-editing anything. Trigger on "create a task", "list tasks", "what's the status of X", "add a comment/label", "close/delete a task", "link this to that", or any task-tracking request in a repo using git-task.
---

# git-task

A git-native task manager. Tasks are event-sourced operations stored as git commits under
`refs/tasks/<id>` — there is no working-tree file to read or edit. Every mutation is a CLI
subcommand; never write to `refs/tasks/*` directly and never invent a task file format.

Two equivalent binaries: `git task <cmd>` (git subcommand form) and `gtask <cmd>` (standalone).
Use whichever is on `$PATH`; try `git task` first since that's the common case.

## Always use `--format json` when you (the agent) need to parse the result

Every command accepts a global `--format json` flag (goes anywhere: `git task ls --format json` or
`git task --format json ls`). It prints exactly one JSON document on stdout and nothing else — no
hint text, no automation chatter — so it is always safe to parse, including on failure:

```jsonc
// success
{ "ok": true, "command": "new", "version": "1.0.0", "data": { /* command-specific */ }, "warnings": [] }

// failure — still exit code 1, still one JSON document on stdout, not an exception to catch differently
{ "ok": false, "command": "show", "version": "1.0.0",
  "error": { "kind": "not_found", "message": "no task matching 'deadbeef'", "causes": [], "context": {} },
  "warnings": [] }
```

Check `ok` first, not the process exit code alone, if you want the structured reason. `error.kind`
is one of `not_a_repo | identity_missing | not_found | ambiguous_id | validation | conflict |
rejected | remote | io | internal`. Tasks in JSON responses (`show`, `export`, `ls`, and every
mutation's `task` field) carry a resolved `display_id`/`key` and `*_name` fields
(`assignee_name`, `reporter_name`, ...) already looked up for you — don't re-derive them.

Without `--format json`, output is a human-oriented boxed/table view meant for a terminal, not for
parsing — don't scrape it.

## Addressing

A task's real identity is a git object hash. `KEY-<hash prefix>` (e.g. `SRV-9057e58a`) is a
readable display form — the `KEY-` part is cosmetic and stripped before lookup, so a bare hash
prefix (`9057e58a`) always works too. Get the key/hash for a task from a prior `new`/`show`/`ls`
JSON response; don't guess or reformat one yourself.

## Non-interactive gotcha

`title` and `description` are always required; other fields (`priority`/`assignee`/`due`) may be
required too depending on repo config. At a real terminal, `git task new` prompts for missing
required fields. **Piped/scripted (no TTY — this is you)**, it fails fast instead, listing exactly
which fields are missing, rather than hanging waiting for input. Before creating a task
non-interactively, either pass every field you know is required, or check first with
`git task fields --format json` to see the effective required set for this repo.

## Core commands

```sh
git task new "Fix login timeout" --kind bug --priority high --label auth
git task new "Hotfix" --kind bug --status doing        # land directly on a status, one op package
git task new "Design new UI" --kind story --parent SRV-epic --milestone v2.0

git task show SRV-9057e58a --format json                # full task detail
git task show SRV-9057e58a --markdown                    # markdown rendering (human-facing)

git task ls --format json                                # every registered repo, aggregated
git task ls --status doing --kind bug --mine --format json   # filters compose
git task ls --parent SRV-epic --format json               # one epic's children
git task ls --deleted --format json                        # include soft-deleted (hidden by default)

git task status SRV-9057e58a doing        # free-form status string, no fixed workflow
git task comment SRV-9057e58a "found the root cause"
git task comment SRV-9057e58a --edit 1 "revised note"
git task edit SRV-9057e58a --priority critical --assignee alice@example.com
git task edit SRV-9057e58a --clear-assignee --clear-due   # unset a field (also --clear-priority/--clear-milestone)
git task label SRV-9057e58a add urgent
git task label SRV-9057e58a rm urgent
git task log SRV-9057e58a --format json    # full audit trail (every op, in causal order)

git task epic SRV-epic add SRV-child      # make a task a child of an epic
git task epic SRV-epic rm SRV-child
git task link SRV-1 add blocks SRV-2      # relation kinds: blocks | relates | dup
git task link SRV-1 rm blocks SRV-2

git task export --all --format json       # every task in the repo, machine-readable
```

`git task edit` with **no flags at all** is interactive (prompts per field, enter keeps current
value) — never invoke bare `edit` from a script; always pass explicit `--field value` flags.

## Deleting — two different commands, don't confuse them

- `git task delete SRV-x` — **soft** delete: appends an event, syncs to peers on their next pull,
  hidden from `ls` by default. This is almost always what a user asking to "delete" or "close" a
  task means, since it's the one that propagates.
- `git task drop SRV-x --force` — **hard** delete: removes the local ref only, no history entry,
  does not sync (a peer's later `pull` can bring the task right back). Add `--remote [name]` to
  also remove it on that remote. Only use this if the user explicitly wants to purge local ref
  state, not to communicate a task is done/cancelled.

## Related skills

- **git-task-config** — per-repo config (address key, required fields) and automation rules.
- **git-task-sync** — clone/push/pull, cross-repo registration, aggregated `ls`.
