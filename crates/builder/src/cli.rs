use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(about, version)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Start processing new build sessions.
    Serve,
}
