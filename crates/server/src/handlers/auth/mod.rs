/// CLI token exchange route.
mod exchange;

/// User authentication route.
mod login;

/// User registration route.
mod register;

use std::sync::Arc;

use axum::{routing::post, Router};
use db::DatabaseConnection;

/// Create a router that provides an API server with authentication routes.
pub(crate) fn routes() -> Router<Arc<DatabaseConnection>> {
    Router::new()
        .route("/login", post(login::login))
        .route("/register", post(register::register))
        .route("/exchange", post(exchange::exchange))
}
