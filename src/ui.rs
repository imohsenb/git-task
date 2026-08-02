use std::fmt::Display;
use std::sync::OnceLock;

use anyhow::Result;
use chrono::NaiveDate;
use inquire::ui::{calendar::CalendarRenderConfig, Attributes, Color, RenderConfig, StyleSheet, Styled};
use inquire::{DateSelect, InquireError, Select, Text};

use crate::color;
use crate::table;
use crate::wrap;

/// Soft white used for primary prompt text/labels — distinct from `color::light`'s muted ash
/// since this palette is scoped to interactive prompt widgets, not printed output.
const SOFT_WHITE: Color = Color::Rgb { r: 148, g: 163, b: 184 };
/// Dark slate used for secondary text, borders, and defaults across every prompt widget.
const DARK_SLATE: Color = Color::Rgb { r: 71, g: 85, b: 105 };
/// Reuses `color::CYAN_RGB` (the app's one accent color) rather than redefining it, so this
/// palette and the rest of the app's coloring can't drift apart.
const CYAN: Color = Color::Rgb { r: color::CYAN_RGB.0, g: color::CYAN_RGB.1, b: color::CYAN_RGB.2 };

/// Built once and reused by every prompt call — this is the whole point of centralizing the
/// theme here instead of each call site building its own `RenderConfig`.
fn theme() -> RenderConfig<'static> {
    static THEME: OnceLock<RenderConfig<'static>> = OnceLock::new();
    *THEME.get_or_init(|| RenderConfig {
        prompt_prefix: Styled::new("?").with_fg(CYAN),
        answered_prompt_prefix: Styled::new("✓").with_fg(CYAN),
        default_value: StyleSheet::new().with_fg(DARK_SLATE),
        help_message: StyleSheet::new().with_fg(DARK_SLATE),
        text_input: StyleSheet::new().with_fg(SOFT_WHITE),
        answer: StyleSheet::new().with_fg(CYAN).with_attr(Attributes::BOLD),
        highlighted_option_prefix: Styled::new("●").with_fg(CYAN),
        unhighlighted_option_prefix: Styled::new("○").with_fg(DARK_SLATE),
        option: StyleSheet::new().with_fg(SOFT_WHITE),
        selected_option: Some(StyleSheet::new().with_fg(CYAN).with_attr(Attributes::BOLD)),
        calendar: CalendarRenderConfig {
            prefix: Styled::new(">").with_fg(CYAN),
            header: StyleSheet::new().with_fg(SOFT_WHITE).with_attr(Attributes::BOLD),
            week_header: StyleSheet::new().with_fg(DARK_SLATE),
            selected_date: Some(StyleSheet::new().with_fg(Color::Black).with_bg(CYAN)),
            today_date: StyleSheet::new().with_fg(CYAN),
            different_month_date: StyleSheet::new().with_fg(DARK_SLATE),
            unavailable_date: StyleSheet::new().with_fg(DARK_SLATE),
        },
        ..RenderConfig::empty()
    })
}

/// Maps Esc/Ctrl+C to a plain, non-panicking error instead of the library's own message —
/// both terminate the surrounding command the same way any other `bail!` does (printed once
/// by `lib::run` and a non-zero exit), never a panic or a raw stack unwind.
fn map_result<T>(res: std::result::Result<T, InquireError>) -> Result<T> {
    match res {
        Ok(v) => Ok(v),
        Err(InquireError::OperationCanceled) | Err(InquireError::OperationInterrupted) => {
            anyhow::bail!("cancelled")
        }
        Err(err) => Err(err.into()),
    }
}

/// Single-line text prompt, pre-filled with `default_val` — pressing enter keeps it, typing
/// replaces it. `help_text`, if given, renders as a `[bracketed]` hint below the prompt.
pub fn prompt_text(label: &str, default_val: &str, help_text: Option<&str>) -> Result<String> {
    let mut prompt = Text::new(label).with_default(default_val).with_render_config(theme());
    if let Some(help) = help_text {
        prompt = prompt.with_help_message(help);
    }
    map_result(prompt.prompt())
}

/// Arrow-key (or `j`/`k`) single-choice menu over any `Display`-able option list, cursor
/// starting on `current_index` so pressing enter immediately keeps the current value. Typed
/// filtering is disabled — this is a radio-style picker, not a search box.
pub fn prompt_select<T: Display>(label: &str, options: Vec<T>, current_index: usize) -> Result<T> {
    let start = current_index.min(options.len().saturating_sub(1));
    let prompt = Select::new(label, options)
        .with_starting_cursor(start)
        .with_vim_mode(true)
        .without_filtering()
        .with_render_config(theme());
    map_result(prompt.prompt())
}

/// Arrow-key calendar picker, defaulting to today. `label` is shown above the calendar grid;
/// month/day/year navigation and the min/max-date bounds are inquire's own defaults.
pub fn prompt_date(label: &str) -> Result<NaiveDate> {
    let prompt = DateSelect::new(label).with_render_config(theme());
    map_result(prompt.prompt())
}

/// Box-drawn header for an interactive flow, reusing the same border glyphs as
/// `render::to_text`'s task-detail card (`table::boxed_titled_border`) so a prompt-driven form
/// reads as the same visual system as `show`/`config show` rather than a foreign widget.
pub fn render_header_card(title: &str, subtitle: &str) {
    let width = wrap::terminal_width();
    println!();
    println!("{}", table::boxed_titled_border("╭", "╮", Some(title), width));
    println!("{}", table::boxed_row(&[table::spaces_seg(table::BOX_INDENT), table::dim_seg(subtitle)], width));
    println!("{}", table::boxed_titled_border("╰", "╯", None, width));
    println!();
}
