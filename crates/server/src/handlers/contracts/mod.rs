mod details;
mod events;

use std::sync::Arc;

use axum::{routing::get, Router};
use db::DatabaseConnection;

pub(crate) fn routes() -> Router<Arc<DatabaseConnection>> {
    Router::new()
        .route("/events/:account", get(events::events))
        .route("/:account", get(details::details))
}
