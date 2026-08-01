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
