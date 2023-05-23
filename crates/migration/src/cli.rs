use clap::Parser;
use sea_orm_cli::MigrateSubcommands;

#[derive(Parser)]
pub(crate) struct Cli {
    #[clap(subcommand)]
    pub command: Option<MigrateSubcommands>,
}
