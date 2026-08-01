use anyhow::{bail, Result};
use clap::Args;
use git2::Repository;

use crate::actor::Actor;
use crate::color;
use crate::config::global::{GlobalConfig, RepoEntry};
use crate::config::project;
use crate::domain::id;
use crate::domain::op::TaskKind;
use crate::domain::task::Task;
use crate::git;
use crate::hints;
use crate::store::git_store::Store;
use crate::table::{self, Seg};

#[derive(Args)]
pub struct LsArgs {
    /// Force just the current repo, even with --repo/--project/--all set. Also the implicit
    /// default whenever `ls` runs inside a repo with no other selector.
    #[arg(long)]
    here: bool,
    /// Aggregate across every registered repo instead of just the current one
    #[arg(long)]
    all: bool,
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
    let registry_selected = args.repo.is_some() || args.project.is_some();
    if args.here && (registry_selected || args.all) {
        bail!("--here lists only the current repo; --repo/--project/--all select from the registry instead");
    }

    let global_cfg = GlobalConfig::load()?;
    let current_repo = git::repo::discover_current();

    // Default (no --all, no --repo/--project) inside a repo means "just this repo" — --all
    // opts into the full registry. Outside any repo there's no "this repo" to default to, so
    // an unadorned `ls` there still spans the whole registry, same as it always has.
    let current_only = args.here || (!registry_selected && !args.all && current_repo.is_ok());

    if current_only {
        let repo = current_repo?;
        let (repo_name, project_name) = current_repo_label(&repo, &global_cfg);
        let rows = collect_rows(&repo, &repo_name, &project_name, &args)?;
        let had_rows = !rows.is_empty();
        print_rows(rows);
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
    print_rows(rows);
    if had_rows {
        print_follow_up_hints();
    }
    Ok(())
}

/// The current repo's display name and project, preferring the registry entry (so it matches
/// whatever `repos`/`projects` call it) and falling back to the working directory's own name
/// with no project — same fallback `banner::repo_status` uses for an unregistered repo.
fn current_repo_label(repo: &Repository, global_cfg: &GlobalConfig) -> (String, String) {
    if let Ok(workdir) = git::repo::workdir(repo) {
        if let Some((name, entry)) = global_cfg.repos.iter().find(|(_, e)| e.path == workdir) {
            return (name.clone(), entry.project.clone());
        }
        let repo_name = workdir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "repo".to_string());
        return (repo_name, String::new());
    }
    ("repo".to_string(), String::new())
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

// const HEADERS: [&str; 8] = ["ID", "REPO", "PROJECT", "STATUS", "KIND", "PRIORITY", "ASSIGNEE", "TITLE"];
const HEADERS: [&str; 6] = ["ID", "STATUS", "KIND", "PRIORITY", "ASSIGNEE", "TITLE"];

fn row_to_segs(row: Row) -> Vec<Seg> {
    let Row { repo, project, display_id, task } = row;

    let id_seg = Seg { colored: color::cyan(&display_id), plain: display_id };
    // let repo_seg = Seg { colored: color::light(&repo), plain: repo };
    // let project_seg = Seg { colored: color::dim(&project), plain: project };

    let status_sem = color::status_semantic(&task.status);
    let status_plain = format!("{} {}", color::semantic_icon(status_sem), task.status);
    let status_seg = Seg { colored: color::paint(status_sem, &status_plain), plain: status_plain };

    let kind_plain = format!("{:?}", task.kind);
    let kind_seg = Seg { colored: color::paint(color::kind_semantic(task.kind), &kind_plain), plain: kind_plain };

    let priority_seg = match task.priority {
        Some(p) => {
            let text = format!("{} {}", color::priority_icon(p), p.as_str());
            Seg { colored: color::paint(color::priority_semantic(p), &text), plain: text }
        }
        None => Seg { colored: String::new(), plain: String::new() },
    };

    let assignee_seg = match task.assignee {
        Some(a) => Seg { colored: a.clone(), plain: a },
        None => Seg { colored: color::dim("unassigned"), plain: "unassigned".to_string() },
    };

    let title_seg = Seg { colored: color::bold(&task.title), plain: task.title };

    // vec![id_seg, repo_seg, project_seg, status_seg, kind_seg, priority_seg, assignee_seg, title_seg]
    vec![id_seg, status_seg, kind_seg, priority_seg, assignee_seg, title_seg]
}

fn print_rows(rows: Vec<Row>) {
    if rows.is_empty() {
        println!("no tasks found.");
        return;
    }

    let title = format!("TASKS ({})", rows.len());
    let table_rows: Vec<Vec<Seg>> = rows.into_iter().map(row_to_segs).collect();
    for line in table::list_box(&title, &HEADERS, table_rows) {
        println!("{line}");
    }
}
