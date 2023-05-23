mod cli;

use std::error::Error;

use clap::Parser;
use cli::Cli;
use common::config::Config;
use migration::{cli::run_migrate, sea_orm::Database};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    let config = Config::new()?;

    let db = Database::connect(&config.database.url).await?;

    run_migrate(migration::Migrator, &db, cli.command, false).await?;

    Ok(())
}
