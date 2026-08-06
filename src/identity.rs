use std::collections::HashMap;

use anyhow::Result;
use git2::Repository;

use crate::output::ClassifiedError;

/// Validates CLI-entered assignee input: it must look like a real email address
/// (`local@domain`, both non-empty, domain has a dot). A bare name or handle isn't a stable
/// enough identity to key off across a distributed, event-sourced task store — two people can
/// share a display name, they can't share an email.
pub fn validate_email(input: &str) -> Result<String> {
    let email = input.trim();
    let invalid = || {
        anyhow::Error::new(ClassifiedError::Validation {
            message: format!("assignee must be an email address, got '{email}'"),
            field: Some("assignee".to_string()),
            missing: Vec::new(),
        })
    };
    let Some((local, domain)) = email.split_once('@') else {
        return Err(invalid());
    };
    if local.is_empty() || domain.is_empty() || !domain.contains('.') {
        return Err(invalid());
    }
    Ok(email.to_string())
}

/// Builds an email -> display name lookup by walking every commit reachable from any
/// `refs/tasks/*` ref. Every op ever recorded is committed with its author's name+email as the
/// git commit signature (see `store::git_store::write_commit`), so this ref namespace already
/// *is* a directory of everyone who has ever touched this task store — no separate address book
/// needed. Sorted newest-first, so the most recent name wins per email (picks up a git config
/// name change without rewriting history).
pub fn contributor_directory(repo: &Repository) -> Result<HashMap<String, String>> {
    let mut revwalk = repo.revwalk()?;
    revwalk.set_sorting(git2::Sort::TIME)?;
    for r in repo.references_glob("refs/tasks/*")? {
        if let Some(oid) = r?.target() {
            revwalk.push(oid)?;
        }
    }

    let mut directory = HashMap::new();
    for oid in revwalk {
        let commit = repo.find_commit(oid?)?;
        let author = commit.author();
        if let (Some(name), Some(email)) = (author.name(), author.email()) {
            directory.entry(email.to_string()).or_insert_with(|| name.to_string());
        }
    }
    Ok(directory)
}

/// `contributor_directory`, sorted by email — the list an interactive assignee prompt shows
/// (kept separate from the map above so prompt/menu code doesn't each re-sort it).
pub fn sorted_contributors(repo: &Repository) -> Result<Vec<(String, String)>> {
    let mut list: Vec<(String, String)> = contributor_directory(repo)?.into_iter().collect();
    list.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(list)
}

/// Compact display for table columns: the resolved name if this email has ever authored
/// anything in this task store, else the bare email — someone assigned before they've run a
/// `git-task` command themselves has no name on record yet.
pub fn display_name(directory: &HashMap<String, String>, email: &str) -> String {
    directory.get(email).cloned().unwrap_or_else(|| email.to_string())
}

/// Full "Name <email>" form for detail views; falls back to the bare email when no name is on
/// record yet.
pub fn full_display(directory: &HashMap<String, String>, email: &str) -> String {
    match directory.get(email) {
        Some(name) => format!("{name} <{email}>"),
        None => email.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_email_accepts_local_at_domain_dot_tld() {
        assert_eq!(validate_email("alice@example.com").unwrap(), "alice@example.com");
        assert_eq!(validate_email("  alice@example.com  ").unwrap(), "alice@example.com");
    }

    #[test]
    fn validate_email_rejects_bare_names_and_malformed_addresses() {
        assert!(validate_email("alice").is_err());
        assert!(validate_email("alice@").is_err());
        assert!(validate_email("@example.com").is_err());
        assert!(validate_email("alice@localhost").is_err());
    }

    #[test]
    fn display_name_falls_back_to_email_when_unknown() {
        let directory = HashMap::new();
        assert_eq!(display_name(&directory, "bob@example.com"), "bob@example.com");
    }

    #[test]
    fn full_display_includes_name_when_known() {
        let mut directory = HashMap::new();
        directory.insert("bob@example.com".to_string(), "Bob".to_string());
        assert_eq!(full_display(&directory, "bob@example.com"), "Bob <bob@example.com>");
        assert_eq!(full_display(&directory, "carol@example.com"), "carol@example.com");
    }
}
