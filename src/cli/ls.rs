use std::collections::{BTreeMap, BTreeSet, HashMap};

use anyhow::{bail, Result};
use clap::Args;
use git2::Repository;
use serde::Serialize;

use crate::actor::Actor;
use crate::color;
use crate::config::global::{GlobalConfig, RepoEntry};
use crate::config::project;
use crate::domain::id;
use crate::domain::op::TaskKind;
use crate::domain::task::Task;
use crate::git;
use crate::hints;
use crate::identity;
use crate::logger::Logger;
use crate::output::{self, ClassifiedError, TaskJson};
use crate::store::git_store::Store;
use crate::table::{self, Seg};
use crate::style;

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
    #[arg(long = "fixed-version")]
    fixed_version: Option<String>,
    #[arg(long = "affected-version")]
    affected_version: Option<String>,
    #[arg(long, value_enum)]
    kind: Option<TaskKind>,
    /// Shorthand for tasks assigned to you (matches your git user.name or user.email)
    #[arg(long)]
    mine: bool,
    /// Only children of this epic (id or KEY-hash address)
    #[arg(long)]
    parent: Option<String>,
    /// Include soft-deleted tasks (hidden by default)
    #[arg(long)]
    deleted: bool,
    /// `--format json` only: include each task's full op-chain history (omitted by default to
    /// keep listings small)
    #[arg(long = "with-history")]
    with_history: bool,
}

struct Row {
    repo: String,
    project: String,
    display_id: String,
    assignee_display: Option<String>,
    task: Task,
}

/// One repo's filtered task set plus what `ls`'s own JSON/text rendering both need from it —
/// `collect_task_set` is the single place the filter predicates live, so text and JSON can never
/// disagree on which tasks matched.
struct RepoTaskSet {
    key: String,
    directory: HashMap<String, String>,
    tasks: Vec<Task>,
}

/// One repo's contribution to either rendering: metadata plus its filtered tasks. Built once per
/// repo regardless of mode (`here` produces exactly one; the registry sweep produces one per
/// successfully-opened repo), so both `--format json` and the text table read from the same list.
struct RepoResult {
    name: String,
    project: String,
    path: String,
    key: String,
    branch: Option<String>,
    directory: HashMap<String, String>,
    tasks: Vec<Task>,
}

pub fn run(args: LsArgs) -> Result<()> {
    if args.mine && args.assignee.is_some() {
        bail!("pass either --mine or --assignee, not both");
    }
    let registry_selected = args.repo.is_some() || args.project.is_some();
    if args.here && (registry_selected || args.all) {
        bail!("--here lists only the current repo; --repo/--project/--all select from the registry instead");
    }

    // Narrows what "no tasks found" should suggest: with a filter active, an empty result more
    // likely means the filter excluded everything than that the repo has no tasks at all.
    let has_filters = args.status.is_some()
        || args.assignee.is_some()
        || args.label.is_some()
        || args.fixed_version.is_some()
        || args.affected_version.is_some()
        || args.kind.is_some()
        || args.mine
        || args.parent.is_some();

    let global_cfg = GlobalConfig::load()?;
    let current_repo = git::repo::discover_current();

    // Default (no --all, no --repo/--project) inside a repo means "just this repo" — --all
    // opts into the full registry. Outside any repo there's no "this repo" to default to, so
    // an unadorned `ls` there still spans the whole registry, same as it always has.
    let current_only = args.here || (!registry_selected && !args.all && current_repo.is_ok());

    if current_only {
        let repo = current_repo?;
        let (repo_name, project_name) = current_repo_label(&repo, &global_cfg);
        let branch = git::repo::current_branch(&repo);
        let path = git::repo::workdir(&repo).map(|p| p.display().to_string()).unwrap_or_default();
        let set = collect_task_set(&repo, &args)?;
        let result = RepoResult {
            name: repo_name,
            project: project_name,
            path,
            key: set.key,
            branch,
            directory: set.directory,
            tasks: set.tasks,
        };

        if output::is_json() {
            print_ls_json("here", vec![result], &args);
            return Ok(());
        }

        let scope = match &result.branch {
            Some(branch) => format!("{} [{branch}]", result.name),
            None => result.name.clone(),
        };
        let rows = rows_from(vec![result]);
        let had_rows = !rows.is_empty();
        // A single-repo listing already implies which repo/project it's from — showing those
        // columns here would just repeat the same value down every row. Aggregation across the
        // registry (below) is exactly when they earn their keep, disambiguating each row.
        print_rows(rows, false, has_filters, &scope);
        if had_rows {
            print_follow_up_hints();
        }
        return Ok(());
    }

    let mut entries: Vec<(&String, &RepoEntry)> = global_cfg.repos.iter().collect();
    if let Some(name) = &args.repo {
        entries.retain(|(n, _)| *n == name);
        if entries.is_empty() {
            return Err(anyhow::Error::new(ClassifiedError::NotFound {
                message: format!("no repo registered named '{name}'"),
                query: name.clone(),
                entity: "repo".to_string(),
            }));
        }
    }
    if let Some(proj) = &args.project {
        entries.retain(|(_, e)| &e.project == proj);
        if entries.is_empty() {
            return Err(anyhow::Error::new(ClassifiedError::NotFound {
                message: format!("no repos registered under project '{proj}'"),
                query: proj.clone(),
                entity: "project".to_string(),
            }));
        }
    }
    if entries.is_empty() {
        bail!(
            "no repos registered. Run 'git task register' inside a repo to add one, or pass --here to list just the current repo."
        );
    }
    entries.sort_by_key(|(name, _)| name.as_str());
    let repo_count = entries.len();

    let mut results = Vec::new();
    for (name, entry) in entries {
        let repo = match git::repo::open(&entry.path) {
            Ok(r) => r,
            Err(err) => {
                warn_skip(name, &format!("{} — {err:#}", entry.path.display()), Some(name));
                continue;
            }
        };
        let branch = git::repo::current_branch(&repo);
        let path = entry.path.display().to_string();
        match collect_task_set(&repo, &args) {
            Ok(set) => results.push(RepoResult {
                name: name.clone(),
                project: entry.project.clone(),
                path,
                key: set.key,
                branch,
                directory: set.directory,
                tasks: set.tasks,
            }),
            Err(err) => warn_skip(name, &format!("{err:#}"), Some(name)),
        }
    }

    if output::is_json() {
        print_ls_json("registry", results, &args);
        return Ok(());
    }

    let had_rows = results.iter().any(|r| !r.tasks.is_empty());
    let scope = format!("{repo_count} registered repo{}", if repo_count == 1 { "" } else { "s" });
    let rows = rows_from(results);
    print_rows(rows, true, has_filters, &scope);
    if had_rows {
        print_follow_up_hints();
    }
    Ok(())
}

/// Prints (or, when `Logger::warn` would in text mode, collects) a "skipping '<name>'" warning
/// for one repo the registry sweep couldn't use — an unopenable path, or a filter/read failure
/// inside it. `raw_detail` carries just the error text; text mode still prefixes it with
/// "Details: " (unchanged from before this command supported `--format json`), JSON mode keeps
/// it bare in the warning's own `detail` field.
fn warn_skip(name: &str, raw_detail: &str, scope: Option<&str>) {
    if output::is_json() {
        output::collect_warning(&format!("skipping '{name}'"), Some(raw_detail), scope);
    } else {
        Logger::warn(&format!("skipping '{name}'"), Some(&format!("Details: {raw_detail}")), &[]);
    }
}

/// The current repo's display name and project, preferring the registry entry (so it matches
/// whatever `repos`/`projects` call it) and falling back to the working directory's own name
/// with no project — same fallback `banner::repo_status` uses for an unregistered repo.
fn current_repo_label(repo: &Repository, global_cfg: &GlobalConfig) -> (String, String) {
    if let Ok(workdir) = git::repo::workdir(repo) {
        if let Some((name, entry)) = global_cfg.entry_for_path(&workdir) {
            return (name.to_string(), entry.project.clone());
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

fn collect_task_set(repo: &Repository, args: &LsArgs) -> Result<RepoTaskSet> {
    let store = Store::new(repo);
    let key = project::effective_key_for(repo)?;
    let ids = store.list_ids()?;
    let directory = identity::contributor_directory(repo)?;

    let mine_actor = if args.mine { Some(Actor::from_repo(repo)?) } else { None };
    // If --parent doesn't resolve in this repo, treat as "no match" rather than an error —
    // in cross-repo mode the epic may simply live in a different registered repo.
    let parent_id = args.parent.as_deref().and_then(|p| store.resolve(p).ok());
    if args.parent.is_some() && parent_id.is_none() {
        return Ok(RepoTaskSet { key, directory, tasks: Vec::new() });
    }

    let mut tasks = Vec::new();
    for full_id in ids {
        let task = store.load(&full_id)?;

        if task.deleted && !args.deleted {
            continue;
        }
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
        if let Some(v) = &args.fixed_version {
            if !task.fixed_versions.contains(v) {
                continue;
            }
        }
        if let Some(v) = &args.affected_version {
            if !task.affected_versions.contains(v) {
                continue;
            }
        }
        if let Some(k) = &args.kind {
            if &task.kind != k {
                continue;
            }
        }
        if let Some(a) = &args.assignee {
            let matches_email = task.assignee.as_deref() == Some(a.as_str());
            let matches_name = task
                .assignee
                .as_deref()
                .map(|e| identity::display_name(&directory, e).eq_ignore_ascii_case(a))
                .unwrap_or(false);
            if !matches_email && !matches_name {
                continue;
            }
        }
        if let Some(actor) = &mine_actor {
            if task.assignee.as_deref() != Some(actor.email.as_str()) {
                continue;
            }
        }
        if let Some(p) = &parent_id {
            if task.parent.as_deref() != Some(p.as_str()) {
                continue;
            }
        }

        tasks.push(task);
    }
    Ok(RepoTaskSet { key, directory, tasks })
}

/// Flattens `RepoResult`s into text-table `Row`s — unchanged shape/order from before `ls` grew
/// JSON support, just reading from the shared `RepoResult` list instead of building rows inline.
fn rows_from(results: Vec<RepoResult>) -> Vec<Row> {
    let mut rows = Vec::new();
    for result in results {
        for task in result.tasks {
            let assignee_display = task.assignee.as_deref().map(|e| identity::display_name(&result.directory, e));
            rows.push(Row {
                repo: result.name.clone(),
                project: result.project.clone(),
                display_id: id::display(&result.key, &task.id),
                assignee_display,
                task,
            });
        }
    }
    rows
}

#[derive(Serialize)]
struct LsScope {
    mode: &'static str,
    repo_count: usize,
    branch: Option<String>,
}

#[derive(Serialize)]
struct LsFilters {
    status: Option<String>,
    assignee: Option<String>,
    label: Option<String>,
    fixed_version: Option<String>,
    affected_version: Option<String>,
    kind: Option<TaskKind>,
    parent: Option<String>,
    mine: bool,
    deleted: bool,
}

#[derive(Serialize)]
struct RepoJson {
    name: String,
    project: String,
    path: String,
    key: String,
    branch: Option<String>,
    tasks: Vec<TaskJson>,
}

#[derive(Serialize)]
struct LsJson {
    scope: LsScope,
    filters_applied: LsFilters,
    repos: Vec<RepoJson>,
    contributors: BTreeMap<String, String>,
    statuses: Vec<String>,
    total: usize,
}

fn print_ls_json(mode: &'static str, results: Vec<RepoResult>, args: &LsArgs) {
    let branch = if mode == "here" { results.first().and_then(|r| r.branch.clone()) } else { None };

    let mut contributors = BTreeMap::new();
    let mut statuses = BTreeSet::new();
    let mut total = 0usize;
    let mut repos = Vec::with_capacity(results.len());
    for result in results {
        for (email, name) in &result.directory {
            contributors.entry(email.clone()).or_insert_with(|| name.clone());
        }
        for task in &result.tasks {
            statuses.insert(task.status.clone());
        }
        total += result.tasks.len();
        let tasks = result
            .tasks
            .iter()
            .map(|t| TaskJson::from_task(t, &result.key, &result.directory, args.with_history))
            .collect();
        repos.push(RepoJson {
            name: result.name,
            project: result.project,
            path: result.path,
            key: result.key,
            branch: result.branch,
            tasks,
        });
    }

    let response = LsJson {
        scope: LsScope { mode, repo_count: repos.len(), branch },
        filters_applied: LsFilters {
            status: args.status.clone(),
            assignee: args.assignee.clone(),
            label: args.label.clone(),
            fixed_version: args.fixed_version.clone(),
            affected_version: args.affected_version.clone(),
            kind: args.kind,
            parent: args.parent.clone(),
            mine: args.mine,
            deleted: args.deleted,
        },
        repos,
        contributors,
        statuses: statuses.into_iter().collect(),
        total,
    };
    output::print_ok(response);
}

const HEADERS: [&str; 6] = ["ID", "STATUS", "KIND", "PRIORITY", "ASSIGNEE", "TITLE"];
const HEADERS_WITH_REPO: [&str; 8] =
    ["ID", "REPO", "PROJECT", "STATUS", "KIND", "PRIORITY", "ASSIGNEE", "TITLE"];

fn row_to_segs(row: Row, show_repo_project: bool) -> Vec<Seg> {
    let Row { repo, project, display_id, assignee_display, task } = row;

    let id_seg = Seg { colored: color::cyan(&display_id), plain: display_id };
    let status_seg = if task.deleted {
        Seg { colored: color::bold_red("DELETED"), plain: "DELETED".to_string() }
    } else {
        style::status(&task)
    };
    let priority_seg = style::priority(&task);
    let kind_seg = style::kind(&task);

    let assignee_seg = match assignee_display {
        Some(a) => Seg { colored: a.clone(), plain: a },
        None => Seg { colored: color::dim("unassigned"), plain: "unassigned".to_string() },
    };

    let title_seg = Seg { colored: color::bold(&task.title), plain: task.title };

    if show_repo_project {
        let repo_seg = Seg { colored: color::light(&repo), plain: repo };
        let project_seg = Seg { colored: color::dim(&project), plain: project };
        vec![id_seg, repo_seg, project_seg, status_seg, kind_seg, priority_seg, assignee_seg, title_seg]
    } else {
        vec![id_seg, status_seg, kind_seg, priority_seg, assignee_seg, title_seg]
    }
}

fn print_rows(rows: Vec<Row>, show_repo_project: bool, has_filters: bool, scope: &str) {
    if rows.is_empty() {
        let message = format!("No tasks found in {}", color::cyan(scope));
        if has_filters {
            Logger::info(&message, None, &[("ls".to_string(), "clear filters and list everything".to_string())]);
        } else {
            Logger::info(
                &message,
                None,
                &[("new \"title\" --desc \"...\"".to_string(), "create your first task".to_string())],
            );
        }
        return;
    }

    let title = format!("TASKS ({})", rows.len());
    let headers: &[&str] = if show_repo_project { &HEADERS_WITH_REPO } else { &HEADERS };
    let table_rows: Vec<Vec<Seg>> =
        rows.into_iter().map(|row| row_to_segs(row, show_repo_project)).collect();
    for line in table::list_box(&title, headers, table_rows) {
        println!("{line}");
    }
}
