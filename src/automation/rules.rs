use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::global::config_dir;

const AUTOMATION_FILE: &str = "automation.toml";

/// `on = "task.created"`, `when = "kind == 'bug'"` (evalexpr, optional — always matches if
/// unset), `do = ["set_priority high", "add_label triage"]` (executed in order).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// Rules present in `new` that either didn't exist in `old` or exist there with different
/// content — i.e. project automation a puller hasn't seen before. `project::ProjectConfig`'s
/// rules sync in via the shared `refs/tasks/config` ref like any other task data and start firing
/// on the puller's very next mutating command with no other confirmation step, so `pull` uses
/// this to warn about exactly the rules that just changed instead of staying silent about new
/// automation appearing out of nowhere.
pub fn changed_or_added<'a>(old: &[Rule], new: &'a [Rule]) -> Vec<&'a Rule> {
    new.iter().filter(|r| !old.contains(r)).collect()
}

/// Overwrites `~/.config/git-task/automation.toml` with `rules`, used by the `automation add`
/// wizard. Callers pass the full desired set (load, mutate, save) rather than an append primitive.
pub fn save_global(rules: &[Rule]) -> Result<()> {
    let dir = config_dir()?;
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let path = dir.join(AUTOMATION_FILE);
    let set = RuleSet { rules: rules.to_vec() };
    let text = toml::to_string_pretty(&set).context("serializing automation rules")?;
    std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))
}
