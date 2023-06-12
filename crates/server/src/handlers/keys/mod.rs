/// Public key deletion route.
mod delete;

/// Public key list route.
mod list;

/// Public key verification route.
mod verify;

use std::sync::Arc;

use axum::{routing::get, Router};
use db::DatabaseConnection;

/// Create a router that provides an API server with public key management routes.
pub(crate) fn routes() -> Router<Arc<DatabaseConnection>> {
    Router::new().route(
        "/",
        get(list::list).post(verify::verify).delete(delete::delete),
    )
}
