use std::sync::OnceLock;

use crate::color;

struct State {
    bin_name: &'static str,
    disabled: bool,
}

static STATE: OnceLock<State> = OnceLock::new();

/// Call once, early, from `Cli::run` — before any subcommand's `run()` executes. Two
/// independent opt-outs, deliberately not one: `--no-hints` for a single invocation,
/// `GIT_TASK_NO_HINTS` (presence, any value — same convention as `NO_COLOR`) to turn hints
/// off everywhere without having to remember the flag on every call.
pub fn init(bin_name: &'static str, no_hints_flag: bool) {
    let disabled = no_hints_flag || std::env::var_os("GIT_TASK_NO_HINTS").is_some();
    let _ = STATE.set(State { bin_name, disabled });
}

/// Prints a dim "Tip:" block naming likely next commands, e.g. `("show SRV-ab12", "view full
/// details")`. No-op if hints are disabled, `lines` is empty, or `init` was never called (e.g.
/// a unit test invoking a command's `run()` directly without going through `Cli::run`) — hints
/// fail closed, never panic.
pub fn print(lines: &[(String, String)]) {
    let Some(state) = STATE.get() else { return };
    if state.disabled || lines.is_empty() {
        return;
    }

    let full_cmds: Vec<String> = lines.iter().map(|(cmd, _)| format!("{} {cmd}", state.bin_name)).collect();
    let width = full_cmds.iter().map(|c| c.len()).max().unwrap_or(0);

    println!("{}", color::dim_bold(&format!("  Tips:")));
    for (_, (full_cmd, (_, desc))) in full_cmds.iter().zip(lines.iter()).enumerate() {
        println!("{}  {}", 
            color::light_bold(&format!("    {full_cmd:<width$}")),
            color::dim(&format!("{desc}"))
        );
    }
    println!();
}
