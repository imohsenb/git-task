use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use serde::Serialize;

use crate::git;
use crate::logger::Logger;
use crate::output;

#[derive(Args)]
pub struct SkillsArgs {
    #[command(subcommand)]
    action: SkillsAction,
}

#[derive(Subcommand)]
enum SkillsAction {
    /// Copy the bundled agent skills into coding-agent skill directories
    Install(InstallArgs),
}

#[derive(Args)]
#[command(after_help = "\
Best-effort: only Claude Code's skill directory convention (.claude/skills/<name>/SKILL.md) is
currently known, so that's what gets scanned for and written to. Point --dir at any other
location (a different agent's skills folder, a non-default path) to install there instead —
auto-detection is skipped entirely once --dir is given.

  git task skills install                    # every detected agent's global (~/...) skills dir
  git task skills install --project           # ...plus this repo's .claude/skills (shared via git)
  git task skills install --dir ./some/dir    # exactly this directory, nothing auto-detected")]
pub struct InstallArgs {
    /// Also install into this repo's .claude/skills, so it's committed and shared with every clone
    #[arg(long)]
    project: bool,
    /// Install into this exact directory instead of any auto-detected location
    #[arg(long)]
    dir: Option<PathBuf>,
}

/// One skill shipped under `/skills` in the source tree, embedded into the binary so `install`
/// works from a plain `cargo install --locked --path .` with no source checkout on the machine
/// that runs it.
struct BundledSkill {
    dir_name: &'static str,
    content: &'static str,
}

const SKILLS: &[BundledSkill] = &[
    BundledSkill { dir_name: "git-task", content: include_str!("../../skills/git-task/SKILL.md") },
    BundledSkill {
        dir_name: "git-task-config",
        content: include_str!("../../skills/git-task-config/SKILL.md"),
    },
    BundledSkill {
        dir_name: "git-task-sync",
        content: include_str!("../../skills/git-task-sync/SKILL.md"),
    },
];

/// Known coding-agent skill directory conventions, home-relative. Currently just Claude Code —
/// the only one confirmed to use a `<dir>/<skill-name>/SKILL.md` layout — but kept as a table so
/// a second entry (another agent adopting the same open format) is a one-line addition, not a
/// redesign of the scan below.
struct AgentTarget {
    name: &'static str,
    /// Directory under `$HOME` whose presence means this agent is actually set up here.
    marker: &'static str,
    /// Directory under `$HOME` to install skills into.
    skills_dir: &'static str,
}

const KNOWN_AGENTS: &[AgentTarget] =
    &[AgentTarget { name: "Claude Code", marker: ".claude", skills_dir: ".claude/skills" }];

struct Target {
    label: String,
    dir: PathBuf,
    /// Whether this target's marker was actually found on disk, vs. used as a fallback default.
    detected: bool,
}

#[derive(Serialize)]
struct InstalledTargetJson {
    label: String,
    dir: String,
    detected: bool,
    files: Vec<String>,
}

#[derive(Serialize)]
struct SkillsInstallJson {
    installed: Vec<InstalledTargetJson>,
}

pub fn run(args: SkillsArgs) -> Result<()> {
    match args.action {
        SkillsAction::Install(a) => install(a),
    }
}

fn install(args: InstallArgs) -> Result<()> {
    let targets = resolve_targets(&args)?;

    let mut installed = Vec::with_capacity(targets.len());
    for target in targets {
        let files = install_into(&target.dir)?;
        installed.push(InstalledTargetJson {
            label: target.label,
            dir: target.dir.display().to_string(),
            detected: target.detected,
            files,
        });
    }

    if output::is_json() {
        output::print_ok(SkillsInstallJson { installed });
        return Ok(());
    }

    for t in &installed {
        let note = if t.detected {
            String::new()
        } else {
            " (default target — not detected on this machine; pass --dir to target something else)"
                .to_string()
        };
        Logger::info(
            &format!("Installed {} skill(s) to {}{note}", t.files.len(), t.dir),
            Some(&t.label),
            &[],
        );
    }
    Ok(())
}

fn resolve_targets(args: &InstallArgs) -> Result<Vec<Target>> {
    if let Some(dir) = &args.dir {
        return Ok(vec![Target { label: dir.display().to_string(), dir: dir.clone(), detected: true }]);
    }

    let home = directories::BaseDirs::new().context("could not determine home directory")?.home_dir().to_path_buf();

    let detected: Vec<&AgentTarget> = KNOWN_AGENTS.iter().filter(|a| home.join(a.marker).is_dir()).collect();
    let mut targets = if detected.is_empty() {
        // No known agent marker found — still install to the one known convention as a default,
        // since it's overwhelmingly the common case for anyone reaching for this command.
        let default = &KNOWN_AGENTS[0];
        vec![Target { label: default.name.to_string(), dir: home.join(default.skills_dir), detected: false }]
    } else {
        detected
            .into_iter()
            .map(|a| Target { label: a.name.to_string(), dir: home.join(a.skills_dir), detected: true })
            .collect()
    };

    if args.project {
        let repo = git::repo::discover_current()?;
        let workdir = git::repo::workdir(&repo)?;
        targets.push(Target {
            label: "this repo (project)".to_string(),
            dir: workdir.join(".claude/skills"),
            detected: true,
        });
    }

    Ok(targets)
}

/// Writes every bundled skill under `dir/<skill-name>/SKILL.md`, creating directories as needed.
/// Returns the paths written, for reporting.
fn install_into(dir: &Path) -> Result<Vec<String>> {
    let mut files = Vec::with_capacity(SKILLS.len());
    for skill in SKILLS {
        let skill_dir = dir.join(skill.dir_name);
        fs::create_dir_all(&skill_dir).with_context(|| format!("creating {}", skill_dir.display()))?;
        let path = skill_dir.join("SKILL.md");
        fs::write(&path, skill.content).with_context(|| format!("writing {}", path.display()))?;
        files.push(path.display().to_string());
    }
    Ok(files)
}
