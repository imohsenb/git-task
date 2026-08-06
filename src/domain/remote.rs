//! Protocol/host-agnostic comparison of git remote URLs. Two clones of the same repo
//! commonly have different `origin` URLs (ssh vs https vs scp-like), and self-hosted
//! backends add their own quirks (Gerrit's HTTP form carries a `/a/` auth-prefix segment
//! its ssh form lacks) — `normalize` reduces any of these to one comparison key so
//! cross-repo task links match regardless of which form was typed or captured.

/// True for anything that looks like a git remote URL (scheme-based, or scp-like
/// `user@host:path`) rather than a bare local filesystem path.
pub fn looks_like_url(s: &str) -> bool {
    if s.contains("://") {
        return true;
    }
    if let Some(at) = s.find('@') {
        if let Some(colon_rel) = s[at..].find(':') {
            let colon = at + colon_rel;
            return !s[colon + 1..].starts_with("//");
        }
    }
    false
}

/// Reduces a git remote URL to a `host/path` comparison key, stripping scheme, embedded
/// credentials, port, a Gerrit-style leading `/a/` auth segment, and a trailing `.git` — so
/// the same repo cloned over ssh/https/scp-like syntax, on GitHub, GitLab (including
/// subgroups), Bitbucket, Gerrit, or any other standard git host, normalizes to one key.
/// Not a general-purpose URL parser — just enough of git's own remote-URL grammar
/// (https://git-scm.com/docs/git-clone#URLS) to dedupe the forms that commonly point at the
/// same repo. Host is lowercased (case-insensitive per DNS); path segments are left as-is
/// (path case-sensitivity varies by backend, so this can't safely lowercase it).
pub fn normalize(url: &str) -> String {
    let mut s = url.trim().to_string();

    // scp-like syntax (`user@host:path`, no scheme) -> rewrite the host/path separator to
    // '/' so the rest of the pipeline treats it the same as every other form.
    if !s.contains("://") {
        if let Some(at) = s.find('@') {
            if let Some(colon_rel) = s[at..].find(':') {
                let colon = at + colon_rel;
                if !s[colon + 1..].starts_with("//") {
                    s.replace_range(colon..colon + 1, "/");
                }
            }
        }
    }

    // strip a leading `scheme://`
    if let Some((_, rest)) = s.split_once("://") {
        s = rest.to_string();
    }

    // strip leading `user@`/`user:pass@` credentials, up to the first '/'
    let host_end = s.find('/').unwrap_or(s.len());
    if let Some(at) = s[..host_end].rfind('@') {
        s = s[at + 1..].to_string();
    }

    // split host[:port] from path, dropping the port
    let split = s.find('/').unwrap_or(s.len());
    let host_port = &s[..split];
    let host = host_port.split(':').next().unwrap_or(host_port).to_ascii_lowercase();

    let mut path = s[split..].trim_start_matches('/');
    // Gerrit's HTTP form often prefixes the project path with an `a/` auth segment its
    // ssh:// form lacks (`https://gerrit.example.com/a/project/repo` vs
    // `ssh://.../project/repo`) — strip it so both forms match.
    if let Some(rest) = path.strip_prefix("a/") {
        path = rest;
    }
    let path = path.trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);

    format!("{host}/{path}")
}

/// `None == None` (both same-repo/local), or both `Some` and `normalize` agrees — the shared
/// comparison behind `Link::same_target_repo` and the equivalent check for a cross-repo epic
/// parent, so a repo recorded via one URL/path form is still recognized via any equivalent one.
pub fn same(a: &Option<String>, b: &Option<String>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(x), Some(y)) => normalize(x) == normalize(y),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scp_like_and_https_match() {
        assert_eq!(normalize("git@github.com:org/repo.git"), normalize("https://github.com/org/repo.git"));
    }

    #[test]
    fn ssh_scheme_matches_https() {
        assert_eq!(normalize("ssh://git@github.com/org/repo"), normalize("https://github.com/org/repo.git"));
    }

    #[test]
    fn gitlab_subgroup_path_preserved() {
        assert_eq!(
            normalize("git@gitlab.com:group/subgroup/repo.git"),
            normalize("https://gitlab.com/group/subgroup/repo.git")
        );
        assert_eq!(normalize("https://gitlab.com/group/subgroup/repo"), "gitlab.com/group/subgroup/repo");
    }

    #[test]
    fn bitbucket_scp_like_matches_https() {
        assert_eq!(
            normalize("git@bitbucket.org:org/repo.git"),
            normalize("https://bitbucket.org/org/repo.git")
        );
    }

    #[test]
    fn gerrit_ssh_port_matches_https_auth_prefix() {
        assert_eq!(
            normalize("ssh://user@gerrit.example.com:29418/project/repo"),
            normalize("https://gerrit.example.com/a/project/repo")
        );
    }

    #[test]
    fn host_is_case_insensitive() {
        assert_eq!(normalize("https://GitHub.com/org/repo"), normalize("https://github.com/org/repo"));
    }

    #[test]
    fn different_repos_do_not_match() {
        assert_ne!(normalize("https://github.com/org/repo-a"), normalize("https://github.com/org/repo-b"));
        assert_ne!(normalize("https://github.com/org/repo"), normalize("https://gitlab.com/org/repo"));
    }

    #[test]
    fn looks_like_url_distinguishes_paths_from_urls() {
        assert!(looks_like_url("https://github.com/org/repo.git"));
        assert!(looks_like_url("git@github.com:org/repo.git"));
        assert!(!looks_like_url("/Users/alice/dev/backend"));
        assert!(!looks_like_url("../backend"));
    }
}
