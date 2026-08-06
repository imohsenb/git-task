use std::io::IsTerminal;

/// Standard terminal width fallback (80 columns) — used whenever stdout isn't a terminal, its
/// size can't be determined, or the reported size is implausibly narrow (some ptys report 0
/// columns until a size is explicitly set, which would otherwise wrap every word onto its own
/// line).
const FALLBACK_WIDTH: usize = 80;
const MIN_PLAUSIBLE_WIDTH: usize = 20;

/// Real terminal column count when stdout is a tty and the reported size is plausible, else
/// the 80-column standard fallback.
pub fn terminal_width() -> usize {
    if !std::io::stdout().is_terminal() {
        return FALLBACK_WIDTH;
    }
    match crossterm::terminal::size() {
        Ok((cols, _)) if cols as usize >= MIN_PLAUSIBLE_WIDTH => cols as usize,
        _ => FALLBACK_WIDTH,
    }
}

/// Strips control characters (C0/C1, including ESC) from user-supplied text before it reaches a
/// terminal — task titles/descriptions/comments/labels/statuses are free-form strings that sync
/// in from other machines via `push`/`pull`/`clone`, so nothing stops one from carrying raw
/// ANSI/OSC escape sequences (cursor moves, screen-clears, an OSC 52 clipboard write) unless this
/// runs before the content is written to stdout. `\n`/`\t` are kept since callers rely on them for
/// layout.
pub fn sanitize(s: &str) -> String {
    s.chars().filter(|c| !c.is_control() || *c == '\n' || *c == '\t').collect()
}

/// Greedy word-wrap: fills each line up to `width` without splitting a word. A single word
/// longer than `width` is left whole on its own line rather than force-broken mid-word.
/// Existing newlines in `text` are preserved as paragraph breaks, each wrapped independently.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        if paragraph.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut current = String::new();
        for word in paragraph.split_whitespace() {
            let would_be_len = if current.is_empty() { word.len() } else { current.len() + 1 + word.len() };
            if would_be_len > width && !current.is_empty() {
                lines.push(std::mem::take(&mut current));
            }
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
        }
        lines.push(current);
    }
    lines
}

/// Truncates `text` to at most `max_width` characters, replacing the tail with a single `…`
/// when it doesn't fit — so a long value degrades to `"some long titl…"` instead of overflowing
/// its column and breaking a table's alignment.
pub fn truncate_ellipsis(text: &str, max_width: usize) -> String {
    if text.chars().count() <= max_width {
        return text.to_string();
    }
    if max_width == 0 {
        return String::new();
    }
    let mut s: String = text.chars().take(max_width - 1).collect();
    s.push('…');
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_stays_on_one_line() {
        assert_eq!(wrap("hello world", 80), vec!["hello world"]);
    }

    #[test]
    fn sanitize_strips_ansi_escape_and_other_control_chars() {
        assert_eq!(sanitize("Evil\x1b[31mRED\x1b[0mTitle"), "Evil[31mRED[0mTitle");
        assert_eq!(sanitize("a\x07b\rc\x7fd"), "abcd");
    }

    #[test]
    fn sanitize_keeps_newlines_and_tabs() {
        assert_eq!(sanitize("line1\nline2\ttabbed"), "line1\nline2\ttabbed");
    }

    #[test]
    fn wraps_at_word_boundaries() {
        assert_eq!(wrap("one two three four", 9), vec!["one two", "three", "four"]);
    }

    #[test]
    fn preserves_existing_newlines_as_paragraph_breaks() {
        assert_eq!(wrap("first\nsecond", 80), vec!["first", "second"]);
    }

    #[test]
    fn a_word_longer_than_width_is_not_broken() {
        assert_eq!(wrap("supercalifragilistic ok", 5), vec!["supercalifragilistic", "ok"]);
    }

    #[test]
    fn blank_line_between_paragraphs_is_preserved() {
        assert_eq!(wrap("first\n\nsecond", 80), vec!["first", "", "second"]);
    }

    #[test]
    fn truncate_short_text_is_unchanged() {
        assert_eq!(truncate_ellipsis("hello", 10), "hello");
    }

    #[test]
    fn truncate_exact_fit_is_unchanged() {
        assert_eq!(truncate_ellipsis("hello", 5), "hello");
    }

    #[test]
    fn truncate_long_text_gets_ellipsis() {
        assert_eq!(truncate_ellipsis("hello world", 8), "hello w…");
    }

    #[test]
    fn truncate_zero_width_is_empty() {
        assert_eq!(truncate_ellipsis("hello", 0), "");
    }
}
