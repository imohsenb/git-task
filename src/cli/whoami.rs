use anyhow::Result;
use clap::Args;
use serde::Serialize;

use crate::git;
use crate::output;
use crate::output::identity::{self, IdentityJson};

#[derive(Args)]
pub struct WhoamiArgs {}

#[derive(Serialize)]
struct WhoamiJson {
    #[serde(skip_serializing_if = "Option::is_none")]
    repo: Option<IdentityJson>,
    global: IdentityJson,
    effective: IdentityJson,
}

/// Shows what identity a write would actually be attributed to, before it fails — the frontend's
/// only identity surface, since there is deliberately no `--author` override (identity always
/// comes from git config; see `CLAUDE.md`/the brief on why). `repo` is this repo's own
/// `.git/config` alone, `global` is the user-level config alone, `effective` is what
/// `Actor::from_repo` would really resolve to (the merged cascade a write uses).
pub fn run(_args: WhoamiArgs) -> Result<()> {
    let repo = git::repo::discover_current().ok();
    let global = identity::global_level();
    let effective = match &repo {
        Some(r) => identity::effective(r),
        None => global.clone(),
    };
    let repo_identity = repo.as_ref().map(identity::repo_level);

    if output::is_json() {
        output::print_ok(WhoamiJson { repo: repo_identity, global, effective });
        return Ok(());
    }

    if let Some(r) = &repo_identity {
        print_line("Repo", r);
    }
    print_line("Global", &global);
    print_line("Effective", &effective);
    Ok(())
}

fn print_line(label: &str, identity: &IdentityJson) {
    match (&identity.name, &identity.email) {
        (Some(name), Some(email)) => println!("{label}: {name} <{email}> ({})", identity.source),
        _ => println!("{label}: not configured"),
    }
}
