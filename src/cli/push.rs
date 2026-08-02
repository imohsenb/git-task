use std::cell::RefCell;

use anyhow::Result;
use clap::Args;
use serde::Serialize;

use crate::config::project;
use crate::domain::id;
use crate::git;
use crate::logger::Logger;
use crate::output::{self, Classify, ClassifiedError};
use crate::store::git_store::{Store, CONFIG_ID};

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
    let mut remote = repo.find_remote(&remote_name).classify_err(|| ClassifiedError::Remote {
        message: format!("no such remote '{remote_name}'"),
    })?;

    let store = Store::new(&repo);
    let ids = store.list_ids()?;

    // Explicit src:dst per ref, not a `refs/tasks/*:refs/tasks/*` glob — libgit2's
    // push path (unlike plain `git push`, which handles the glob fine) rejects it.
    let mut refspecs: Vec<String> =
        ids.iter().map(|id| format!("refs/tasks/{id}:refs/tasks/{id}")).collect();
    // The per-repo config lives at the reserved `refs/tasks/config` (excluded from `list_ids`),
    // so push it explicitly when it exists.
    let has_config_ref = store.find_tip(CONFIG_ID)?.is_some();
    if has_config_ref {
        refspecs.push(format!("refs/tasks/{CONFIG_ID}:refs/tasks/{CONFIG_ID}"));
    }
    if refspecs.is_empty() {
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
    let refspec_refs: Vec<&str> = refspecs.iter().map(String::as_str).collect();

    // `Remote::push`'s `Result` only reports transport-level failures; per-ref
    // rejections (e.g. remote has moved on — needs a `pull` first) come through
    // this callback instead. It fires once per ref regardless of outcome (status is
    // `None` on success), so this doubles as the source of the JSON `refs` list too.
    let results = RefCell::new(Vec::<(String, Option<String>)>::new());
    let mut callbacks = git::repo::remote_callbacks();
    callbacks.push_update_reference(|refname, status| {
        results.borrow_mut().push((refname.to_string(), status.map(str::to_string)));
        Ok(())
    });

    let mut opts = git2::PushOptions::new();
    opts.remote_callbacks(callbacks);

    let push_result = remote.push(&refspec_refs, Some(&mut opts)).classify_err(|| ClassifiedError::Remote {
        message: format!("pushing tasks to '{remote_name}'"),
    });
    drop(opts); // releases the borrow the callback holds on `results`
    push_result?;

    let results = results.into_inner();
    let rejected: Vec<String> = results
        .iter()
        .filter_map(|(name, status)| status.as_ref().map(|msg| format!("{name}: {msg}")))
        .collect();
    if !rejected.is_empty() {
        return Err(anyhow::Error::new(ClassifiedError::Rejected {
            message: format!(
                "'{remote_name}' rejected {} task ref(s) — run 'git task pull' first:\n{}",
                rejected.len(),
                rejected.join("\n")
            ),
            refs: rejected,
        }));
    }

    if output::is_json() {
        let key = project::effective_key_for(&repo)?;
        let config_ref_name = format!("refs/tasks/{CONFIG_ID}");
        let refs: Vec<PushRefJson> = results
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
            attempted: ids.len(),
            pushed: refs.len(),
            config_ref_pushed: has_config_ref,
            nothing_to_push: false,
            refs,
            rejected: Vec::new(),
        });
        return Ok(());
    }

    Logger::info(&format!("Pushed {} task(s) to '{remote_name}'", ids.len()), None, &[]);
    Ok(())
}
