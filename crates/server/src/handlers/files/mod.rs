/// File information route.
mod details;

/// Build session file upload sealing route.
mod seal;

/// File upload route
mod upload;

use std::sync::Arc;

use aide::axum::{
    routing::{get_with, post_with},
    ApiRouter,
};
use db::DatabaseConnection;

/// Create an [`ApiRouter`] that provides an API server with source code file handling routes.
pub(crate) fn routes() -> ApiRouter<Arc<DatabaseConnection>> {
    ApiRouter::new()
        .api_route("/seal/:token", post_with(seal::seal, seal::docs))
        .api_route("/upload/:token", post_with(upload::upload, upload::docs))
        .api_route("/:sourceCode", get_with(details::details, details::docs))
        .with_path_items(|op| op.tag("File uploads"))
}
