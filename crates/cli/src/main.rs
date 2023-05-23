use clap::Parser;
use commands::{Cli, Commands};

mod archiver;
mod commands;
mod config;

fn main() -> Result<(), anyhow::Error> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Auth {
            server_path: server_domain,
            web_path: web_domain,
        } => commands::auth(server_domain, web_domain)?,
        Commands::Deploy {
            constructor,
            force_new_build_sessions,
            url,
            suri,
            args,
            cargo_contract_flags: var_arg,
        } => commands::deploy(
            force_new_build_sessions,
            constructor,
            url,
            suri,
            args,
            var_arg,
        )?,
    }

    Ok(())
}
