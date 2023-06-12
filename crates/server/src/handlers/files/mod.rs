/// File information route.
mod details;

/// Build session file upload sealing route.
mod seal;

/// File upload route
mod upload;

use std::sync::Arc;

use axum::{
    routing::{get, post},
    Router,
};
use db::DatabaseConnection;

/// Create a router that provides an API server with source code file handling routes.
pub(crate) fn routes() -> Router<Arc<DatabaseConnection>> {
    Router::new()
        .route("/seal/:token", post(seal::seal))
        .route("/upload/:token", post(upload::upload))
        .route("/:sourceCode", get(details::details))
}
