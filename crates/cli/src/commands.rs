mod auth;
mod deploy;

pub(crate) use auth::auth;
pub(crate) use deploy::deploy;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(about)]
pub(crate) struct Cli {
    /// Configuration file path.
    #[arg(short, long, default_value = "Deploy.toml")]
    pub config_file: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub(crate) enum Commands {
    /// Authenticate using the browser flow.
    Auth {
        /// Custom server path.
        #[arg(short, long)]
        server_path: Option<String>,

        /// Custom web path.
        #[arg(short, long)]
        web_path: Option<String>,
    },

    /// Start the deployment process.
    #[clap(trailing_var_arg = true)]
    Deploy {
        /// Contract constructor name.
        constructor: String,

        /// Always start new build sessions, even if the source code was verified previously.
        #[arg(short, long)]
        force_new_build_sessions: bool,

        /// WebSocket URL of an RPC node.
        #[arg(short, long)]
        url: Option<String>,

        /// Secret URI for signing requests.
        #[arg(short, long)]
        suri: Option<String>,

        /// Space-separated values passed to constructor.
        #[arg(short, long)]
        args: Option<String>,

        /// Additional options passed to cargo-contract.
        #[clap(allow_hyphen_values = true)]
        cargo_contract_flags: Vec<String>,
    },
}
