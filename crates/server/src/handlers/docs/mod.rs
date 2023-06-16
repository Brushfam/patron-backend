use std::sync::Arc;

use aide::{
    axum::{routing::get, ApiRouter},
    openapi::OpenApi,
    redoc::Redoc,
};
use axum::{Extension, Json};
use db::DatabaseConnection;

/// Create an [`ApiRouter`] that provides an API server with documentation routes.
pub(crate) fn routes() -> ApiRouter<Arc<DatabaseConnection>> {
    ApiRouter::new()
        .route("/", Redoc::new("/docs/api.json").axum_route())
        .route(
            "/api.json",
            get(|Extension(oapi): Extension<Arc<OpenApi>>| async move { Json(oapi) }),
        )
}
