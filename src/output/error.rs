use std::collections::BTreeMap;

use serde::Serialize;

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CliErrorKind {
    NotARepo,
    IdentityMissing,
    NotFound,
    AmbiguousId,
    Validation,
    Conflict,
    Rejected,
    Remote,
    Io,
    Internal,
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum ContextValue {
    Text(String),
    List(Vec<String>),
}

#[derive(Serialize)]
pub struct CliError {
    pub kind: CliErrorKind,
    pub message: String,
    pub causes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<BTreeMap<String, ContextValue>>,
}

/// The classifiable failures this CLI knows how to attribute a `CliErrorKind` to. Every variant
/// carries the exact human message the un-classified code used to build via `anyhow::Context`
/// (`#[error("{message}")]` just replays it), so attaching one of these via `.classify_err()`
/// never changes what a caller sees in text mode — it only adds a downcastable, structured layer
/// `output::classify` can pull `kind`/`context` back out of at the top level. Anything not
/// wrapped in one of these falls back to `CliErrorKind::Internal`, which is fine — see
/// `CLAUDE.md`/the brief: exhaustive classification is explicitly not the goal.
#[derive(Debug, thiserror::Error)]
pub enum ClassifiedError {
    #[error("{message}")]
    NotARepo { message: String, path: String },
    #[error("{message}")]
    IdentityMissing { message: String, path: String, missing: Vec<String>, config_files: Vec<String> },
    #[error("{message}")]
    NotFound { message: String, query: String, entity: String },
    #[error("{message}")]
    AmbiguousId { message: String, query: String, matches: Vec<String> },
    #[error("{message}")]
    Validation { message: String, field: Option<String>, missing: Vec<String> },
    #[error("{message}")]
    Conflict { message: String },
    #[error("{message}")]
    Rejected { message: String, refs: Vec<String> },
    #[error("{message}")]
    Remote { message: String },
    #[error("{message}")]
    Io { message: String },
}

impl ClassifiedError {
    pub fn kind(&self) -> CliErrorKind {
        match self {
            ClassifiedError::NotARepo { .. } => CliErrorKind::NotARepo,
            ClassifiedError::IdentityMissing { .. } => CliErrorKind::IdentityMissing,
            ClassifiedError::NotFound { .. } => CliErrorKind::NotFound,
            ClassifiedError::AmbiguousId { .. } => CliErrorKind::AmbiguousId,
            ClassifiedError::Validation { .. } => CliErrorKind::Validation,
            ClassifiedError::Conflict { .. } => CliErrorKind::Conflict,
            ClassifiedError::Rejected { .. } => CliErrorKind::Rejected,
            ClassifiedError::Remote { .. } => CliErrorKind::Remote,
            ClassifiedError::Io { .. } => CliErrorKind::Io,
        }
    }

    pub fn context_map(&self) -> BTreeMap<String, ContextValue> {
        let mut m = BTreeMap::new();
        match self {
            ClassifiedError::NotARepo { path, .. } => {
                m.insert("path".to_string(), ContextValue::Text(path.clone()));
            }
            ClassifiedError::IdentityMissing { path, missing, config_files, .. } => {
                m.insert("path".to_string(), ContextValue::Text(path.clone()));
                m.insert("missing".to_string(), ContextValue::List(missing.clone()));
                m.insert("config_files".to_string(), ContextValue::List(config_files.clone()));
            }
            ClassifiedError::NotFound { query, entity, .. } => {
                m.insert("query".to_string(), ContextValue::Text(query.clone()));
                m.insert("entity".to_string(), ContextValue::Text(entity.clone()));
            }
            ClassifiedError::AmbiguousId { query, matches, .. } => {
                m.insert("query".to_string(), ContextValue::Text(query.clone()));
                m.insert("matches".to_string(), ContextValue::List(matches.clone()));
            }
            ClassifiedError::Validation { field, missing, .. } => {
                if let Some(f) = field {
                    m.insert("field".to_string(), ContextValue::Text(f.clone()));
                }
                if !missing.is_empty() {
                    m.insert("missing".to_string(), ContextValue::List(missing.clone()));
                }
            }
            ClassifiedError::Rejected { refs, .. } => {
                m.insert("refs".to_string(), ContextValue::List(refs.clone()));
            }
            ClassifiedError::Conflict { .. } | ClassifiedError::Remote { .. } | ClassifiedError::Io { .. } => {}
        }
        m
    }
}

/// Attaches a `ClassifiedError` to an existing error as an `anyhow::Context` layer, without
/// altering what `.to_string()` reports — `ClassifiedError`'s `Display` just replays the
/// `message` the caller already built, so text-mode output is byte-identical whether or not the
/// call site classifies. `output::classify` at the top level finds it later via
/// `err.chain().find_map(downcast_ref)`.
pub trait Classify<T> {
    fn classify_err(self, info: impl FnOnce() -> ClassifiedError) -> anyhow::Result<T>;
}

impl<T, E> Classify<T> for Result<T, E>
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn classify_err(self, info: impl FnOnce() -> ClassifiedError) -> anyhow::Result<T> {
        self.map_err(|e| anyhow::Error::new(e).context(info()))
    }
}
