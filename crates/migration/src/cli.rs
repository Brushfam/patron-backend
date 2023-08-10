use std::path::PathBuf;

use clap::Parser;
use sea_orm_cli::MigrateSubcommands;

#[derive(Parser)]
pub(crate) struct Cli {
    #[clap(subcommand)]
    pub command: Option<MigrateSubcommands>,

    /// Path to configuration file.
    #[clap(short, long, value_parser)]
    pub config: Option<PathBuf>,
}
