mod cli;

use std::error::Error;

use clap::Parser;
use cli::Cli;
use common::config::Config;
use migration::{cli::run_migrate, sea_orm::Database};
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    let config = Config::new()?;

    info!("connecting to database");
    let db = Database::connect(&config.database.url).await?;
    info!("database connection established");

    run_migrate(migration::Migrator, &db, cli.command, false).await?;

    Ok(())
}
