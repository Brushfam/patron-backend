/// Membership check route.
mod check;

use std::sync::Arc;

use axum::{routing::post, Router};
use db::DatabaseConnection;

/// Create a router that provides an API server with payment verification routes.
pub(crate) fn routes() -> Router<Arc<DatabaseConnection>> {
    Router::new().route("/", post(check::check))
}
