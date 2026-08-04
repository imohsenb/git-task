use crate::color;
use crate::wrap;

/// One piece of a box row: the plain text (used to compute how much padding the row needs so
/// the right border lines up) paired with its already-ANSI-wrapped form. Colors are applied
/// once, when a `Seg` is built, and never touched again — so padding math only ever measures
/// plain strings and never has to strip escape codes back out of colored ones. Shared by every
/// bordered-box renderer (`render::to_text`'s task-detail card, `list_box`'s tables) so they
/// all read as one visual system.
pub struct Seg {
    pub plain: String,
    pub colored: String,
}

pub fn plain_seg(text: &str) -> Seg {
    Seg { colored: text.to_string(), plain: text.to_string() }
}

pub fn bold_seg(text: &str) -> Seg {
    Seg { colored: color::bold(text), plain: text.to_string() }
}

pub fn dim_seg(text: &str) -> Seg {
    Seg { colored: color::dim(text), plain: text.to_string() }
}

pub fn spaces_seg(n: usize) -> Seg {
    let s = " ".repeat(n);
    Seg { colored: s.clone(), plain: s }
}

pub fn box_border(s: &str) -> String {
    color::dim(s)
}

pub fn boxed_row(segs: &[Seg], width: usize) -> String {
    let inner_width = width.saturating_sub(2);
    let plain_len: usize = segs.iter().map(|s| s.plain.chars().count()).sum();
    let pad = inner_width.saturating_sub(plain_len);
    let content: String = segs.iter().map(|s| s.colored.as_str()).collect();
    format!("{}{content}{}{}", box_border("│"), " ".repeat(pad), box_border("│"))
}

pub fn boxed_blank(width: usize) -> String {
    boxed_row(&[], width)
}

pub const BOX_INDENT: usize = 2;
pub const BOX_LABEL_WIDTH: usize = 9;
pub const BOX_COL_GAP: usize = 2;
pub const BOX_HALF_COL: usize = 34;

/// A label:value row inside a bordered detail card, e.g. `  ID        SRV-1f2dce54` — label
/// left-padded to a fixed column so every single-field row in the card lines up regardless of
/// label length. Shared by every "detail card" renderer (task `show`, `config show`, and any
/// future one) so they all read as one visual system instead of each reinventing the box math.
pub fn field_row(label_text: &str, value: Seg, width: usize) -> String {
    let gap = BOX_LABEL_WIDTH.saturating_sub(label_text.chars().count()) + BOX_COL_GAP;
    boxed_row(&[spaces_seg(BOX_INDENT), dim_seg(label_text), spaces_seg(gap), value], width)
}

/// Two label:value pairs on one row (e.g. `Status`/`Kind`) — the first pair is padded out to
/// `BOX_HALF_COL` so the second pair's label always starts at the same column no matter how long
/// the first value is.
pub fn field_row2(label1: &str, value1: Seg, label2: &str, value2: Seg, width: usize) -> String {
    let gap1 = BOX_LABEL_WIDTH.saturating_sub(label1.chars().count()) + BOX_COL_GAP;
    let left_len = BOX_INDENT + label1.chars().count() + gap1 + value1.plain.chars().count();
    let mid_pad = BOX_HALF_COL.saturating_sub(left_len);
    let gap2 = BOX_LABEL_WIDTH.saturating_sub(label2.chars().count()) + BOX_COL_GAP;
    boxed_row(
        &[
            spaces_seg(BOX_INDENT),
            dim_seg(label1),
            spaces_seg(gap1),
            value1,
            spaces_seg(mid_pad),
            dim_seg(label2),
            spaces_seg(gap2),
            value2,
        ],
        width,
    )
}

/// One free-text line inside a detail card, indented like a field row but with no label column
/// (task descriptions, config rule lines, …).
pub fn text_row(text: &str, width: usize) -> String {
    boxed_row(&[spaces_seg(BOX_INDENT), plain_seg(text)], width)
}

/// Max plain-text length for a wrapped line indented by `indent` spaces inside a detail card, so
/// it never fills the row exactly flush to the right border — leaves at least one column of
/// breathing room before the closing `│`, matching the left-hand indent's margin.
pub fn wrap_width_for(indent: usize, width: usize) -> usize {
    width.saturating_sub(2 + indent + 1)
}

/// `╭── TITLE ──────...──╮` (or `├─…─┤` mid-box, `╰─…─╯` for the close, when `left`/`right`
/// are the matching corner/tee characters) with the title itself sized off its plain text so
/// the dash count comes out right regardless of the heading color codes wrapped around it.
pub fn boxed_titled_border(left: &str, right: &str, title: Option<&str>, width: usize) -> String {
    let inner_width = width.saturating_sub(2);
    match title {
        Some(title) => {
            let head = format!("── {title} ");
            let dashes = inner_width.saturating_sub(head.chars().count());
            format!(
                "{}{}{}{}{}{}",
                box_border(left),
                box_border("── "),
                color::heading(title),
                box_border(" "),
                box_border(&"─".repeat(dashes)),
                box_border(right),
            )
        }
        None => format!("{}{}{}", box_border(left), box_border(&"─".repeat(inner_width)), box_border(right)),
    }
}

const ROW_INDENT: usize = 2;
const COL_GAP: usize = 2;

/// Right-pads every column except the last (which just rides `boxed_row`'s own row-wide
/// padding) out to `col_width[i] + COL_GAP`, and prefixes the row with `ROW_INDENT` — so a
/// plain `Vec<Seg>` per row, built with no knowledge of the other rows, still lines up into
/// aligned columns.
fn pad_row(cells: Vec<Seg>, col_width: &[usize]) -> Vec<Seg> {
    let n = cells.len();
    let mut segs = Vec::with_capacity(n * 2 + 1);
    segs.push(spaces_seg(ROW_INDENT));
    for (i, cell) in cells.into_iter().enumerate() {
        let plain_len = cell.plain.chars().count();
        segs.push(cell);
        if i + 1 < n {
            let target = col_width.get(i).copied().unwrap_or(0) + COL_GAP;
            segs.push(spaces_seg(target.saturating_sub(plain_len)));
        }
    }
    segs
}

/// Truncates a cell's colored+plain text to `max_width`, appending `…` when it doesn't fit.
/// Splices the truncated plain text back into the colored string in place of the original,
/// which is safe because every `color::*` helper wraps the *whole* string once (prefix + text
/// + suffix) rather than nesting multiple codes, so `plain` always appears in `colored` as one
/// contiguous, unique substring.
fn truncate_seg(seg: Seg, max_width: usize) -> Seg {
    if seg.plain.chars().count() <= max_width {
        return seg;
    }
    let new_plain = wrap::truncate_ellipsis(&seg.plain, max_width);
    let new_colored =
        if seg.colored == seg.plain { new_plain.clone() } else { seg.colored.replacen(&seg.plain, &new_plain, 1) };
    Seg { colored: new_colored, plain: new_plain }
}

/// Truncates just the last cell of a row to `col_width`'s last entry — the free-text column
/// (TITLE, PATH, REPOS…) that every `list_box` caller puts last and that's the one actually
/// capable of overflowing a terminal.
fn truncate_last(mut cells: Vec<Seg>, col_width: &[usize]) -> Vec<Seg> {
    if let Some(&w) = col_width.last() {
        if let Some(last) = cells.pop() {
            cells.push(truncate_seg(last, w));
        }
    }
    cells
}

/// The bordered "TITLE (N)" list table shared by every list-shaped command (`ls`, `repos`,
/// `projects`): a titled box, a bold/dim header row, one row per item, blank padding lines
/// top and bottom — the same box vocabulary `render::to_text` uses for the task-detail card,
/// so tabular and card output read as one style instead of two.
pub fn list_box(title: &str, headers: &[&str], rows: Vec<Vec<Seg>>) -> Vec<String> {
    let mut col_width: Vec<usize> = headers.iter().map(|h| h.chars().count()).collect();
    for row in &rows {
        for (i, cell) in row.iter().enumerate() {
            if let Some(w) = col_width.get_mut(i) {
                *w = (*w).max(cell.plain.chars().count());
            }
        }
    }

    // Cap the last column to whatever's left of the terminal width after every other column
    // and its gap, so a long value (a task title, a long repo path, …) degrades to an ellipsis
    // instead of forcing the box wider than the terminal — which the terminal then soft-wraps,
    // breaking the box-drawing alignment (the exact failure `width` below works around for the
    // box as a whole, but can't fix once a single cell is the culprit).
    const MIN_LAST_COL_WIDTH: usize = 20;
    let n = headers.len();
    if n > 0 {
        let non_last_total = col_width[..n - 1].iter().sum::<usize>() + COL_GAP * (n - 1);
        let overhead = ROW_INDENT + 2;
        let last_budget = wrap::terminal_width().saturating_sub(non_last_total + overhead).max(MIN_LAST_COL_WIDTH);
        col_width[n - 1] = col_width[n - 1].min(last_budget);
    }

    let header_cells: Vec<Seg> = headers.iter().map(|h| Seg { colored: color::dim_bold(h), plain: h.to_string() }).collect();
    let header_row = pad_row(truncate_last(header_cells, &col_width), &col_width);
    let data_rows: Vec<Vec<Seg>> =
        rows.into_iter().map(|row| pad_row(truncate_last(row, &col_width), &col_width)).collect();

    // A wide, many-column table routinely needs more than the terminal's fallback width (80
    // cols when stdout isn't a tty, e.g. every non-interactive run) — unlike `render.rs`'s
    // label:value rows, which comfortably fit under it, so this box grows to fit its content
    // instead of letting `boxed_row`'s padding underflow to zero and break the right border.
    let row_width = |segs: &[Seg]| segs.iter().map(|s| s.plain.chars().count()).sum::<usize>();
    let content_width = std::iter::once(&header_row).chain(data_rows.iter()).map(|r| row_width(r)).max().unwrap_or(0);
    let width = wrap::terminal_width().max(content_width + 3);

    let mut lines = vec![String::new(), boxed_titled_border("╭", "╮", Some(title), width), boxed_blank(width)];
    lines.push(boxed_row(&header_row, width));
    for row in data_rows {
        lines.push(boxed_row(&row, width));
    }
    lines.push(boxed_blank(width));
    lines.push(boxed_titled_border("╰", "╯", None, width));
    lines.push(String::new());
    lines
}
