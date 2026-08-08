use std::process::Command;

use anyhow::{Context, Result};

use crate::web::paths;

/// Reads the installed git-task-web's own `package.json`. `None` if it isn't installed or the
/// file can't be parsed — either way there's nothing to compare against, not an error.
pub fn installed_version() -> Result<Option<String>> {
    let pkg_json = paths::install_dir()?.join("node_modules").join("git-task-web").join("package.json");
    if !pkg_json.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&pkg_json).with_context(|| format!("reading {}", pkg_json.display()))?;
    let value: serde_json::Value =
        serde_json::from_str(&text).with_context(|| format!("parsing {}", pkg_json.display()))?;
    Ok(value.get("version").and_then(|v| v.as_str()).map(str::to_string))
}

/// The latest version published on npm, via the same `npm` binary `install::install` already
/// shells out to (no new HTTP-client dependency, same proxy/registry config either way). `None`
/// on any failure — offline, npm unreachable, registry down — since this only ever gates an
/// optional prompt, never blocks `start`/`upgrade` from proceeding on the currently installed
/// version.
pub fn latest_version() -> Option<String> {
    let output = Command::new("npm").args(["view", "git-task-web", "version"]).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let version = String::from_utf8(output.stdout).ok()?;
    let version = version.trim();
    (!version.is_empty()).then(|| version.to_string())
}

/// Naive numeric `major.minor.patch` compare — good enough since git-task-web's own CI enforces
/// monotonic version bumps and publishes no prerelease tags.
pub fn is_newer(current: &str, latest: &str) -> bool {
    fn parts(v: &str) -> [u64; 3] {
        let mut out = [0u64; 3];
        for (i, p) in v.split('.').take(3).enumerate() {
            out[i] = p.parse().unwrap_or(0);
        }
        out
    }
    parts(latest) > parts(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_newer_compares_numerically_not_lexically() {
        assert!(is_newer("0.1.9", "0.1.10"));
        assert!(is_newer("0.1.2", "0.2.0"));
        assert!(is_newer("0.1.2", "1.0.0"));
        assert!(!is_newer("0.1.2", "0.1.2"));
        assert!(!is_newer("0.1.2", "0.1.1"));
    }

    #[test]
    fn is_newer_treats_missing_or_garbage_segments_as_zero() {
        assert!(is_newer("1", "1.0.1"));
        assert!(!is_newer("1.2.3", "1.2.x"));
    }
}
