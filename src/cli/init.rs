use anyhow::Result;
use clap::Args;

use crate::cli::config;
use crate::config::config_op::ConfigOp;
use crate::config::global::GlobalConfig;
use crate::config::project::{self, ProjectConfig};
use crate::git;
use crate::cli::wizard;
use crate::output::{self, ClassifiedError};

#[derive(Args)]
pub struct InitArgs {}

/// Interactive setup: asks for the address key and required fields (writing them to the
/// event-sourced config ref, `refs/tasks/config` — no working-tree footprint), offers to register
/// the repo in the user-level config, and offers to hand off into the automation-rule wizard.
pub fn run(_args: InitArgs) -> Result<()> {
    // Entirely interactive, no flag-based form at all — a JSON caller can't answer any of its
    // prompts, so refuse outright rather than blocking on stdin or leaking plain-text prompts.
    if output::is_json() {
        return Err(anyhow::Error::new(ClassifiedError::Validation {
            message: "init is interactive-only; use 'config key'/'config field'/'register' under --format json".to_string(),
            field: None,
            missing: Vec::new(),
        }));
    }

    let repo = git::repo::discover_current()?;
    let workdir = git::repo::workdir(&repo)?;
    let cfg = ProjectConfig::load(&repo)?;

    println!("setting up git-task for {}", workdir.display());

    let default_key = cfg.effective_key(&workdir);
    let key = loop {
        let raw = wizard::prompt_default("project key (e.g. SRV)", &default_key)?;
        let key = raw.to_ascii_uppercase();
        let valid = key.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
            && key.chars().all(|c| c.is_ascii_alphanumeric());
        if !valid {
            println!("key must start with a letter and contain only letters/digits");
            continue;
        }
        break key;
    };

    let mut ops = vec![ConfigOp::SetKey { key: key.clone() }];
    for field in ["priority", "assignee", "due"] {
        let currently_required = cfg.fields.get(field).is_some_and(|f| f.required);
        let required =
            wizard::prompt_yn(&format!("require '{field}' on new tasks?"), currently_required)?;
        ops.push(ConfigOp::SetFieldRequired { field: field.to_string(), required });
    }

    project::append_ops(&repo, ops)?;
    println!("saved config to refs/tasks/config (key={key})");

    if wizard::prompt_yn("register this repo in your global config now?", true)? {
        let default_name = workdir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "repo".to_string());
        let name = wizard::prompt_default("repo name", &default_name)?;
        let mut global = GlobalConfig::load()?;
        let default_project = global.default_project.clone();
        let project = wizard::prompt_default("project group", &default_project)?;
        let remote = git::repo::origin_url(&repo);
        match global.register(name.clone(), workdir.clone(), Some(project), remote) {
            Ok(project) => {
                global.save()?;
                println!("registered '{name}' in project '{project}'");
            }
            Err(err) => println!("skipped registration: {err:#}"),
        }
    }

    if wizard::prompt_yn("add an automation rule now?", false)? {
        config::add_interactive()?;
    }

    Ok(())
}
