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

    let header_cells: Vec<Seg> = headers.iter().map(|h| Seg { colored: color::dim_bold(h), plain: h.to_string() }).collect();
    let header_row = pad_row(header_cells, &col_width);
    let data_rows: Vec<Vec<Seg>> = rows.into_iter().map(|row| pad_row(row, &col_width)).collect();

    // A wide, many-column table routinely needs more than the terminal's fallback width (80
    // cols when stdout isn't a tty, e.g. every non-interactive run) — unlike `render.rs`'s
    // label:value rows, which comfortably fit under it, so this box grows to fit its content
    // instead of letting `boxed_row`'s padding underflow to zero and break the right border.
    let row_width = |segs: &[Seg]| segs.iter().map(|s| s.plain.chars().count()).sum::<usize>();
    let content_width = std::iter::once(&header_row).chain(data_rows.iter()).map(|r| row_width(r)).max().unwrap_or(0);
    let width = wrap::terminal_width().max(content_width + 3);

    let mut lines = vec![boxed_titled_border("╭", "╮", Some(title), width), boxed_blank(width)];
    lines.push(boxed_row(&header_row, width));
    for row in data_rows {
        lines.push(boxed_row(&row, width));
    }
    lines.push(boxed_blank(width));
    lines.push(boxed_titled_border("╰", "╯", None, width));
    lines
}
