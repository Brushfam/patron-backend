use clap::{Parser, Subcommand};

/// CLI configuration, provided for the [`clap`] crate.
#[derive(Parser)]
#[command(about, version)]
pub(crate) struct Cli {
    /// Selected subcommand.
    #[command(subcommand)]
    pub command: Command,
}

/// Available subcommands.
#[derive(Subcommand)]
pub(crate) enum Command {
    /// Start processing new build sessions.
    Serve,
}
