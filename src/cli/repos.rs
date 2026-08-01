use anyhow::Result;
use clap::Args;
use comfy_table::{Cell, Color};

use crate::config::global::GlobalConfig;
use crate::table;

#[derive(Args)]
pub struct ReposArgs {}

pub fn run(_args: ReposArgs) -> Result<()> {
    let config = GlobalConfig::load()?;
    if config.repos.is_empty() {
        println!("no repos registered. Run 'git task register' inside a repo to add one.");
        return Ok(());
    }

    let mut t = table::new();
    t.set_header(table::header(&["NAME", "PROJECT", "PATH"]));
    for (name, entry) in &config.repos {
        t.add_row(vec![
            Cell::new(name).fg(table::cyan()),
            Cell::new(&entry.project).fg(Color::Blue),
            Cell::new(entry.path.display().to_string()),
        ]);
    }
    println!("{t}");
    Ok(())
}
