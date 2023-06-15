/// Membership check route.
mod check;

use std::sync::Arc;

use aide::axum::{routing::post_with, ApiRouter};
use db::DatabaseConnection;

/// Create a [`ApiRouter`] that provides an API server with payment verification routes.
pub(crate) fn routes() -> ApiRouter<Arc<DatabaseConnection>> {
    ApiRouter::new()
        .api_route("/", post_with(check::check, check::docs))
        .with_path_items(|op| op.tag("Membership and payments"))
}
