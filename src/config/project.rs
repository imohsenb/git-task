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

fn default_key(workdir: &Path) -> String {
    let name = workdir.file_name().and_then(|n| n.to_str()).unwrap_or("task");
    let key: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if key.is_empty() {
        "TASK".to_string()
    } else {
        key
    }
}
