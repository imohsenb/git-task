use std::cell::RefCell;

use anyhow::{Context, Result};
use git2::Repository;

use crate::actor::Actor;
use crate::output::{Classify, ClassifiedError};
use crate::store::git_store::{Store, CONFIG_ID};
use crate::store::merge::{self, Outcome};

/// Non-CLI mechanics shared by `cli::push`/`cli::pull` (which format this into text/JSON output)
/// and `sync::worker` (the background auto-sync worker, which has no output to format at all —
/// see `automation::builtins::AUTO_SYNC`). Kept free of `clap::Args`/`output::*` formatting so
/// either caller can drive it without going through argument parsing or a terminal.
pub struct PushResult {
    pub attempted: usize,
    pub has_config_ref: bool,
    pub nothing_to_push: bool,
    /// (ref_name, rejection_message) pairs exactly as returned by git's per-ref push callback —
    /// `None` message means that ref was accepted.
    pub refs: Vec<(String, Option<String>)>,
}

/// Pushes every local task ref (+ the reserved config ref, if present) to `remote_name`.
/// `Err(ClassifiedError::Remote)` if the remote doesn't exist or the transport fails;
/// `Err(ClassifiedError::Rejected)` if the remote rejected any ref (needs a pull first).
pub fn push_all(repo: &Repository, remote_name: &str) -> Result<PushResult> {
    let mut remote = repo.find_remote(remote_name).classify_err(|| ClassifiedError::Remote {
        message: format!("no such remote '{remote_name}'"),
    })?;

    let store = Store::new(repo);
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
        return Ok(PushResult { attempted: 0, has_config_ref: false, nothing_to_push: true, refs: Vec::new() });
    }
    let refspec_refs: Vec<&str> = refspecs.iter().map(String::as_str).collect();

    // `Remote::push`'s `Result` only reports transport-level failures; per-ref
    // rejections (e.g. remote has moved on — needs a `pull` first) come through
    // this callback instead. It fires once per ref regardless of outcome (status is
    // `None` on success), so this doubles as the ref-by-ref result list too.
    let results = RefCell::new(Vec::<(String, Option<String>)>::new());
    let mut callbacks = crate::git::repo::remote_callbacks();
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

    Ok(PushResult { attempted: ids.len(), has_config_ref, nothing_to_push: false, refs: results })
}

pub struct PullResult {
    /// (task id, outcome) for every task ref reconciled — excludes the reserved config ref,
    /// which is reported separately via `config_outcome`.
    pub tasks: Vec<(String, Outcome)>,
    pub config_outcome: Option<Outcome>,
}

/// Fetches `refs/tasks/*` from `remote_name` into a remote-tracking namespace and reconciles
/// each ref against the local store (see `store::merge::reconcile`). `Err(ClassifiedError::Remote)`
/// if the remote doesn't exist or fetch fails.
pub fn pull_all(repo: &Repository, remote_name: &str, author: &Actor) -> Result<PullResult> {
    let mut remote = repo.find_remote(remote_name).classify_err(|| ClassifiedError::Remote {
        message: format!("no such remote '{remote_name}'"),
    })?;

    let remote_prefix = format!("refs/remote-tasks/{remote_name}/");
    let fetch_refspec = format!("refs/tasks/*:{remote_prefix}*");
    let mut opts = git2::FetchOptions::new();
    opts.remote_callbacks(crate::git::repo::remote_callbacks());
    remote.fetch(&[&fetch_refspec], Some(&mut opts), None).classify_err(|| ClassifiedError::Remote {
        message: format!("fetching tasks from '{remote_name}'"),
    })?;

    let store = Store::new(repo);
    let refs = repo.references_glob(&format!("{remote_prefix}*"))?;

    let mut config_outcome: Option<Outcome> = None;
    let mut tasks = Vec::new();

    for r in refs {
        let r = r?;
        let name = r.name().context("remote-tracking ref name is not valid utf-8")?;
        let Some(id) = name.strip_prefix(&remote_prefix) else { continue };
        let remote_tip = r.target().with_context(|| format!("{name} is not a direct reference"))?;

        let outcome = merge::reconcile(&store, &id.to_string(), remote_tip, author)?;
        if id == CONFIG_ID {
            config_outcome = Some(outcome);
            continue;
        }
        tasks.push((id.to_string(), outcome));
    }

    Ok(PullResult { tasks, config_outcome })
}
