mod list;
mod upload;

use std::sync::Arc;

use axum::{middleware::from_fn_with_state, routing::get, Router};
use common::config::Config;
use db::DatabaseConnection;

use crate::auth;

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
