//! # Archiver
//!
//! To package smart contract's source code we use ZIP file format.
//!
//! To archive the project itself, we recursively iterate over contents
//! of the directory where user launched the deployment flow. While collecting available
//! paths, we need to ignore directories which are most likely to be unused during builds,
//! such as the `target` directory and hidden entries (for example, `.git`).

#![deny(missing_docs)]
#![deny(clippy::missing_docs_in_private_items)]

use clap::Parser;
use commands::{Cli, Commands};

/// Contract source code archiving utilities.
mod archiver;

/// CLI subcommands.
mod commands;

/// CLI-specific configuration (authentication, project).
mod config;

/// Remote build process implementation.
mod process;

/// CLI entrypoint.
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), anyhow::Error> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Auth(args) => commands::auth(args).await?,
        Commands::Deploy(args) => commands::deploy(args).await?,
        Commands::Build(args) => commands::build(args).await?,
        Commands::Verify(args) => commands::verify(args).await?,
        Commands::Watch(args) => commands::watch(args).await?,
    }

    Ok(())
}
