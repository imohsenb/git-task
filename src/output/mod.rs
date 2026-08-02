//! Machine-readable output: the `--format json` response envelope, error classification, and
//! process-wide state (format mode, invoked command path, collected warnings) that every
//! subcommand's `run()` reads from or writes into. Text-mode output is untouched by any of this
//! — `is_json()` gates every call site that would otherwise print JSON or suppress a human line.
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::Serialize;

mod error;

pub use error::{Classify, ClassifiedError, CliError, CliErrorKind, ContextValue};

/// clap's `ValueEnum` derive defaults to kebab-case value strings, so these two single-word
/// variants already parse as `text`/`json` with no rename attribute needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Format {
    Text,
    Json,
}

static FORMAT: OnceLock<Format> = OnceLock::new();
static COMMAND: OnceLock<std::sync::Mutex<String>> = OnceLock::new();

thread_local! {
    static WARNINGS: RefCell<Vec<CliWarning>> = const { RefCell::new(Vec::new()) };
}

/// Set once, early, from `Cli::run` — before any subcommand's `run()` executes.
pub fn init_format(format: Format) {
    let _ = FORMAT.set(format);
}

pub fn is_json() -> bool {
    matches!(FORMAT.get(), Some(Format::Json))
}

/// The invoked command path, space-joined (`"ls"`, `"config show"`, `"project create"`). Each
/// leaf `xxx::run()` sets this itself (it's the only place that knows the full sub-action path
/// for nested subcommands like `config rule add`), so it's a plain overwritable cell rather than
/// a `OnceLock` — a coarse name set by a dispatcher would otherwise block a more specific one.
pub fn set_command(name: impl Into<String>) {
    let cell = COMMAND.get_or_init(|| std::sync::Mutex::new(String::new()));
    *cell.lock().expect("command name mutex poisoned") = name.into();
}

pub fn command() -> String {
    COMMAND.get().map(|m| m.lock().expect("command name mutex poisoned").clone()).unwrap_or_default()
}

#[derive(Serialize)]
pub struct CliWarning {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

/// Records a warning. In JSON mode it's collected into the envelope's `warnings[]`; in text mode
/// it's printed exactly as `Logger::warn` always has been — one call site, so callers don't have
/// to branch on format themselves.
pub fn warn(message: &str, detail: Option<&str>, scope: Option<&str>) {
    if is_json() {
        WARNINGS.with(|w| {
            w.borrow_mut().push(CliWarning {
                message: message.to_string(),
                detail: detail.map(str::to_string),
                scope: scope.map(str::to_string),
            })
        });
    } else {
        crate::logger::Logger::warn(message, detail, &[]);
    }
}

fn take_warnings() -> Vec<CliWarning> {
    WARNINGS.with(|w| std::mem::take(&mut *w.borrow_mut()))
}

#[derive(Serialize)]
pub struct CliOk<T: Serialize> {
    pub ok: bool,
    pub command: String,
    pub version: String,
    pub data: T,
    pub warnings: Vec<CliWarning>,
}

#[derive(Serialize)]
pub struct CliErr {
    pub ok: bool,
    pub command: String,
    pub version: String,
    pub error: CliError,
    pub warnings: Vec<CliWarning>,
}

/// The one shape every `--format json` invocation prints on stdout — success or failure. Not
/// actually constructed directly (`print_ok`/`print_err` build `CliOk`/`CliErr` and serialize
/// them without this wrapper), but documents the two shapes a consumer needs to discriminate on
/// `ok`.
#[derive(Serialize)]
#[serde(untagged)]
#[allow(dead_code)]
pub enum CliResponse<T: Serialize> {
    Ok(CliOk<T>),
    Err(CliErr),
}

fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Prints the success envelope to stdout. The only stdout write a command makes in JSON mode.
pub fn print_ok<T: Serialize>(data: T) {
    let response = CliOk { ok: true, command: command(), version: version(), data, warnings: take_warnings() };
    match serde_json::to_string_pretty(&response) {
        Ok(text) => println!("{text}"),
        Err(err) => eprintln!("git-task: failed to serialize JSON response: {err}"),
    }
}

/// Prints the error envelope to stdout and returns — the caller (`lib::run`) still exits 1 and
/// may still print the human `✖ Error:` line to stderr afterward.
pub fn print_err(err: &anyhow::Error) {
    let message = err.to_string();
    let causes: Vec<String> = err.chain().skip(1).map(|e| e.to_string()).collect();
    let (kind, context) = classify(err);
    let response = CliErr {
        ok: false,
        command: command(),
        version: version(),
        error: CliError { kind, message, causes, context },
        warnings: take_warnings(),
    };
    match serde_json::to_string_pretty(&response) {
        Ok(text) => println!("{text}"),
        Err(serde_err) => eprintln!("git-task: failed to serialize JSON error response: {serde_err}"),
    }
}

fn classify(err: &anyhow::Error) -> (CliErrorKind, Option<BTreeMap<String, ContextValue>>) {
    for cause in err.chain() {
        if let Some(classified) = cause.downcast_ref::<ClassifiedError>() {
            let ctx = classified.context_map();
            return (classified.kind(), (!ctx.is_empty()).then_some(ctx));
        }
    }
    (CliErrorKind::Internal, None)
}
