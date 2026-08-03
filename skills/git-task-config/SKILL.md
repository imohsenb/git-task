---
name: git-task-config
description: Use when the user asks about a repo's git-task configuration (its short address key, which fields are required on new tasks) or automation rules (auto-set fields / auto-label / auto-comment on task events) — all via `git task config ...`. Trigger on "set the required fields", "add an automation rule", "change the task key prefix", "what fields are required", "auto-triage bugs".
---

# git-task config & automation

Per-repo config (address key, field requirements, automation rules) is **event-sourced under
`refs/tasks/config`** — the same commit-chain mechanism as a task, not a working-tree file. There
is no `.gittask/` file to read or edit. Every change goes through `git task config ...`; never
hand-edit anything to change it.

Two separate layers — know which one a request means before picking a flag:

- **Per-repo** (shared, travels with every clone, lives under `refs/tasks/config`): address key,
  required-field overrides, this repo's automation rules.
- **User-level** (personal, this machine only, plain TOML): `~/.config/git-task/config.toml`
  (registered repos, default field requirements) and `~/.config/git-task/automation.toml` (global
  automation rules that apply across every repo). Reach for `--global` on `rule add` to write here
  instead of the per-repo chain.

## Inspect

```sh
git task config show --format json     # key, resolved required fields, rules — read this first
git task fields --format json          # just the effective required-field schema
```

## Address key

```sh
git task config key SRV        # pin the key used for display (SRV-9057e58a); "git task key SRV" is a short alias
git task config key             # omit the new key to print the current one
```

## Required fields

Only `priority`, `assignee`, `due` are configurable this way — `title`/`description` are always
required and cannot be turned off.

```sh
git task config field priority required   # or: optional (drops the override back to default)
```

## Automation rules

Rules run after every mutation (`new`, `edit`, `status`, `comment`, `label`, `version`, `epic`, `link`).

```sh
git task config rule add \
  --name auto-triage-bugs \
  --on task.created \
  --when 'kind == "bug"' \
  --do 'set_priority high' --do 'add_label triage'

git task config rule add --global     # write to ~/.config/git-task/automation.toml instead
git task config rule list --format json   # effective global + per-repo rules
git task config rule remove auto-triage-bugs
```

- `--on` (required, exactly one): `task.created | status.changed | comment.added | label.added |
  task.updated`.
- `--when` (optional; omit for unconditional): an `evalexpr` condition over `kind` / `status` /
  `priority` / `assignee` / `title` — all strings, empty string if the field is unset on that task.
- `--do` (repeatable, at least one): `set_priority|status|assignee|kind|due|milestone <value>`,
  `add_label|remove_label <value>`, `add_fixed_version|remove_fixed_version <value>`,
  `add_affected_version|remove_affected_version <value>`, `add_comment "text"`.
- Omitting `--name`/`--on`/`--do` on `rule add` starts an interactive wizard instead — don't invoke
  bare `rule add` from a script; always pass the flags.

Rule actions run as their own attributed op-package (`git-task-automation` actor, visible in
`git task log`). A rule can fire **at most once per command** — its own actions can cascade into
*other* rules (e.g. a `set_status` action re-triggers a `status.changed` rule), but never back into
itself, so there's no infinite loop to guard against yourself. A misconfigured rule (bad `when`,
unparseable action) is skipped with a warning, not a hard failure of the triggering command.
