use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::config::global::GlobalConfig;
use crate::domain::remote;
use crate::git;
use crate::output::ClassifiedError;

/// A `--repo` argument (`link add`/`epic add` and their `rm` counterparts), resolved and — for
/// the one form that's actually checkable locally — validated. Shared by `cli::link` and
/// `cli::epic` so both commands agree on what "a repo argument" means and fail the same way on
/// one that can't be resolved at all, rather than each maintaining its own copy.
pub struct ResolvedRepo {
    /// What gets persisted as `target_repo`/`parent_repo`: a registered repo's `origin` remote
    /// URL when known (portable across machines), else its local filesystem path. For a raw
    /// URL argument, the URL itself, verbatim.
    pub identifier: String,
    /// The project this repo is registered under, when known. `None` for a raw path/URL that
    /// resolved but doesn't correspond to any registered repo — `epic`'s same-project check
    /// treats that as unresolved-for-its-purposes (register it first), while `link` (which
    /// carries no project constraint) is untroubled by it.
    pub project: Option<String>,
    /// Best-known local path to open this repo directly, when one is known.
    pub local_path: Option<PathBuf>,
}

/// Resolves `raw` — a registered repo name, a local filesystem path, or a remote URL — against
/// the repo registry.
///
/// A registered name always resolves (the registry vouches for it, even if its local path isn't
/// currently reachable — same tolerance `ls --all` gives an offline registered repo). A URL is
/// *never* validated against the network — matching prior behavior, it's stored/compared
/// verbatim regardless of whether any registered repo's `origin` happens to match it; this is
/// intentional (see `cross_repo_link_matches_across_equivalent_url_forms` in `tests/basic.rs`),
/// not a gap. A bare local path is the one form that's actually checkable without a network
/// round trip, so it's the one form this now hard-fails on when it doesn't exist — the old
/// behavior silently stored whatever string it was given, including typos, as if it were a
/// valid target.
pub fn resolve(raw: &str, config: &mut GlobalConfig) -> Result<ResolvedRepo> {
    if let Some(entry) = config.repos.get(raw) {
        let project = Some(entry.project.clone());
        if let Some(url) = &entry.remote {
            return Ok(ResolvedRepo { identifier: url.clone(), project, local_path: Some(entry.path.clone()) });
        }
        let path = entry.path.clone();
        if path.exists() {
            if let Ok(other_repo) = git::repo::open(&path) {
                if let Some(url) = git::repo::origin_url(&other_repo) {
                    if let Some(e) = config.repos.get_mut(raw) {
                        e.remote = Some(url.clone());
                    }
                    let _ = config.save();
                    return Ok(ResolvedRepo { identifier: url, project, local_path: Some(path) });
                }
            }
        }
        return Ok(ResolvedRepo { identifier: path.display().to_string(), project, local_path: Some(path) });
    }

    if remote::looks_like_url(raw) {
        let matched_path = config.path_for_remote(raw).map(Path::to_path_buf);
        let project = matched_path
            .as_ref()
            .and_then(|p| config.repos.values().find(|e| &e.path == p).map(|e| e.project.clone()));
        return Ok(ResolvedRepo { identifier: raw.to_string(), project, local_path: matched_path });
    }

    let path = Path::new(raw);
    if !path.exists() {
        return Err(unresolved(raw));
    }
    let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let project = config.repos.values().find(|e| e.path == canon).map(|e| e.project.clone());
    if let Ok(other_repo) = git::repo::open(&canon) {
        if let Some(url) = git::repo::origin_url(&other_repo) {
            return Ok(ResolvedRepo { identifier: url, project, local_path: Some(canon) });
        }
    }
    Ok(ResolvedRepo { identifier: canon.display().to_string(), project, local_path: Some(canon) })
}

fn unresolved(raw: &str) -> anyhow::Error {
    anyhow::Error::new(ClassifiedError::NotFound {
        message: format!(
            "cannot resolve repo '{raw}': not a registered repo name, and no local path exists there — register it first with 'git task register' (run from inside it), or double-check the path"
        ),
        query: raw.to_string(),
        entity: "repo".to_string(),
    })
}
