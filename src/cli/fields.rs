use anyhow::Result;
use clap::Args;

#[derive(Args)]
pub struct FieldsArgs {}

// Read-only alias for the field view of `git task config show`. Set fields with
// `git task config field <name> required|optional`.
pub fn run(_args: FieldsArgs) -> Result<()> {
    crate::cli::config::show_fields()
}
