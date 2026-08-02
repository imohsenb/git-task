use anyhow::Result;
use clap::Args;

use crate::config::global::GlobalConfig;
use crate::logger::Logger;
use crate::output::ClassifiedError;

#[derive(Args)]
pub struct UnregisterArgs {
    /// Name the repo was registered under
    name: String,
}

pub fn run(args: UnregisterArgs) -> Result<()> {
    let mut config = GlobalConfig::load()?;
    if !config.unregister(&args.name) {
        return Err(anyhow::Error::new(ClassifiedError::NotFound {
            message: format!("no repo named '{}' is registered", args.name),
            query: args.name.clone(),
            entity: "repo".to_string(),
        }));
    }
    config.save()?;
    Logger::info(&format!("Unregistered '{}'", args.name), None, &[]);
    Ok(())
}
