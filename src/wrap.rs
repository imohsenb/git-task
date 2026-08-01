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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_stays_on_one_line() {
        assert_eq!(wrap("hello world", 80), vec!["hello world"]);
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
}
