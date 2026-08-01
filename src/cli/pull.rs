use anyhow::{Context, Result};
use clap::Args;

use crate::actor::Actor;
use crate::git;
use crate::store::git_store::Store;
use crate::store::merge::{self, Outcome};

#[derive(Args)]
pub struct PullArgs {
    /// Remote to pull from (defaults to "origin")
    remote: Option<String>,
}

pub fn run(args: PullArgs) -> Result<()> {
    let repo = git::repo::discover_current()?;
    let author = Actor::from_repo(&repo)?;
    let remote_name = args.remote.unwrap_or_else(|| "origin".to_string());

    let mut remote = repo
        .find_remote(&remote_name)
        .with_context(|| format!("no such remote '{remote_name}'"))?;

    let remote_prefix = format!("refs/remote-tasks/{remote_name}/");
    let fetch_refspec = format!("refs/tasks/*:{remote_prefix}*");
    let mut opts = git2::FetchOptions::new();
    opts.remote_callbacks(git::repo::remote_callbacks());
    remote
        .fetch(&[&fetch_refspec], Some(&mut opts), None)
        .with_context(|| format!("fetching tasks from '{remote_name}'"))?;

    let store = Store::new(&repo);
    let refs = repo.references_glob(&format!("{remote_prefix}*"))?;

    let (mut new_count, mut ff_count, mut merged_count, mut up_to_date_count) = (0, 0, 0, 0);

    for r in refs {
        let r = r?;
        let name = r.name().context("remote-tracking ref name is not valid utf-8")?;
        let Some(id) = name.strip_prefix(&remote_prefix) else { continue };
        let remote_tip = r
            .target()
            .with_context(|| format!("{name} is not a direct reference"))?;

        match merge::reconcile(&store, &id.to_string(), remote_tip, &author)? {
            Outcome::New => new_count += 1,
            Outcome::FastForwarded => ff_count += 1,
            Outcome::Merged => merged_count += 1,
            Outcome::UpToDate => up_to_date_count += 1,
        }
    }

    println!(
        "pulled from '{remote_name}': {new_count} new, {ff_count} fast-forwarded, {merged_count} merged, {up_to_date_count} up to date"
    );
    Ok(())
}
