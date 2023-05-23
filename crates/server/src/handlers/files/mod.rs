mod details;
mod seal;
mod upload;

use std::sync::Arc;

use axum::{
    routing::{get, post},
    Router,
};
use db::DatabaseConnection;

pub(crate) fn routes() -> Router<Arc<DatabaseConnection>> {
    Router::new()
        .route("/seal/:token", post(seal::seal))
        .route("/upload/:token", post(upload::upload))
        .route("/:sourceCode", get(details::details))
}
