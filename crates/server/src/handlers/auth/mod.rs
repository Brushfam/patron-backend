mod exchange;
mod login;
mod register;

use std::sync::Arc;

use axum::{routing::post, Router};
use db::DatabaseConnection;

pub(crate) fn routes() -> Router<Arc<DatabaseConnection>> {
    Router::new()
        .route("/login", post(login::login))
        .route("/register", post(register::register))
        .route("/exchange", post(exchange::exchange))
}
