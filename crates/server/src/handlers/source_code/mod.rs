/// Source code archive list route.
mod list;

/// Source code archive upload route.
mod upload;

use std::sync::Arc;

use aide::axum::{routing::get_with, ApiRouter};
use axum::middleware::from_fn_with_state;
use common::config::Config;
use db::DatabaseConnection;

use crate::auth;

/// Create a router that provides an API server with source code management routes.
pub(crate) fn routes(
    database: Arc<DatabaseConnection>,
    config: Arc<Config>,
) -> ApiRouter<Arc<DatabaseConnection>> {
    ApiRouter::new()
        .api_route(
            "/",
            get_with(list::list, list::docs).post_with(upload::upload, upload::docs),
        )
        .route_layer(from_fn_with_state(
            (database, config),
            auth::require_authentication::<true, true, _>,
        ))
        .with_path_items(|op| {
            op.security_requirement("Authentication token")
                .tag("Source code management")
        })
}
