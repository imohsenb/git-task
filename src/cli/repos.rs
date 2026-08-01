use anyhow::Result;
use clap::Args;

use crate::color;
use crate::config::global::GlobalConfig;
use crate::table::{self, Seg};

#[derive(Args)]
pub struct ReposArgs {}

pub fn run(_args: ReposArgs) -> Result<()> {
    let config = GlobalConfig::load()?;
    if config.repos.is_empty() {
        println!("no repos registered. Run 'git task register' inside a repo to add one.");
        return Ok(());
    }

    let headers = ["NAME", "PROJECT", "PATH"];
    let rows: Vec<Vec<Seg>> = config
        .repos
        .iter()
        .map(|(name, entry)| {
            vec![
                Seg { colored: color::cyan(name), plain: name.clone() },
                Seg { colored: color::dim(&entry.project), plain: entry.project.clone() },
                Seg { colored: entry.path.display().to_string(), plain: entry.path.display().to_string() },
            ]
        })
        .collect();

    let title = format!("REPOS ({})", rows.len());
    for line in table::list_box(&title, &headers, rows) {
        println!("{line}");
    }
    Ok(())
}
