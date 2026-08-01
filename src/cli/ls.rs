use anyhow::{bail, Result};
use clap::Args;
use comfy_table::{Cell, Color};
use git2::Repository;

use crate::actor::Actor;
use crate::color::{self, Semantic};
use crate::config::global::{GlobalConfig, RepoEntry};
use crate::config::project;
use crate::domain::id;
use crate::domain::op::TaskKind;
use crate::domain::task::Task;
use crate::git;
use crate::hints;
use crate::store::git_store::Store;
use crate::table;

#[derive(Args)]
pub struct LsArgs {
    /// List only the current repo, ignoring the registry
    #[arg(long)]
    here: bool,
    /// Limit to one registered repo by name
    #[arg(long)]
    repo: Option<String>,
    /// Limit to repos registered under one project
    #[arg(long)]
    project: Option<String>,
    #[arg(long)]
    status: Option<String>,
    #[arg(long)]
    assignee: Option<String>,
    #[arg(long)]
    label: Option<String>,
    #[arg(long, value_enum)]
    kind: Option<TaskKind>,
    /// Shorthand for tasks assigned to you (matches your git user.name or user.email)
    #[arg(long)]
    mine: bool,
    /// Only children of this epic (id or KEY-hash address)
    #[arg(long)]
    parent: Option<String>,
}

struct Row {
    repo: String,
    project: String,
    display_id: String,
    task: Task,
}

pub fn run(args: LsArgs) -> Result<()> {
    if args.mine && args.assignee.is_some() {
        bail!("pass either --mine or --assignee, not both");
    }
    if args.here && (args.repo.is_some() || args.project.is_some()) {
        bail!("--here lists only the current repo; --repo/--project select from the registry instead");
    }

    let global_cfg = GlobalConfig::load()?;
    let registry_selected = args.repo.is_some() || args.project.is_some();

    if args.here || (!registry_selected && global_cfg.repos.is_empty()) {
        let repo = git::repo::discover_current()?;
        let rows = collect_rows(&repo, "", "", &args)?;
        let had_rows = !rows.is_empty();
        print_rows(rows, false);
        if had_rows {
            print_follow_up_hints();
        }
        return Ok(());
    }

    let mut entries: Vec<(&String, &RepoEntry)> = global_cfg.repos.iter().collect();
    if let Some(name) = &args.repo {
        entries.retain(|(n, _)| *n == name);
        if entries.is_empty() {
            bail!("no repo registered named '{name}'");
        }
    }
    if let Some(proj) = &args.project {
        entries.retain(|(_, e)| &e.project == proj);
        if entries.is_empty() {
            bail!("no repos registered under project '{proj}'");
        }
    }
    if entries.is_empty() {
        bail!(
            "no repos registered. Run 'git task register' inside a repo to add one, or pass --here to list just the current repo."
        );
    }
    entries.sort_by_key(|(name, _)| name.as_str());

    let mut rows = Vec::new();
    for (name, entry) in entries {
        let repo = match git::repo::open(&entry.path) {
            Ok(r) => r,
            Err(err) => {
                eprintln!("warning: skipping '{name}' ({}): {err:#}", entry.path.display());
                continue;
            }
        };
        match collect_rows(&repo, name, &entry.project, &args) {
            Ok(mut r) => rows.append(&mut r),
            Err(err) => eprintln!("warning: skipping '{name}': {err:#}"),
        }
    }

    let had_rows = !rows.is_empty();
    print_rows(rows, true);
    if had_rows {
        print_follow_up_hints();
    }
    Ok(())
}

fn print_follow_up_hints() {
    hints::print(&[
        ("show <id>".to_string(), "view full task details".to_string()),
        ("status <id> <status>".to_string(), "change a task's status".to_string()),
    ]);
}

fn collect_rows(repo: &Repository, repo_name: &str, project_name: &str, args: &LsArgs) -> Result<Vec<Row>> {
    let store = Store::new(repo);
    let key = project::effective_key_for(repo)?;
    let ids = store.list_ids()?;

    let mine_actor = if args.mine { Some(Actor::from_repo(repo)?) } else { None };
    // If --parent doesn't resolve in this repo, treat as "no match" rather than an error —
    // in cross-repo mode the epic may simply live in a different registered repo.
    let parent_id = args.parent.as_deref().and_then(|p| store.resolve(p).ok());
    if args.parent.is_some() && parent_id.is_none() {
        return Ok(Vec::new());
    }

    let mut rows = Vec::new();
    for full_id in ids {
        let task = store.load(&full_id)?;

        if let Some(s) = &args.status {
            if &task.status != s {
                continue;
            }
        }
        if let Some(l) = &args.label {
            if !task.labels.contains(l) {
                continue;
            }
        }
        if let Some(k) = &args.kind {
            if &task.kind != k {
                continue;
            }
        }
        if let Some(a) = &args.assignee {
            if task.assignee.as_deref() != Some(a.as_str()) {
                continue;
            }
        }
        if let Some(actor) = &mine_actor {
            let is_mine = task.assignee.as_deref() == Some(actor.name.as_str())
                || task.assignee.as_deref() == Some(actor.email.as_str());
            if !is_mine {
                continue;
            }
        }
        if let Some(p) = &parent_id {
            if task.parent.as_deref() != Some(p.as_str()) {
                continue;
            }
        }

        rows.push(Row {
            repo: repo_name.to_string(),
            project: project_name.to_string(),
            display_id: id::display(&key, &full_id),
            task,
        });
    }
    Ok(rows)
}

fn semantic_color(sem: Semantic) -> Color {
    match sem {
        Semantic::Success => Color::Green,
        Semantic::Warn => Color::Yellow,
        Semantic::Danger => Color::Red,
        Semantic::Info => Color::Cyan,
        Semantic::Neutral => Color::Reset,
    }
}

fn print_rows(rows: Vec<Row>, with_repo_columns: bool) {
    if rows.is_empty() {
        println!("no tasks found.");
        return;
    }

    let mut t = table::new();
    if with_repo_columns {
        t.set_header(table::header(&[
            "REPO", "PROJECT", "ID", "STATUS", "KIND", "PRIORITY", "ASSIGNEE", "TITLE",
        ]));
    } else {
        t.set_header(table::header(&["ID", "STATUS", "KIND", "PRIORITY", "ASSIGNEE", "TITLE"]));
    }

    for row in rows {
        let task = row.task;
        let mut cells: Vec<Cell> = if with_repo_columns {
            vec![Cell::new(row.repo).fg(Color::Magenta), Cell::new(row.project).fg(Color::Blue)]
        } else {
            Vec::new()
        };

        let kind_text = format!("{:?}", task.kind);
        let priority_text = task.priority.unwrap_or_default();

        cells.push(Cell::new(row.display_id).fg(Color::Cyan));
        cells.push(Cell::new(&task.status).fg(semantic_color(color::status_semantic(&task.status))));
        cells.push(Cell::new(&kind_text).fg(semantic_color(color::kind_semantic(task.kind))));
        cells.push(if priority_text.is_empty() {
            Cell::new("")
        } else {
            Cell::new(&priority_text).fg(semantic_color(color::priority_semantic(&priority_text)))
        });
        cells.push(Cell::new(task.assignee.unwrap_or_default()));
        cells.push(Cell::new(task.title));

        t.add_row(cells);
    }

    println!("{t}");
}
