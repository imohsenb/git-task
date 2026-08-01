use anyhow::Result;
use clap::Args;

use crate::cli::automation;
use crate::cli::wizard;
use crate::config::fields::FieldSpec;
use crate::config::global::GlobalConfig;
use crate::config::project::ProjectConfig;
use crate::git;

#[derive(Args)]
pub struct InitArgs {}

/// Interactive replacement for hand-editing `.gittask/config.toml` (and, optionally,
/// `~/.config/git-task/config.toml`): asks for the address key and required fields, offers to
/// register the repo, and offers to hand off into the `automation add` wizard.
pub fn run(_args: InitArgs) -> Result<()> {
    let repo = git::repo::discover_current()?;
    let workdir = git::repo::workdir(&repo)?;
    let mut cfg = ProjectConfig::load(&workdir)?;

    println!("setting up git-task for {}", workdir.display());

    let default_key = cfg.effective_key(&workdir);
    cfg.key = Some(loop {
        let raw = wizard::prompt_default("project key (e.g. SRV)", &default_key)?;
        let key = raw.to_ascii_uppercase();
        let valid = key.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
            && key.chars().all(|c| c.is_ascii_alphanumeric());
        if !valid {
            println!("key must start with a letter and contain only letters/digits");
            continue;
        }
        break key;
    });

    for field in ["priority", "assignee", "due"] {
        let currently_required = cfg.fields.get(field).is_some_and(|f| f.required);
        let required =
            wizard::prompt_yn(&format!("require '{field}' on new tasks?"), currently_required)?;
        if required {
            cfg.fields.insert(field.to_string(), FieldSpec { required: true });
        } else {
            cfg.fields.remove(field);
        }
    }

    cfg.save(&workdir)?;
    println!("wrote .gittask/config.toml (key={})", cfg.key.as_deref().unwrap_or(""));

    if wizard::prompt_yn("register this repo in your global config now?", true)? {
        let default_name =
            workdir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "repo".to_string());
        let name = wizard::prompt_default("repo name", &default_name)?;
        let mut global = GlobalConfig::load()?;
        let default_project = global.default_project.clone();
        let project = wizard::prompt_default("project group", &default_project)?;
        match global.register(name.clone(), workdir.clone(), Some(project)) {
            Ok(project) => {
                global.save()?;
                println!("registered '{name}' in project '{project}'");
            }
            Err(err) => println!("skipped registration: {err:#}"),
        }
    }

    if wizard::prompt_yn("add an automation rule now?", false)? {
        automation::add_interactive()?;
    }

    Ok(())
}
