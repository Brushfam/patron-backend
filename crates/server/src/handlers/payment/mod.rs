mod check;

use std::sync::Arc;

use axum::{routing::post, Router};
use db::DatabaseConnection;

pub(crate) fn routes() -> Router<Arc<DatabaseConnection>> {
    Router::new().route("/", post(check::check))
}
