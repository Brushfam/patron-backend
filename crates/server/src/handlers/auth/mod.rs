/// CLI token exchange route.
mod exchange;

/// User authentication route.
mod login;

/// User registration route.
mod register;

use std::sync::Arc;

use aide::axum::{routing::post_with, ApiRouter};
use db::DatabaseConnection;

/// Create an [`ApiRouter`] that provides an API server with authentication routes.
pub(crate) fn routes() -> ApiRouter<Arc<DatabaseConnection>> {
    ApiRouter::new()
        .api_route("/login", post_with(login::login, login::docs))
        .api_route("/register", post_with(register::register, register::docs))
        .api_route("/exchange", post_with(exchange::exchange, exchange::docs))
        .with_path_items(|op| op.tag("Authentication"))
}
