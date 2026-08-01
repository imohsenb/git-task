use anyhow::Result;
use clap::Args;

use crate::config::global::GlobalConfig;
use crate::git;

#[derive(Args)]
pub struct RegisterArgs {
    /// Name to register the repo under (defaults to the repo directory name)
    name: Option<String>,
    /// Project to group this repo under (defaults to the configured default project)
    #[arg(long)]
    project: Option<String>,
}

pub fn run(args: RegisterArgs) -> Result<()> {
    let repo = git::repo::discover_current()?;
    let path = git::repo::workdir(&repo)?;

    let name = args.name.unwrap_or_else(|| {
        path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "repo".to_string())
    });

    let mut config = GlobalConfig::load()?;
    let project = config.register(name.clone(), path.clone(), args.project)?;
    config.save()?;

    println!("registered '{name}' ({}) in project '{project}'", path.display());
    Ok(())
}
