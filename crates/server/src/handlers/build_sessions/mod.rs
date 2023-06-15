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

use aide::axum::{routing::get_with, ApiRouter};
use axum::middleware::from_fn_with_state;
use common::config::Config;
use db::DatabaseConnection;

use crate::auth;

/// Create a router that provides an API server with
/// build session management routes.
pub(crate) fn routes(
    database: Arc<DatabaseConnection>,
    config: Arc<Config>,
) -> ApiRouter<Arc<DatabaseConnection>> {
    let public_routes = ApiRouter::new()
        .api_route(
            "/latest/:archiveHash",
            get_with(latest::latest, latest::docs),
        )
        .api_route(
            "/metadata/:codeHash",
            get_with(metadata::metadata, metadata::docs),
        )
        .api_route("/wasm/:codeHash", get_with(wasm::wasm, wasm::docs))
        .api_route(
            "/details/:codeHash",
            get_with(details::details, details::docs),
        )
        .api_route("/status/:id", get_with(status::status, status::docs))
        .api_route("/logs/:id", get_with(logs::logs, logs::docs));

    let private_routes = ApiRouter::new()
        .api_route(
            "/",
            get_with(list::list, list::docs).post_with(create::create, create::docs),
        )
        .route_layer(from_fn_with_state(
            (database, config),
            auth::require_authentication::<true, true, _>,
        ))
        .with_path_items(|op| op.security_requirement("Authentication token"));

    ApiRouter::new()
        .merge(private_routes)
        .merge(public_routes)
        .with_path_items(|op| op.tag("Build session management"))
}
