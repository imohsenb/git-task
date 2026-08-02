use std::cell::RefCell;

use anyhow::Result;
use clap::Args;

use crate::git;
use crate::logger::Logger;
use crate::output::{Classify, ClassifiedError};
use crate::store::git_store::{Store, CONFIG_ID};

#[derive(Args)]
pub struct PushArgs {
    /// Remote to push to (defaults to "origin")
    remote: Option<String>,
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
    if store.find_tip(CONFIG_ID)?.is_some() {
        refspecs.push(format!("refs/tasks/{CONFIG_ID}:refs/tasks/{CONFIG_ID}"));
    }
    if refspecs.is_empty() {
        Logger::warn("nothing to push", Some(&format!("no local tasks differ from '{remote_name}'")), &[]);
        return Ok(());
    }
    let refspec_refs: Vec<&str> = refspecs.iter().map(String::as_str).collect();

    // `Remote::push`'s `Result` only reports transport-level failures; per-ref
    // rejections (e.g. remote has moved on — needs a `pull` first) come through
    // this callback instead.
    let rejected = RefCell::new(Vec::<String>::new());
    let mut callbacks = git::repo::remote_callbacks();
    callbacks.push_update_reference(|refname, status| {
        if let Some(msg) = status {
            rejected.borrow_mut().push(format!("{refname}: {msg}"));
        }
        Ok(())
    });

    let mut opts = git2::PushOptions::new();
    opts.remote_callbacks(callbacks);

    let push_result = remote.push(&refspec_refs, Some(&mut opts)).classify_err(|| ClassifiedError::Remote {
        message: format!("pushing tasks to '{remote_name}'"),
    });
    drop(opts); // releases the borrow the callback holds on `rejected`
    push_result?;

    let rejected = rejected.into_inner();
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

    Logger::info(&format!("Pushed {} task(s) to '{remote_name}'", ids.len()), None, &[]);
    Ok(())
}
