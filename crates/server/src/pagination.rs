use std::num::NonZeroU64;

use schemars::JsonSchema;
use serde::Deserialize;

/// Count of items per page.
pub const PER_PAGE: u64 = 25;

/// Total page limit.
pub const MAX_PAGES: u64 = 10000;

/// Pagination helper for the [`Query`] extractor.
///
/// [`Query`]: axum::extract::Query
#[derive(Deserialize, JsonSchema)]
pub struct Pagination {
    /// Current page value.
    #[serde(default = "default_page")]
    page: NonZeroU64,
}

/// Default page value used when user didn't provide one.
fn default_page() -> NonZeroU64 {
    // FIXME: Replace with https://doc.rust-lang.org/stable/std/num/struct.NonZeroU64.html#associatedconstant.MIN
    NonZeroU64::new(1).unwrap()
}

impl Pagination {
    /// Get `LIMIT` value for a SQL query.
    pub fn limit(&self) -> u64 {
        PER_PAGE
    }

    /// Get `OFFSET` value for a SQL query.
    pub fn offset(&self) -> u64 {
        (self.page.get().min(MAX_PAGES) - 1) * PER_PAGE
    }
}
