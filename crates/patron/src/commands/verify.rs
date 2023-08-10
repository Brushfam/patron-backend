use std::{
    fs::File,
    io::{self, Read},
    process::{Command, Stdio},
};

use common::hash::blake2;
use derive_more::{Display, Error, From};
use indicatif::ProgressBar;

use crate::{
    commands::Verify,
    config::{AuthenticationConfig, AuthenticationConfigError, ProjectConfig},
    process::{
        ensure_cargo_contract_exists, remote_build, CargoContractInstallError,
        FinishedBuildSession, RemoteBuildError,
    },
};

/// `verify` subcommand errors.
#[derive(Debug, Display, From, Error)]
pub(crate) enum VerifyError {
    /// Authentication configuration error.
    Authentication(AuthenticationConfigError),

    /// Unable to parse the project configuration with [`figment`].
    Figment(figment::Error),

    /// IO-related error.
    Io(io::Error),

    /// JSON parsing error.
    Json(serde_json::Error),

    /// Remote build process error.
    BuildProcessError(RemoteBuildError),

    /// [`which`] crate was unable to determine location of the `cargo` binary file.
    #[display(fmt = "unable to locate cargo: {}", _0)]
    Which(which::Error),

    /// Unable to install `cargo-contract`.
    CargoContractInstallError(CargoContractInstallError),

    /// Unable to get WASM path from cargo-contract output.
    #[display(fmt = "unable to get WASM path from cargo-contract output")]
    InvalidOutputJson,
}

/// Verify flow entrypoint.
pub(crate) fn verify(
    Verify {
        force_new_build_sessions,
    }: Verify,
) -> Result<(), VerifyError> {
    let auth_config = AuthenticationConfig::new()?;
    let project_config = ProjectConfig::new()?;

    let progress = ProgressBar::new_spinner();

    let cargo = which::which("cargo")?;

    ensure_cargo_contract_exists(&cargo, &project_config.cargo_contract_version, &progress)?;

    let FinishedBuildSession { code_hash, .. } = remote_build(
        &auth_config,
        &project_config,
        &progress,
        force_new_build_sessions,
    )?;

    println!("Remote code hash: 0x{code_hash}");

    progress.finish_with_message("Remote build finished. Proceeding with the local build...");

    let local_command = Command::new(&cargo)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .args(["contract", "build", "--verifiable", "--output-json"])
        .spawn()?
        .wait_with_output()?;

    let output: serde_json::Value = serde_json::from_slice(&local_command.stdout)?;

    let mut wasm_buf = Vec::new();

    File::open(
        output
            .get("dest_wasm")
            .ok_or(VerifyError::InvalidOutputJson)?
            .as_str()
            .ok_or(VerifyError::InvalidOutputJson)?,
    )?
    .read_to_end(&mut wasm_buf)?;

    let local_code_hash = hex::encode(blake2(&wasm_buf));

    println!("Local code hash: 0x{local_code_hash}");

    if local_code_hash == code_hash {
        println!("Code hashes are matching.");
    } else {
        println!("Code hashes do not match.");
    }

    Ok(())
}
