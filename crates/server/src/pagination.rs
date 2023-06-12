use serde::Deserialize;

/// Count of items per page.
pub const PER_PAGE: u64 = 25;

/// Total page limit.
pub const MAX_PAGES: u64 = 10000;

/// Pagination helper for the [`Query`] extractor.
///
/// [`Query`]: axum::extract::Query
#[derive(Deserialize)]
pub struct Pagination {
    /// Current page value.
    #[serde(default)]
    page: u64,
}

impl Pagination {
    /// Get `LIMIT` value for a SQL query.
    pub fn limit(&self) -> u64 {
        PER_PAGE
    }

    /// Get `OFFSET` value for a SQL query.
    pub fn offset(&self) -> u64 {
        self.page.min(MAX_PAGES).saturating_sub(1) * PER_PAGE
    }
}
