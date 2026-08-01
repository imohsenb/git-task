use anyhow::Result;
use clap::Args;
use comfy_table::{Cell, Color};

use crate::config::global::GlobalConfig;
use crate::table;

#[derive(Args)]
pub struct ProjectsArgs {}

pub fn run(_args: ProjectsArgs) -> Result<()> {
    let config = GlobalConfig::load()?;
    let projects = config.projects();
    if projects.is_empty() {
        println!("no repos registered. Run 'git task register' inside a repo to add one.");
        return Ok(());
    }

    let mut t = table::new();
    t.set_header(table::header(&["PROJECT", "REPOS"]));
    for (project, repos) in &projects {
        t.add_row(vec![Cell::new(project).fg(Color::Blue), Cell::new(repos.join(", "))]);
    }
    println!("{t}");
    Ok(())
}
