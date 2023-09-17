/// `auth` subcommand.
mod auth;

/// `build` subcommand.
mod build;

/// `deploy` subcommand.
mod deploy;

/// `verify` subcommand.
mod verify;

/// 'watch' subcommand.
mod watch;

pub(crate) use auth::auth;
pub(crate) use build::build;
pub(crate) use deploy::deploy;
pub(crate) use verify::verify;
pub(crate) use watch::watch;

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

/// CLI configuration.
#[derive(Parser)]
#[command(about)]
pub(crate) struct Cli {
    /// Configuration file path.
    #[arg(short, long, default_value = "Deploy.toml")]
    pub config_file: Option<PathBuf>,

    /// Selected subcommand.
    #[command(subcommand)]
    pub command: Commands,
}

/// Supported subcommands.
#[derive(Subcommand)]
pub(crate) enum Commands {
    /// Authenticate using the browser flow.
    Auth(Auth),

    /// Start the build and deployment process.
    Deploy(Deploy),

    /// Build the contract remotely without the initial deployment.
    Build(Build),

    /// Verify remotely built contract with locally built one.
    Verify(Verify),

    /// Watch for changes and rebuild the contract.
    Watch(Watch),
}

/// `auth` subcommand configuration.
#[derive(Args)]
pub struct Auth {
    /// Custom server path.
    #[arg(short, long)]
    server_path: Option<String>,

    /// Custom web path.
    #[arg(short, long)]
    web_path: Option<String>,
}

/// `deploy` subcommand configuration.
#[derive(Args)]
#[clap(trailing_var_arg = true)]
pub struct Deploy {
    /// Contract constructor name.
    constructor: String,

    /// Always start new build sessions, even if the source code was verified previously.
    #[arg(short, long)]
    force_new_build_sessions: bool,

    /// Relative project root used to build multi-contract projects.
    #[arg(short, long)]
    root: Option<PathBuf>,

    /// WebSocket URL of an RPC node.
    #[arg(short, long)]
    url: Option<String>,

    /// Secret URI for signing requests.
    #[arg(short, long)]
    suri: Option<String>,

    /// Space-separated values passed to constructor.
    #[arg(short, long)]
    args: Option<String>,

    /// Gas value used to instantiate the contract.
    #[arg(short, long)]
    gas: Option<u64>,

    /// Maximum proof size for contract instantiation.
    #[arg(short, long)]
    proof_size: Option<u64>,

    /// Salt value used to create multiple instances of the same contract.
    #[arg(long)]
    salt: Option<u64>,

    /// Additional options passed to cargo-contract.
    #[clap(allow_hyphen_values = true)]
    cargo_contract_flags: Vec<String>,
}

/// `build` subcommand configuration.
#[derive(Args)]
pub struct Build {
    /// Always start new build sessions, even if the source code was verified previously.
    #[arg(short, long)]
    force_new_build_sessions: bool,

    /// Relative project root used to build multi-contract projects.
    #[arg(short, long)]
    root: Option<PathBuf>,

    /// Path where to output a newly built contract WASM blob.
    #[arg(short, long)]
    wasm_path: Option<PathBuf>,

    /// Path where to output a newly built contract JSON metadata.
    #[arg(short, long)]
    metadata_path: Option<PathBuf>,

    /// Path where to output a bundled JSON, which contains both WASM and metadata.
    #[arg(short, long)]
    bundle_path: Option<PathBuf>,
}

/// `verify` subcommand configuration.
#[derive(Args)]
pub struct Verify {
    /// Always start new build sessions, even if the source code was verified previously.
    #[arg(short, long)]
    force_new_build_sessions: bool,

    /// Relative project root used to build multi-contract projects.
    #[arg(short, long)]
    root: Option<PathBuf>,
}

/// `watch` subcommand configuration.
#[derive(Args)]
pub struct Watch {
    /// Custom web path.
    #[arg(short, long)]
    web_path: Option<String>,

    /// Contract constructor name.
    constructor: String,

    /// Space-separated values passed to constructor.
    #[arg(short, long)]
    args: Option<String>,

    /// Secret URI for signing requests.
    #[arg(short, long)]
    suri: Option<String>,

    /// WebSocket URL of an RPC node.
    #[arg(short, long)]
    url: Option<String>,

    /// Gas value used to instantiate the contract.
    #[arg(short, long)]
    gas: Option<u64>,

    /// Maximum proof size for contract instantiation.
    #[arg(short, long)]
    proof_size: Option<u64>,

    /// Additional options passed to cargo-contract.
    #[clap(allow_hyphen_values = true)]
    cargo_contract_flags: Vec<String>,
}
