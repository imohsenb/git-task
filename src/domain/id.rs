pub type TaskId = String;

pub const SHORT_LEN: usize = 8;

pub fn short(id: &str) -> &str {
    &id[..SHORT_LEN.min(id.len())]
}

/// Human-facing address: `<repo key>-<short hash>`, e.g. `SRV-9057e58a`.
pub fn display(key: &str, full_id: &str) -> String {
    format!("{key}-{}", short(full_id))
}

/// Strips an optional `KEY-` prefix so lookups accept both `SRV-9057e58a`
/// and a bare hash prefix. The key is purely cosmetic — this never
/// validates it against the repo's configured key, it just recognizes the
/// shape (non-hex prefix, dash, hex remainder) and drops it.
pub fn normalize_ref_input(input: &str) -> &str {
    if let Some((_, rest)) = input.split_once('-') {
        if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_hexdigit()) {
            return rest;
        }
    }
    input
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_truncates_to_eight_chars() {
        assert_eq!(short("9057e58a50d7a47fcf945c545071b469d5005689"), "9057e58a");
    }

    #[test]
    fn short_leaves_shorter_ids_alone() {
        assert_eq!(short("abc"), "abc");
    }

    #[test]
    fn display_prefixes_short_hash_with_key() {
        assert_eq!(display("SRV", "9057e58a50d7a47fcf945c545071b469d5005689"), "SRV-9057e58a");
    }

    #[test]
    fn normalize_strips_key_prefix_over_hex_remainder() {
        assert_eq!(normalize_ref_input("SRV-9057e58a"), "9057e58a");
    }

    #[test]
    fn normalize_leaves_bare_hash_alone() {
        assert_eq!(normalize_ref_input("9057e58a"), "9057e58a");
    }

    #[test]
    fn normalize_leaves_non_hex_remainder_alone() {
        // Whole thing isn't a valid hash prefix either way — resolve() will just
        // report "no task matching", which is the correct behavior here.
        assert_eq!(normalize_ref_input("SRV-not-hex"), "SRV-not-hex");
    }
}
