use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::global::config_dir;

const AUTOMATION_FILE: &str = "automation.toml";

/// `on = "task.created"`, `when = "kind == 'bug'"` (evalexpr, optional — always matches if
/// unset), `do = ["set_priority high", "add_label triage"]` (executed in order).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub name: String,
    pub on: String,
    #[serde(default)]
    pub when: Option<String>,
    #[serde(default, rename = "do")]
    pub actions: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RuleSet {
    #[serde(default, rename = "rule")]
    pub rules: Vec<Rule>,
}

/// Personal rules that apply everywhere, from `~/.config/git-task/automation.toml`.
pub fn load_global() -> Result<Vec<Rule>> {
    let path = config_dir()?.join(AUTOMATION_FILE);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let set: RuleSet =
        toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    Ok(set.rules)
}
