use std::sync::Arc;

use bollard::{errors::Error, Docker};
use common::config;
use db::{DatabaseConnection, DbErr};
use derive_more::{Display, Error, From};
use futures_util::{stream::FuturesUnordered, FutureExt, StreamExt};
use tokio::sync::mpsc;
use tracing::{info, instrument};

use crate::{log_collector, process::worker};

/// `serve` command errors.
#[derive(Display, Debug, From, Error)]
pub enum ServeError {
    /// Database-related error.
    DbErr(DbErr),
}

/// Spawn build session workers to handle new build sessions.
#[instrument(skip_all, err)]
pub async fn serve(
    builder_config: config::Builder,
    storage_config: config::Storage,
    supported_cargo_contract_versions: Vec<String>,
    database: DatabaseConnection,
) -> Result<(), Error> {
    let builder_config = Arc::new(builder_config);
    let storage_config = Arc::new(storage_config);
    let supported_cargo_contract_versions = Arc::new(supported_cargo_contract_versions);
    let docker = Arc::new(Docker::connect_with_socket_defaults()?);
    let database = Arc::new(database);

    info!("spawning log collector");
    let (sender, receiver) = mpsc::unbounded_channel();
    tokio::spawn(log_collector::collect_logs(database.clone(), receiver));

    info!("started build session processing");

    (0..builder_config.worker_count)
        .map(|_| {
            tokio::spawn(worker::spawn(
                builder_config.clone(),
                storage_config.clone(),
                supported_cargo_contract_versions.clone(),
                docker.clone(),
                database.clone(),
                sender.clone(),
            ))
            .map(|_| ())
        })
        .collect::<FuturesUnordered<_>>()
        .collect::<()>()
        .await;

    Ok(())
}
