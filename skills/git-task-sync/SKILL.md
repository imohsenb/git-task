---
name: git-task-sync
description: Use when the user wants to sync git-task tasks across machines or clones, register repos so `git task ls` aggregates across all of them without cd-ing, or hand someone a tasks-only clone with no source checkout — `git task clone/push/pull/register/unregister/repos/projects`. Trigger on "sync tasks", "push/pull tasks", "register this repo", "list tasks across all my repos", "clone just the tasks", "set up a tasks-only clone for a PM".
---

# git-task sync & cross-repo registration

Tasks live under `refs/tasks/*`, moved by their own explicit refspecs — a plain `git fetch` /
`git pull` / `git push` never touches them at all. Always use the `git task` subcommands below,
not raw `git push`/`git fetch` with a manual refspec.

## Push / pull

```sh
git task push [remote]     # default remote "origin"
git task pull [remote]     # fetch + reconcile
```

`pull` reconciles each task independently: new-to-you → created outright; remote strictly ahead →
fast-forward; edited on both sides → a real two-parent git merge commit (no data loss — both
branches' full histories remain, and reading a task always replays every reachable op in
topological order, so which side happened to run the merge doesn't matter). If your local tasks
have diverged from the remote, `push` is rejected exactly like a non-fast-forward branch push —
run `git task pull` first, don't force anything.

## Tasks-only clone

For someone who only needs to read/triage tasks and doesn't want the source checked out:

```sh
git task clone git@github.com:you/your-repo.git   # → ./your-repo-tasks, fresh dir, no working tree
cd your-repo-tasks
git task ls
```

This sets up `origin` on the way in, so `push`/`pull` work immediately after — it's not a one-off
export, it's a real (if minimal) `git task` install. The repo's config (address key, required
fields, automation rules) comes along too, since it's just another `refs/tasks/*` ref, so `ls`/
`show` display the real `KEY-` prefix immediately.

## Cross-repo registration (for `ls` without `cd`)

```sh
git task register                  # register current repo under its dir name, project "main"
git task register --project web    # register under an explicit project group
git task repos --format json       # every registered repo
git task repos --format json --deep   # + per-repo probe: task counts, remotes, identity — never
                                       #   fails outright on one unopenable repo
git task projects --format json    # projects and the repos grouped under each
git task unregister <name>         # remove a registration
```

Once repos are registered, `git task ls` with no flags aggregates tasks across all of them (falls
back to just the current repo if nothing is registered yet):

```sh
git task ls --format json                       # everything, every registered repo
git task ls --project backend --format json      # only repos in one project group
git task ls --repo web --format json             # only one named repo
git task ls --here --format json                 # only the current repo, ignoring the registry
git task ls --format json --with-history          # kanban-shaped grouped listing
```

Registration is user-level state (`~/.config/git-task/config.toml`), personal to this machine —
registering a repo on one clone doesn't register it anywhere else; run `register` again on each
machine/clone that should include it in aggregated `ls`.

`register` also captures the repo's `origin` remote URL (if it has one) alongside its local path.
This is what makes `git task link ... --repo <registered-name>` (see the **git-task** skill)
portable: the URL, not the machine-local path, gets stored on the link, and it's compared in a
protocol/host-form-agnostic way — so the same cross-repo link resolves correctly for a teammate
whose registry maps that URL to a different local path than yours.

## Related skills

- **git-task** — everyday task CRUD (new/show/edit/status/comment/label/link/delete/cross-repo link).
- **git-task-config** — per-repo config and automation rules (also travels via clone/push/pull).
