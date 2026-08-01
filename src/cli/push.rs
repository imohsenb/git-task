use std::cell::RefCell;

use anyhow::{bail, Context, Result};
use clap::Args;

use crate::git;
use crate::store::git_store::Store;

#[derive(Args)]
pub struct PushArgs {
    /// Remote to push to (defaults to "origin")
    remote: Option<String>,
}

pub fn run(args: PushArgs) -> Result<()> {
    let repo = git::repo::discover_current()?;
    let remote_name = args.remote.unwrap_or_else(|| "origin".to_string());
    let mut remote = repo
        .find_remote(&remote_name)
        .with_context(|| format!("no such remote '{remote_name}'"))?;

    let store = Store::new(&repo);
    let ids = store.list_ids()?;
    if ids.is_empty() {
        println!("no tasks to push.");
        return Ok(());
    }

    // Explicit src:dst per task, not a `refs/tasks/*:refs/tasks/*` glob — libgit2's
    // push path (unlike plain `git push`, which handles the glob fine) rejects it.
    let refspecs: Vec<String> = ids.iter().map(|id| format!("refs/tasks/{id}:refs/tasks/{id}")).collect();
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

    let push_result = remote
        .push(&refspec_refs, Some(&mut opts))
        .with_context(|| format!("pushing tasks to '{remote_name}'"));
    drop(opts); // releases the borrow the callback holds on `rejected`
    push_result?;

    let rejected = rejected.into_inner();
    if !rejected.is_empty() {
        bail!(
            "'{remote_name}' rejected {} task ref(s) — run 'git task pull' first:\n{}",
            rejected.len(),
            rejected.join("\n")
        );
    }

    println!("pushed {} task(s) to '{remote_name}'", ids.len());
    Ok(())
}
