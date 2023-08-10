//! # API server
//!
//! # Proxy HTTP server
//!
//! The API server will not handle TLS termination or any request body size limiting
//! by itself, thus it has to be proxied via some other server which will handle all of that.
//!
//! Request body size limiting is necessary to ensure that you don't get overwhelmed with
//! source code archive uploads while using a self-hosted environment.

#![deny(missing_docs)]
#![deny(clippy::missing_docs_in_private_items)]

/// API authentication middleware and helpers.
mod auth;

/// Route handlers.
mod handlers;

/// Hex-encoded array wrapper.
mod hex_hash;

/// Resource pagination structs.
mod pagination;

/// Validated JSON bodies.
mod validation;

/// [`schemars`] crate helper functions.
mod schema;

#[cfg(test)]
mod testing;

use std::sync::Arc;

use aide::{
    axum::ApiRouter,
    openapi::{OpenApi, SecurityScheme, Tag},
    transform::TransformOpenApi,
};
use axum::{middleware::from_fn_with_state, Extension, Server};
use common::{config::Config, logging};
use db::{Database, DatabaseConnection};
use tracing::info;

/// API server entrypoint.
#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let config = Config::new(None)?;

    logging::init(&config);

    let Some(server_config) = config.server.as_ref() else {
        return Err(anyhow::Error::msg("unable to load server config"));
    };

    info!("connecting to database");
    let database = Arc::new(Database::connect(&config.database.url).await?);
    info!("database connection established");
    let server = Server::bind(&server_config.address);
    let config = Arc::new(config);

    let mut api = OpenApi::default();

    server
        .serve(
            app_router(database, config)
                .finish_api_with(&mut api, api_docs)
                .layer(Extension(Arc::new(api)))
                .into_make_service(),
        )
        .await?;

    Ok(())
}

/// Construct a [`ApiRouter`] with API server endpoints.
fn app_router(database: Arc<DatabaseConnection>, config: Arc<Config>) -> ApiRouter {
    let mixed_routes = ApiRouter::new()
        .nest(
            "/sourceCode",
            handlers::source_code::routes(database.clone(), config.clone()),
        )
        .nest(
            "/buildSessions",
            handlers::build_sessions::routes(database.clone(), config.clone()),
        );

    let protected_routes = ApiRouter::new()
        .nest("/keys", handlers::keys::routes())
        .route_layer(from_fn_with_state(
            (database.clone(), config.clone()),
            auth::require_authentication::<false, false, _>,
        ))
        .with_path_items(|op| op.security_requirement("Authentication token"));

    let payment_routes = ApiRouter::new()
        .nest("/payment", handlers::payment::routes())
        .route_layer(from_fn_with_state(
            (database.clone(), config.clone()),
            auth::require_authentication::<true, false, _>,
        ))
        .with_path_items(|op| op.security_requirement("Authentication token"));

    ApiRouter::new()
        .merge(mixed_routes)
        .merge(protected_routes)
        .merge(payment_routes)
        .nest("/auth", handlers::auth::routes())
        .nest("/contracts", handlers::contracts::routes())
        .nest("/files", handlers::files::routes())
        .nest("/docs", handlers::docs::routes())
        .layer(Extension(config))
        .with_state(database)
}

/// Document public API using [`aide`] crate.
fn api_docs(api: TransformOpenApi) -> TransformOpenApi {
    api.title("Patron")
        .description("API server public routes")
        .tag(Tag {
            name: "Authentication".into(),
            ..Default::default()
        })
        .tag(Tag {
            name: "Build session management".into(),
            ..Default::default()
        })
        .tag(Tag {
            name: "Contract management".into(),
            ..Default::default()
        })
        .tag(Tag {
            name: "File uploads".into(),
            ..Default::default()
        })
        .tag(Tag {
            name: "Public key verification".into(),
            ..Default::default()
        })
        .tag(Tag {
            name: "Membership and payments".into(),
            ..Default::default()
        })
        .tag(Tag {
            name: "Source code management".into(),
            ..Default::default()
        })
        .security_scheme(
            "Authentication token",
            SecurityScheme::Http {
                scheme: String::from("bearer"),
                bearer_format: None,
                description: None,
                extensions: Default::default(),
            },
        )
}
