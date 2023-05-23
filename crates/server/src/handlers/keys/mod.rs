mod delete;
mod list;
mod verify;

use std::sync::Arc;

use axum::{routing::get, Router};
use db::DatabaseConnection;

pub(crate) fn routes() -> Router<Arc<DatabaseConnection>> {
    Router::new().route(
        "/",
        get(list::list).post(verify::verify).delete(delete::delete),
    )
}
