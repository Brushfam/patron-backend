/// Source code archive list route.
mod list;

/// Source code archive upload route.
mod upload;

use std::sync::Arc;

use axum::{middleware::from_fn_with_state, routing::get, Router};
use common::config::Config;
use db::DatabaseConnection;

use crate::auth;

/// Create a router that provides an API server with source code management routes.
pub(crate) fn routes(
    database: Arc<DatabaseConnection>,
    config: Arc<Config>,
) -> Router<Arc<DatabaseConnection>> {
    Router::new()
        .route("/", get(list::list).post(upload::upload))
        .route_layer(from_fn_with_state(
            (database, config),
            auth::require_authentication::<true, true, _>,
        ))
}
