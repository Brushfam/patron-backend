/// Public key deletion route.
mod delete;

/// Public key list route.
mod list;

/// Public key verification route.
mod verify;

use std::sync::Arc;

use aide::axum::{routing::get_with, ApiRouter};
use db::DatabaseConnection;

/// Create an [`ApiRouter`] that provides an API server with public key management routes.
pub(crate) fn routes() -> ApiRouter<Arc<DatabaseConnection>> {
    ApiRouter::new()
        .api_route(
            "/",
            get_with(list::list, list::docs)
                .post_with(verify::verify, verify::docs)
                .delete_with(delete::delete, delete::docs),
        )
        .with_path_items(|op| op.tag("Public key verification"))
}
