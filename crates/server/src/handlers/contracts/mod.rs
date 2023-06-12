/// Smart contract details route.
mod details;

/// Smart contract events list route.
mod events;

use std::sync::Arc;

use axum::{routing::get, Router};
use db::DatabaseConnection;

/// Create a router that provides an API server with contract information routes.
pub(crate) fn routes() -> Router<Arc<DatabaseConnection>> {
    Router::new()
        .route("/events/:account", get(events::events))
        .route("/:account", get(details::details))
}
