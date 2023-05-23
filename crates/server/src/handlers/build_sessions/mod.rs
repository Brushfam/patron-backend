mod create;
mod details;
mod latest;
mod list;
mod logs;
mod metadata;
mod status;
mod wasm;

use std::sync::Arc;

use axum::{middleware::from_fn_with_state, routing::get, Router};
use common::config::Config;
use db::DatabaseConnection;

use crate::auth;

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
