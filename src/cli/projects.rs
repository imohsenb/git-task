use anyhow::Result;
use clap::Args;
use comfy_table::Table;

use crate::config::global::GlobalConfig;

#[derive(Args)]
pub struct ProjectsArgs {}

pub fn run(_args: ProjectsArgs) -> Result<()> {
    let config = GlobalConfig::load()?;
    let projects = config.projects();
    if projects.is_empty() {
        println!("no repos registered. Run 'git task register' inside a repo to add one.");
        return Ok(());
    }

    let mut table = Table::new();
    table.set_header(vec!["PROJECT", "REPOS"]);
    for (project, repos) in &projects {
        table.add_row(vec![project.clone(), repos.join(", ")]);
    }
    println!("{table}");
    Ok(())
}
