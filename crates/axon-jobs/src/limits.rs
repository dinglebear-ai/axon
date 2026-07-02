pub const DEFAULT_PAGE_LIMIT: u32 = 100;
pub const MAX_PAGE_LIMIT: u32 = 500;

pub fn clamp_page_limit(limit: Option<u32>) -> u32 {
    match limit {
        Some(0) | None => DEFAULT_PAGE_LIMIT,
        Some(limit) => limit.min(MAX_PAGE_LIMIT),
    }
}
