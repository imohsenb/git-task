use anyhow::Result;
use clap::Args;
use serde::Serialize;

use crate::config::project;
use crate::domain::id;
use crate::git;
use crate::logger::Logger;
use crate::output;
use crate::store::git_store::CONFIG_ID;
use crate::store::remote;

#[derive(Args)]
pub struct PushArgs {
    /// Remote to push to (defaults to "origin")
    remote: Option<String>,
}

#[derive(Serialize)]
struct PushRefJson {
    #[serde(rename = "ref")]
    ref_name: String,
    task_id: String,
    display_id: String,
    status: &'static str,
    message: Option<String>,
}

#[derive(Serialize)]
struct PushJson {
    remote: String,
    attempted: usize,
    pushed: usize,
    config_ref_pushed: bool,
    nothing_to_push: bool,
    refs: Vec<PushRefJson>,
    rejected: Vec<PushRefJson>,
}

pub fn run(args: PushArgs) -> Result<()> {
    let repo = git::repo::discover_current()?;
    let remote_name = args.remote.unwrap_or_else(|| "origin".to_string());
    let result = remote::push_all(&repo, &remote_name)?;

    if result.nothing_to_push {
        if output::is_json() {
            output::print_ok(PushJson {
                remote: remote_name,
                attempted: 0,
                pushed: 0,
                config_ref_pushed: false,
                nothing_to_push: true,
                refs: Vec::new(),
                rejected: Vec::new(),
            });
        } else {
            Logger::warn("nothing to push", Some(&format!("no local tasks differ from '{remote_name}'")), &[]);
        }
        return Ok(());
    }

    if output::is_json() {
        let key = project::effective_key_for(&repo)?;
        let config_ref_name = format!("refs/tasks/{CONFIG_ID}");
        let refs: Vec<PushRefJson> = result
            .refs
            .into_iter()
            .filter(|(name, _)| *name != config_ref_name)
            .map(|(name, status)| {
                let task_id = name.strip_prefix("refs/tasks/").unwrap_or(&name).to_string();
                PushRefJson {
                    ref_name: name,
                    display_id: id::display(&key, &task_id),
                    task_id,
                    status: if status.is_none() { "ok" } else { "rejected" },
                    message: status,
                }
            })
            .collect();
        output::print_ok(PushJson {
            remote: remote_name,
            attempted: result.attempted,
            pushed: refs.len(),
            config_ref_pushed: result.has_config_ref,
            nothing_to_push: false,
            refs,
            rejected: Vec::new(),
        });
        return Ok(());
    }

    Logger::info(&format!("Pushed {} task(s) to '{remote_name}'", result.attempted), None, &[]);
    Ok(())
}
