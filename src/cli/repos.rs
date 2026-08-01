use anyhow::Result;
use clap::Args;
use comfy_table::Table;

use crate::config::global::GlobalConfig;

#[derive(Args)]
pub struct ReposArgs {}

pub fn run(_args: ReposArgs) -> Result<()> {
    let config = GlobalConfig::load()?;
    if config.repos.is_empty() {
        println!("no repos registered. Run 'git task register' inside a repo to add one.");
        return Ok(());
    }

    let mut table = Table::new();
    table.set_header(vec!["NAME", "PROJECT", "PATH"]);
    for (name, entry) in &config.repos {
        table.add_row(vec![name.clone(), entry.project.clone(), entry.path.display().to_string()]);
    }
    println!("{table}");
    Ok(())
}
