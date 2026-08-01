use anyhow::Result;
use clap::Args;

use crate::color;
use crate::config::global::GlobalConfig;
use crate::table::{self, Seg};

#[derive(Args)]
pub struct ProjectsArgs {}

pub fn run(_args: ProjectsArgs) -> Result<()> {
    let config = GlobalConfig::load()?;
    let projects = config.projects();
    if projects.is_empty() {
        println!("no repos registered. Run 'git task register' inside a repo to add one.");
        return Ok(());
    }

    let headers = ["PROJECT", "REPOS"];
    let rows: Vec<Vec<Seg>> = projects
        .iter()
        .map(|(project, repos)| {
            let repos_text = repos.join(", ");
            vec![
                Seg { colored: color::dim(project), plain: project.clone() },
                Seg { colored: repos_text.clone(), plain: repos_text },
            ]
        })
        .collect();

    let title = format!("PROJECTS ({})", rows.len());
    for line in table::list_box(&title, &headers, rows) {
        println!("{line}");
    }
    Ok(())
}
