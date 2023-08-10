use std::{
    io,
    process::{Command, Stdio},
};

use derive_more::{Display, Error, From};
use indicatif::ProgressBar;

use crate::{
    commands::Deploy,
    config::{AuthenticationConfig, AuthenticationConfigError, ProjectConfig},
    process::{
        ensure_cargo_contract_exists, remote_build, CargoContractInstallError,
        FinishedBuildSession, RemoteBuildError,
    },
};

/// Default value passed to weight configuration flags of the `cargo-contract`.
const DEFAULT_WEIGHT_VAL: u64 = 10_000_000_000;

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

    /// Contract could not be instantiated from the downloaded WASM blob.
    #[display(fmt = "unable to instantiate a contract")]
    InstantiationError,

    /// Remote build process error.
    RemoteBuildError(RemoteBuildError),
}

/// Deployment flow entrypoint.
pub(crate) fn deploy(
    Deploy {
        constructor,
        force_new_build_sessions,
        url,
        suri,
        args,
        gas,
        proof_size,
        cargo_contract_flags,
    }: Deploy,
) -> Result<(), DeployError> {
    let auth_config = AuthenticationConfig::new()?;
    let project_config = ProjectConfig::new()?;

    let progress = ProgressBar::new_spinner();

    let cargo = which::which("cargo")?;

    ensure_cargo_contract_exists(&cargo, &project_config.cargo_contract_version, &progress)?;

    let FinishedBuildSession {
        wasm_file,
        metadata_file,
        code_hash,
    } = remote_build(
        &auth_config,
        &project_config,
        &progress,
        force_new_build_sessions,
    )?;

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

    upload_command.spawn()?.wait()?;

    // Don't check for upload errors, since we might already have
    // the same code hash uploaded. Proceed with instantiation instead.
    let mut instantiate_command = Command::new(cargo);

    instantiate_command
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .args([
            "contract",
            "instantiate",
            "--execute",
            "--skip-confirm",
            "--skip-dry-run",
            "--gas",
            &gas.unwrap_or(DEFAULT_WEIGHT_VAL).to_string(),
            "--proof-size",
            &proof_size.unwrap_or(DEFAULT_WEIGHT_VAL).to_string(),
        ])
        .arg(metadata_file.path())
        .args(["--constructor", &constructor])
        .args(cargo_contract_flags);

    if let Some(url) = url.as_deref() {
        instantiate_command.args(["--url", url]);
    }

    if let Some(suri) = suri.as_deref() {
        instantiate_command.args(["--suri", suri]);
    }

    if let Some(args) = args {
        instantiate_command.args(["--args", &args]);
    }

    if !instantiate_command.spawn()?.wait()?.success() {
        return Err(DeployError::InstantiationError);
    }

    progress.finish_with_message(format!(
        "Contract uploaded: {}/codeHash/{}",
        auth_config.web_path(),
        code_hash
    ));

    Ok(())
}
