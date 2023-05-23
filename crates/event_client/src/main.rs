mod cli;
pub(crate) mod utils;

use clap::Parser;
use cli::{Cli, Command};
use common::config::Config;
use db::Database;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let config = Config::new()?;

    let database = Database::connect(&config.database.url).await?;

    match Cli::parse().command {
        Command::Initialize {
            name,
            url,
            schema,
            payment_address,
        } => cli::initialize(database, name, url, schema, payment_address).await?,
        Command::Traverse { name } => cli::traverse(database, name).await?,
        Command::UpdateContract {
            name,
            payment_address,
        } => cli::update_contract(database, name, payment_address).await?,
        Command::Watch { name } => cli::watch(database, name).await?,
    }

    Ok(())
}
