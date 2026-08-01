use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL_CONDENSED;
use comfy_table::{Attribute, Cell, Color, Table};

use crate::color;

/// A `Table` pre-styled with rounded UTF8 borders and no divider between every row (comfy-table's
/// default preset draws a `+---+` separator after each row, which reads as noise once there's
/// more than a couple of rows) — shared by every list-shaped command (`ls`, `repos`, `projects`)
/// so they render consistently instead of each picking its own preset.
pub fn new() -> Table {
    let mut table = Table::new();
    table.load_preset(UTF8_FULL_CONDENSED).apply_modifier(UTF8_ROUND_CORNERS);
    table
}

/// Bold, brand-yellow header cells — matches `color::heading`'s accent so tabular output and
/// the banner read as one visual system instead of two.
pub fn header(names: &[&str]) -> Vec<Cell> {
    names.iter().map(|n| Cell::new(*n).fg(Color::Yellow).add_attribute(Attribute::Bold)).collect()
}

/// The same sky-blue accent as `color::cyan` (`color::CYAN_RGB`), as a comfy-table `Color` —
/// comfy-table cells need its own `Color` type rather than a raw ANSI-wrapped string.
pub fn cyan() -> Color {
    let (r, g, b) = color::CYAN_RGB;
    Color::Rgb { r, g, b }
}
