//! # Event client
//!
//! Event client is responsible for the background interaction with different RPC nodes
//! attached to corresponding networks.
//!
//! The communication is done in order to keep the database with recent deployment events
//! and provide users with information about existing smart contracts and uploaded WASM blobs.
//!
//! ## Node initialization
//!
//! Use the `initialize` subcommand to initialize a new node and add information
//! about its deployed smart contracts and uploaded WASM blobs to the database.
//!
//! Refer to the [`initialize`] documentation for more details.
//!
//! ## Node watcher
//!
//! `watch` subcommand can be used to watch for new events from an RPC node.
//! These events contain information about new smart contract deployments and code uploads.
//!
//! Refer to the [`watch`] documentation for more details.
//!
//! ## Node traversal
//!
//! `traverse` subcommand attempts to traverse previous blocks to collect info about
//! previous smart contract events. Be aware, that this command is meant for **testing only**,
//! as detailed info about previous blocks is usually available to fully-featured indexing
//! servers.
//!
//! Refer to the [`traverse`] documentation for more details.
//!
//! ## Payment contract update
//!
//! Using `update-contract` subcommand you can update the address of the payment
//! contract for the specified node.
//!
//! Refer to the [`update_contract`] documentation for more details.
//!
//! [`initialize`]: cli::initialize
//! [`watch`]: cli::watch
//! [`traverse`]: cli::traverse
//! [`update_contract`]: cli::update_contract

#![deny(missing_docs)]
#![deny(clippy::missing_docs_in_private_items)]

/// CLI general configuration and subcommands.
mod cli;

/// Various extraction and mapping utilities.
pub(crate) mod utils;

use clap::Parser;
use cli::{Cli, Command};
use common::{config::Config, logging};
use db::Database;
use tracing::info;

/// Event client entrypoint.
#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let cli = Cli::parse();

    let config = Config::new(cli.config)?;

    logging::init(&config);

    info!("connecting to database");
    let database = Database::connect(&config.database.url).await?;
    info!("database connection established");

    match cli.command {
        Command::Initialize {
            name,
            url,
            payment_address,
        } => cli::initialize(database, name, url, payment_address).await?,
        Command::Traverse { name } => cli::traverse(database, name).await?,
        Command::UpdateContract {
            name,
            payment_address,
        } => cli::update_contract(database, name, payment_address).await?,
        Command::Watch { name } => cli::watch(database, name).await?,
    }

    Ok(())
}
