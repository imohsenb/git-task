use anyhow::Result;
use clap::Args;

use crate::color;
use crate::config::global::GlobalConfig;
use crate::output;
use crate::table::{self, Seg};

#[derive(Args)]
pub struct ReposArgs {
    /// `--format json` only: also open each repo and probe it (key, branch, task counts,
    /// remotes, identity) instead of just listing the bare registry entry. Never fails the
    /// command because one repo is unopenable — that repo gets `openable: false` and an `error`
    /// instead.
    #[arg(long)]
    deep: bool,
}

pub fn run(args: ReposArgs) -> Result<()> {
    let config = GlobalConfig::load()?;

    if output::is_json() {
        output::print_ok(output::registry::build(&config, args.deep));
        return Ok(());
    }

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
