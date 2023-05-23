use serde::Deserialize;

pub const PER_PAGE: u64 = 25;
pub const MAX_PAGES: u64 = 10000;

#[derive(Deserialize)]
pub struct Pagination {
    #[serde(default)]
    page: u64,
}

impl Pagination {
    pub fn limit(&self) -> u64 {
        PER_PAGE
    }

    pub fn offset(&self) -> u64 {
        self.page.min(MAX_PAGES).saturating_sub(1) * PER_PAGE
    }
}
