//! # Smart contract builder
//!
//! Smart contract builder process is responsible for managing
//! Docker containers that build the smart contract WASM blobs
//! in an isolated and reproducible manner.
//!
//! # CLI subcommands
//!
//! Currently, smart contract builder provides just one command - [`serve`],
//! which starts serving unhandled build sessions from the database.
//!
//! [`serve`]: commands::serve
//!
//! # Build process
//!
//! Since the build process is Docker-oriented, there are a few components
//! that are required to start build session containers - volume creation, container
//! instantiation and running container management.
//!
//! Volume creation is necessary to isolate disk space of separate builds into separate
//! files formatted as an ext4 filesystems. For more details, see the [`volume`] module.
//!
//! Container instantiation is done in the [`container`] module, while the container management
//! is present in the [`worker`] module.
//!
//! [`volume`]: process::volume
//! [`container`]: process::container
//! [`worker`]: process::worker
//!
//! # Log collector
//!
//! To provide users with information about whats happening during the build process
//! we spawn the log collector process, which ingests logs from all running build processes.
//!
//! See [`log_collector`] for more details.

#![deny(missing_docs)]
#![deny(clippy::missing_docs_in_private_items)]

/// CLI configuration and available subcommands.
mod cli;

/// Subcommand implementations.
mod commands;

/// Log collector implementation.
mod log_collector;

/// Build process instantiation and management.
mod process;

use clap::Parser;
use cli::{Cli, Command};
use common::{config::Config, logging};
use db::Database;
use tracing::info;

/// Smart contract builder entrypoint.
#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let cli = Cli::parse();

    let config = Config::new(cli.config)?;

    logging::init(&config);

    let Some(builder_config) = config.builder else {
        return Err(anyhow::Error::msg("unable to load builder config"));
    };

    info!("connecting to database");
    let database = Database::connect(&config.database.url).await?;
    info!("database connection established");

    match cli.command {
        Command::Serve => {
            commands::serve(
                builder_config,
                config.storage,
                config.supported_cargo_contract_versions,
                database,
            )
            .await?
        }
    }

    Ok(())
}
