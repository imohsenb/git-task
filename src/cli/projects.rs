use anyhow::Result;
use clap::Args;
use serde::Serialize;

use crate::color;
use crate::config::global::GlobalConfig;
use crate::output;
use crate::table::{self, Seg};

#[derive(Args)]
pub struct ProjectsArgs {}

#[derive(Serialize)]
struct ProjectJson {
    name: String,
    repos: Vec<String>,
}

#[derive(Serialize)]
struct ProjectsJson {
    default_project: String,
    projects: Vec<ProjectJson>,
}

pub fn run(_args: ProjectsArgs) -> Result<()> {
    let config = GlobalConfig::load()?;
    let projects = config.projects();

    if output::is_json() {
        let projects_json =
            projects.into_iter().map(|(name, repos)| ProjectJson { name, repos }).collect();
        output::print_ok(ProjectsJson { default_project: config.default_project, projects: projects_json });
        return Ok(());
    }

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
                Seg { colored: color::cyan(project), plain: project.clone() },
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
