mod cli;
mod commands;
mod log_collector;
mod process;

use clap::Parser;
use cli::{Cli, Command};
use common::{config::Config, logging};
use db::Database;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let config = Config::new()?;

    logging::init(&config);

    let Some(builder_config) = config.builder else {
        return Err(anyhow::Error::msg("unable to load builder config"));
    };

    info!("connecting to database");
    let database = Database::connect(&config.database.url).await?;

    match Cli::parse().command {
        Command::Serve => commands::serve(builder_config, config.storage, database).await?,
    }

    Ok(())
}
