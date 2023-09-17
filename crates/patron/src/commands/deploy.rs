use std::{io, process::Stdio};

use derive_more::{Display, Error, From};
use indicatif::ProgressBar;
use rand::{thread_rng, Rng};
use tokio::process::Command;

use crate::{
    commands::Deploy,
    config::{AuthenticationConfig, AuthenticationConfigError, ProjectConfig},
    process::{
        ensure_cargo_contract_exists, instantiate_contract, remote_build,
        CargoContractInstallError, FinishedBuildSession, Instantiation, InstantiationError,
        RemoteBuildError,
    },
};

/// `deploy` subcommand errors.
#[derive(Debug, Display, From, Error)]
pub(crate) enum DeployError {
    /// Authentication configuration error.
    Authentication(AuthenticationConfigError),

    /// Unable to parse the project configuration with [`figment`].
    Figment(figment::Error),

    /// IO-related error.
    Io(io::Error),

    /// [`which`] crate was unable to determine location of the `cargo` binary file.
    #[display(fmt = "unable to locate cargo: {}", _0)]
    Which(which::Error),

    /// Unable to install `cargo-contract`.
    CargoContractInstallError(CargoContractInstallError),

    /// Remote build process error.
    RemoteBuildError(RemoteBuildError),

    /// Contract could not be instantiated from the downloaded WASM blob.
    #[display(fmt = "unable to instantiate a contract")]
    InstantiationError(InstantiationError),
}

/// Deployment flow entrypoint.
pub(crate) async fn deploy(
    Deploy {
        constructor,
        force_new_build_sessions,
        root,
        url,
        suri,
        args,
        gas,
        proof_size,
        salt,
        cargo_contract_flags,
    }: Deploy,
) -> Result<(), DeployError> {
    let auth_config = AuthenticationConfig::new()?;
    let project_config = ProjectConfig::new()?;

    let progress = ProgressBar::new_spinner();

    let cargo = which::which("cargo")?;

    ensure_cargo_contract_exists(&cargo, &project_config.cargo_contract_version, &progress).await?;

    let FinishedBuildSession {
        wasm_file,
        metadata_file,
        code_hash,
    } = remote_build(
        &auth_config,
        &project_config,
        &progress,
        force_new_build_sessions,
        root.as_deref(),
    )
    .await?;

    progress.set_message("Deploying...");

    let mut upload_command = Command::new(&cargo);

    upload_command
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .args([
            "contract",
            "upload",
            "--execute",
            "--skip-confirm",
            "--skip-dry-run",
        ])
        .arg(wasm_file.path())
        .args(&cargo_contract_flags);

    if let Some(url) = url.as_deref() {
        upload_command.args(["--url", url]);
    }

    if let Some(suri) = suri.as_deref() {
        upload_command.args(["--suri", suri]);
    }

    upload_command.spawn()?.wait().await?;

    // Don't check for upload errors, since we might already have
    // the same code hash uploaded. Proceed with instantiation instead.

    let instantiation_config = Instantiation {
        constructor: &constructor,
        args: args.as_deref(),
        suri: suri.as_deref(),
        url: url.as_deref(),
        gas,
        proof_size,
    };

    instantiate_contract(
        &cargo,
        &instantiation_config,
        &cargo_contract_flags,
        Some(metadata_file.path()),
        salt.unwrap_or_else(|| thread_rng().gen()),
    )
    .await?;

    progress.finish_with_message(format!(
        "Contract uploaded: {}/codeHash/{}",
        auth_config.web_path(),
        code_hash
    ));

    Ok(())
}
