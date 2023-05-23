mod initialize;
mod traverse;
mod update_contract;
mod watch;

use clap::{Parser, Subcommand};

pub use initialize::initialize;
pub use traverse::traverse;
pub use update_contract::update_contract;
pub use watch::watch;

#[derive(Parser)]
#[command(about, version)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Initialize new node with the provided options.
    Initialize {
        /// Node name.
        name: String,

        /// Node WebSocket URL
        url: String,

        /// Schema name, that identifies the node's ABI.
        schema: String,

        /// Address of a contract that accepts membership payments.
        #[clap(long)]
        payment_address: Option<String>,
    },

    /// Traverse old blocks of the provided node for old events.
    Traverse { name: String },

    /// Update payment contract address.
    UpdateContract {
        /// Node name.
        name: String,

        /// Address of a contract that accepts membership payments.
        payment_address: Option<String>,
    },

    /// Watch node for new blocks to discover contract events.
    Watch { name: String },
}
