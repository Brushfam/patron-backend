/// `initialize` subcommand.
mod initialize;

/// `traverse` subcommand.
mod traverse;

/// `update_contract` subcommand.
mod update_contract;

/// `watch` subcommand.
mod watch;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

pub use initialize::initialize;
pub use traverse::traverse;
pub use update_contract::update_contract;
pub use watch::watch;

/// Primary CLI configuration, serves as an entrypoint to [`clap`].
#[derive(Parser)]
#[command(about, version)]
pub(crate) struct Cli {
    /// Selected subcommand.
    #[command(subcommand)]
    pub command: Command,

    /// Path to configuration file.
    #[clap(short, long, value_parser)]
    pub config: Option<PathBuf>,
}

/// Supported subcommands.
#[derive(Subcommand)]
pub(crate) enum Command {
    /// Initialize new node with the provided options.
    Initialize {
        /// Node name.
        name: String,

        /// Node WebSocket URL
        url: String,

        /// Address of a contract that accepts membership payments.
        #[clap(long)]
        payment_address: Option<String>,
    },

    /// Traverse old blocks of the provided node for old events.
    Traverse {
        /// Node name.
        name: String,
    },

    /// Update payment contract address.
    UpdateContract {
        /// Node name.
        name: String,

        /// Address of a contract that accepts membership payments.
        payment_address: Option<String>,
    },

    /// Watch node for new blocks to discover contract events.
    Watch {
        /// Node name.
        name: String,
    },
}
