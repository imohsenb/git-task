use anyhow::Result;
use clap::Args;
use comfy_table::Table;

use crate::config::project;
use crate::domain::id;
use crate::domain::op::TaskKind;
use crate::git;
use crate::store::git_store::Store;

#[derive(Args)]
pub struct LsArgs {
    #[arg(long)]
    status: Option<String>,
    #[arg(long)]
    assignee: Option<String>,
    #[arg(long)]
    label: Option<String>,
    #[arg(long, value_enum)]
    kind: Option<TaskKind>,
}

pub fn run(args: LsArgs) -> Result<()> {
    let repo = git::repo::discover_current()?;
    let store = Store::new(&repo);
    let ids = store.list_ids()?;

    if ids.is_empty() {
        println!("no tasks yet. Run 'git task new \"title\"' to create one.");
        return Ok(());
    }

    let key = project::effective_key_for(&repo)?;
    let mut table = Table::new();
    table.set_header(vec!["ID", "STATUS", "KIND", "PRIORITY", "ASSIGNEE", "TITLE"]);

    for full_id in ids {
        let task = store.load(&full_id)?;

        if let Some(s) = &args.status {
            if &task.status != s {
                continue;
            }
        }
        if let Some(a) = &args.assignee {
            if task.assignee.as_deref() != Some(a.as_str()) {
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

        table.add_row(vec![
            id::display(&key, &full_id),
            task.status.clone(),
            format!("{:?}", task.kind),
            task.priority.clone().unwrap_or_default(),
            task.assignee.clone().unwrap_or_default(),
            task.title.clone(),
        ]);
    }

    println!("{table}");
    Ok(())
}
