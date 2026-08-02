use std::path::Path;

use anyhow::{Context, Result};
use git2::Repository;
use serde::{Deserialize, Serialize};

use crate::actor::Actor;
use crate::automation::rules::Rule;
use crate::config::config_op::{self, ConfigOp, ConfigOpEnvelope};
use crate::config::fields::FieldMap;
use crate::store::git_store::{Store, CONFIG_ID};

/// Per-repo config — the derived read model of the event-sourced config op-chain stored under
/// `refs/tasks/config` (see `config::config_op`). It travels with the tasks via the same
/// push/pull/clone refspecs, so a clone needs no source checkout and the repo carries no
/// `.gittask/` working-tree footprint. Edited only through the CLI (`git task config ...`),
/// never by hand.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectConfig {
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "FieldMap::is_empty")]
    pub fields: FieldMap,
    #[serde(default, rename = "rule", skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<Rule>,
}

impl ProjectConfig {
    /// Folds the config op-chain from `refs/tasks/config`. Returns the default (empty) config
    /// when the ref doesn't exist yet — i.e. the repo hasn't been configured.
    pub fn load(repo: &Repository) -> Result<Self> {
        let store = Store::new(repo);
        if store.find_tip(CONFIG_ID)?.is_none() {
            return Ok(Self::default());
        }
        let mut envelopes: Vec<ConfigOpEnvelope> = Vec::new();
        for blob in store.read_chain(CONFIG_ID, config_op::BLOB_NAME)? {
            let text = std::str::from_utf8(&blob).context("config ops are not valid utf-8")?;
            let batch: Vec<ConfigOpEnvelope> =
                serde_json::from_str(text).context("parsing config ops")?;
            envelopes.extend(batch);
        }
        Ok(config_op::fold(&envelopes))
    }

    pub fn effective_key(&self, workdir: &Path) -> String {
        self.key.clone().unwrap_or_else(|| default_key(workdir))
    }
}

/// Appends config ops to `refs/tasks/config`, attributed to the repo's git user. Creates the
/// ref on the first write, else appends on the current tip (a later `pull` may merge divergent
/// tips exactly as it does for tasks).
pub fn append_ops(repo: &Repository, ops: Vec<ConfigOp>) -> Result<()> {
    let store = Store::new(repo);
    let author = Actor::from_repo(repo)?;
    let ts = time::OffsetDateTime::now_utc().unix_timestamp();
    let message = op_summary(&ops);
    let envelopes: Vec<ConfigOpEnvelope> = ops
        .into_iter()
        .map(|op| ConfigOpEnvelope { author: author.clone(), timestamp: ts, op })
        .collect();
    let bytes = serde_json::to_vec_pretty(&envelopes).context("serializing config ops")?;
    store.append_chain(CONFIG_ID, &author, config_op::BLOB_NAME, &bytes, ts, &message)
}

fn op_summary(ops: &[ConfigOp]) -> String {
    ops.iter()
        .map(|op| match op {
            ConfigOp::SetKey { .. } => "SetKey",
            ConfigOp::SetFieldRequired { .. } => "SetFieldRequired",
            ConfigOp::UpsertRule { .. } => "UpsertRule",
            ConfigOp::RemoveRule { .. } => "RemoveRule",
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Convenience for CLI commands that only need the display key for the
/// current repo, without dealing with `ProjectConfig` directly.
pub fn effective_key_for(repo: &Repository) -> Result<String> {
    let workdir = crate::git::repo::workdir(repo)?;
    let cfg = ProjectConfig::load(repo)?;
    Ok(cfg.effective_key(&workdir))
}

const MAX_DEFAULT_KEY_LEN: usize = 5;

/// Derives a project key from the working directory name, capped at
/// `MAX_DEFAULT_KEY_LEN` chars. The name is split into words on non-alphanumeric
/// delimiters (`-`, `_`, space, ...) and camelCase boundaries, e.g. `ebooklet-api-social`
/// -> `["ebooklet", "api", "social"]`, `GitTask` -> `["Git", "Task"]`. A single word is just
/// truncated (uppercased) to the max length. Multiple words take the first letter of every
/// word except the last, then fill the remaining budget with letters from the last word —
/// so `ebooklet-api-social` -> `EASOC` and `GitTask` -> `GTASK`.
fn default_key(workdir: &Path) -> String {
    let name = workdir.file_name().and_then(|n| n.to_str()).unwrap_or("task");
    let key = key_from_words(&split_words(name));
    if key.is_empty() {
        "TASK".to_string()
    } else {
        key
    }
}

fn split_words(name: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut prev_lower = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            if c.is_ascii_uppercase() && prev_lower && !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            current.push(c);
            prev_lower = c.is_ascii_lowercase();
        } else {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            prev_lower = false;
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

fn key_from_words(words: &[String]) -> String {
    let [only] = words else {
        let Some((last, init)) = words.split_last() else {
            return String::new();
        };
        let mut prefix: String =
            init.iter().filter_map(|w| w.chars().next()).flat_map(|c| c.to_uppercase()).collect();
        if prefix.chars().count() >= MAX_DEFAULT_KEY_LEN {
            prefix.truncate(MAX_DEFAULT_KEY_LEN);
            return prefix;
        }
        let remaining = MAX_DEFAULT_KEY_LEN - prefix.chars().count();
        prefix.extend(last.chars().take(remaining).flat_map(|c| c.to_uppercase()));
        return prefix;
    };
    only.chars().take(MAX_DEFAULT_KEY_LEN).flat_map(|c| c.to_uppercase()).collect()
}

#[cfg(test)]
mod default_key_tests {
    use super::*;

    fn key_for(name: &str) -> String {
        key_from_words(&split_words(name))
    }

    #[test]
    fn multi_section_fills_from_last_word() {
        assert_eq!(key_for("ebooklet-api-social"), "EASOC");
    }

    #[test]
    fn camel_case_splits_into_words() {
        assert_eq!(key_for("GitTask"), "GTASK");
    }

    #[test]
    fn single_word_truncates_to_max_len() {
        assert_eq!(key_for("backend"), "BACKE");
    }

    #[test]
    fn short_single_word_stays_short() {
        assert_eq!(key_for("api"), "API");
    }

    #[test]
    fn many_sections_truncates_prefix() {
        assert_eq!(key_for("a-b-c-d-e-f-g"), "ABCDE");
    }

    #[test]
    fn non_alphanumeric_only_falls_back_to_task() {
        assert_eq!(default_key(Path::new("/tmp/___")), "TASK");
    }
}
