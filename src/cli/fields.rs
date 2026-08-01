use anyhow::Result;
use clap::Args;

use crate::config::fields;
use crate::config::global::GlobalConfig;
use crate::config::project::ProjectConfig;
use crate::git;

#[derive(Args)]
pub struct FieldsArgs {}

pub fn run(_args: FieldsArgs) -> Result<()> {
    let repo = git::repo::discover_current()?;
    let workdir = git::repo::workdir(&repo)?;
    let global = GlobalConfig::load()?;
    let project = ProjectConfig::load(&workdir)?;
    let required = fields::resolve(&global.fields, &project.fields);

    println!("title       required (fixed)");
    println!("description required (fixed)");
    println!("priority    {}", state(required.priority));
    println!("assignee    {}", state(required.assignee));
    println!("due         {}", state(required.due));
    println!();
    println!("Edit ~/.config/git-task/config.toml ([fields.<name>] required = true) for global");
    println!("defaults, or .gittask/config.toml in this repo to override per-project.");
    Ok(())
}

fn state(required: bool) -> &'static str {
    if required {
        "required"
    } else {
        "optional"
    }
}
