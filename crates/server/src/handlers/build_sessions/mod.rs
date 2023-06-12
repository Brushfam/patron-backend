/// Build session create route.
mod create;

/// Build session details route.
mod details;

/// Latest build session info route.
mod latest;

/// Build session list route.
mod list;

/// Build session logs route.
mod logs;

/// Contract JSON metadata route.
mod metadata;

/// Build session status route.
mod status;

/// WASM blob route.
mod wasm;

use std::sync::Arc;

use axum::{middleware::from_fn_with_state, routing::get, Router};
use common::config::Config;
use db::DatabaseConnection;

use crate::auth;

/// Create a router that provides an API server with
/// build session management routes.
pub(crate) fn routes(
    database: Arc<DatabaseConnection>,
    config: Arc<Config>,
) -> Router<Arc<DatabaseConnection>> {
    let public_routes = Router::new()
        .route("/latest/:archiveHash", get(latest::latest))
        .route("/metadata/:codeHash", get(metadata::metadata))
        .route("/wasm/:codeHash", get(wasm::wasm))
        .route("/details/:codeHash", get(details::details))
        .route("/status/:id", get(status::status))
        .route("/logs/:id", get(logs::logs));

    let private_routes = Router::new()
        .route("/", get(list::list).post(create::create))
        .route_layer(from_fn_with_state(
            (database, config),
            auth::require_authentication::<true, true, _>,
        ));

    Router::new().merge(private_routes).merge(public_routes)
}
