use clap::Parser;
use commands::{Cli, Commands};

mod archiver;
mod commands;
mod config;

fn main() -> Result<(), anyhow::Error> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Auth(args) => commands::auth(args)?,
        Commands::Deploy(args) => commands::deploy(args)?,
    }

    Ok(())
}
