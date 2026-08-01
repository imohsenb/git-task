use anyhow::{bail, Result};
use clap::Args;

use crate::config::global::GlobalConfig;

#[derive(Args)]
pub struct UnregisterArgs {
    /// Name the repo was registered under
    name: String,
}

pub fn run(args: UnregisterArgs) -> Result<()> {
    let mut config = GlobalConfig::load()?;
    if !config.unregister(&args.name) {
        bail!("no repo named '{}' is registered", args.name);
    }
    config.save()?;
    println!("unregistered '{}'", args.name);
    Ok(())
}
