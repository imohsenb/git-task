use std::path::Path;

use git2::Repository;
use serde::Serialize;

/// One identity resolution: what `user.name`/`user.email` this view of the git config chain
/// resolves to, whether both are set (`ok`), and which config level actually supplied them.
/// Three different views of this exist (`repo_level`, `global_level`, `effective`) because a
/// caller writing a task needs to know not just "is identity configured" but "would this
/// specific write succeed, and if not, which file should I tell the user to edit" — see
/// `whoami` and `repos --deep`, the two commands that surface this.
#[derive(Serialize, Clone)]
pub struct IdentityJson {
    pub name: Option<String>,
    pub email: Option<String>,
    pub ok: bool,
    pub source: &'static str,
}

fn from_config(cfg: Option<git2::Config>, source: &'static str) -> IdentityJson {
    let name = cfg.as_ref().and_then(|c| c.get_string("user.name").ok());
    let email = cfg.as_ref().and_then(|c| c.get_string("user.email").ok());
    let ok = name.is_some() && email.is_some();
    IdentityJson { name, email, ok, source: if ok { source } else { "none" } }
}

fn defines_user_name(path: &Path) -> bool {
    git2::Config::open(path).ok().and_then(|c| c.get_string("user.name").ok()).is_some()
}

/// Just this repo's own `.git/config` — no global/system fallback. This is what `whoami`'s
/// `repo` field reports, so a caller can see whether the repo overrides identity at all.
pub fn repo_level(repo: &Repository) -> IdentityJson {
    from_config(git2::Config::open(&repo.path().join("config")).ok(), "repo")
}

/// The user-level config alone — global falling back to system, no repo layer. `whoami`'s
/// `global` field.
pub fn global_level() -> IdentityJson {
    if let Ok(path) = git2::Config::find_global() {
        let identity = from_config(git2::Config::open(&path).ok(), "global");
        if identity.ok {
            return identity;
        }
    }
    if let Ok(path) = git2::Config::find_system() {
        let identity = from_config(git2::Config::open(&path).ok(), "system");
        if identity.ok {
            return identity;
        }
    }
    IdentityJson { name: None, email: None, ok: false, source: "none" }
}

/// What `Actor::from_repo` would actually resolve to — libgit2's real merged cascade (repo
/// overrides global overrides system), the same lookup a write uses. `source` is a best-effort
/// label of which level's `user.name` won that cascade, for display only (`repos --deep`,
/// `whoami`'s `effective`) — the resolved `name`/`email` themselves always come from the merged
/// `Repository::config()`, not from re-deriving precedence here.
pub fn effective(repo: &Repository) -> IdentityJson {
    let cfg = repo.config().ok();
    let name = cfg.as_ref().and_then(|c| c.get_string("user.name").ok());
    let email = cfg.as_ref().and_then(|c| c.get_string("user.email").ok());
    let ok = name.is_some() && email.is_some();
    let source = if !ok {
        "none"
    } else if defines_user_name(&repo.path().join("config")) {
        "repo"
    } else if git2::Config::find_global().ok().is_some_and(|p| defines_user_name(&p)) {
        "global"
    } else if git2::Config::find_system().ok().is_some_and(|p| defines_user_name(&p)) {
        "system"
    } else {
        "none"
    };
    IdentityJson { name, email, ok, source }
}
