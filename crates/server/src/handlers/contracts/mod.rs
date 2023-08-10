/// Smart contract details route.
mod details;

/// Smart contract events list route.
mod events;

use std::sync::Arc;

use aide::axum::{routing::get_with, ApiRouter};
use common::rpc::sp_core::crypto::AccountId32;
use db::DatabaseConnection;
use schemars::JsonSchema;
use serde::Deserialize;

/// [`AccountId32`] wrapper for OAPI documentation purposes.
#[derive(Deserialize, JsonSchema)]
#[serde(transparent)]
struct WrappedAccountId32(
    #[schemars(example = "crate::schema::example_account", with = "String")] pub AccountId32,
);

/// Create an [`ApiRouter`] that provides an API server with contract information routes.
pub(crate) fn routes() -> ApiRouter<Arc<DatabaseConnection>> {
    ApiRouter::new()
        .api_route("/events/:account", get_with(events::events, events::docs))
        .api_route("/:account", get_with(details::details, details::docs))
        .with_path_items(|op| op.tag("Contract management"))
}
