pub type TaskId = String;

pub const SHORT_LEN: usize = 8;

pub fn short(id: &str) -> &str {
    &id[..SHORT_LEN.min(id.len())]
}
