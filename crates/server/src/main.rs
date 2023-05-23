mod auth;
mod handlers;
mod hex_hash;
mod pagination;
mod validation;

#[cfg(test)]
mod testing;

use std::sync::Arc;

use axum::{middleware::from_fn_with_state, Extension, Router, Server};
use common::{config::Config, logging};
use db::{Database, DatabaseConnection};
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let config = Config::new()?;

    logging::init(&config);

    let Some(server_config) = config.server.as_ref() else {
        return Err(anyhow::Error::msg("unable to load server config"));
    };

    info!("connecting to database");
    let database = Arc::new(Database::connect(&config.database.url).await?);
    let server = Server::bind(&server_config.address);
    let config = Arc::new(config);

    server
        .serve(app_router(database, config).into_make_service())
        .await?;

    Ok(())
}

fn app_router(database: Arc<DatabaseConnection>, config: Arc<Config>) -> Router {
    let mixed_routes = Router::new()
        .nest(
            "/sourceCode",
            handlers::source_code::routes(database.clone(), config.clone()),
        )
        .nest(
            "/buildSessions",
            handlers::build_sessions::routes(database.clone(), config.clone()),
        );

    let protected_routes = Router::new()
        .nest("/keys", handlers::keys::routes())
        .route_layer(from_fn_with_state(
            (database.clone(), config.clone()),
            auth::require_authentication::<false, false, _>,
        ));

    let payment_routes = Router::new()
        .nest("/payment", handlers::payment::routes())
        .route_layer(from_fn_with_state(
            (database.clone(), config.clone()),
            auth::require_authentication::<true, false, _>,
        ));

    Router::new()
        .merge(mixed_routes)
        .merge(protected_routes)
        .merge(payment_routes)
        .nest("/auth", handlers::auth::routes())
        .nest("/contracts", handlers::contracts::routes())
        .nest("/files", handlers::files::routes())
        .layer(Extension(config))
        .with_state(database)
}
