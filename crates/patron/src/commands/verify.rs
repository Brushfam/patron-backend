use std::{
    fs::File,
    io::{self, Read},
};

use common::hash::blake2;
use derive_more::{Display, Error, From};
use indicatif::ProgressBar;

use crate::{
    commands::Verify,
    config::{AuthenticationConfig, AuthenticationConfigError, ProjectConfig},
    process::{
        build_locally, ensure_cargo_contract_exists, ensure_docker_exists, remote_build,
        BuildError, CargoContractInstallError, FinishedBuildSession, RemoteBuildError,
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

    /// Local build process error.
    LocalBuildProcessError(BuildError),

    /// Remote build process error.
    RemoteBuildProcessError(RemoteBuildError),

    /// [`which`] crate was unable to determine location of the `cargo` binary file.
    #[display(fmt = "unable to locate cargo: {}", _0)]
    Which(which::Error),

    /// Docker installation was not found.
    #[display(fmt = "unable to find docker installation")]
    DockerInstallationMissing,

    /// Unable to install `cargo-contract`.
    CargoContractInstallError(CargoContractInstallError),
}

/// Verify flow entrypoint.
pub(crate) async fn verify(
    Verify {
        force_new_build_sessions,
        root,
    }: Verify,
) -> Result<(), VerifyError> {
    let auth_config = AuthenticationConfig::new()?;
    let project_config = ProjectConfig::new()?;

    let progress = ProgressBar::new_spinner();

    let cargo = which::which("cargo")?;

    ensure_cargo_contract_exists(&cargo, &project_config.cargo_contract_version, &progress).await?;

    if ensure_docker_exists().await {
        return Err(VerifyError::DockerInstallationMissing);
    }

    let FinishedBuildSession { code_hash, .. } = remote_build(
        &auth_config,
        &project_config,
        &progress,
        force_new_build_sessions,
        root.as_deref(),
    )
    .await?;

    println!("Remote code hash: 0x{code_hash}");

    progress.finish_with_message("Remote build finished. Proceeding with the local build...");

    let build_result = build_locally(&cargo, true).await?;

    let mut wasm_buf = Vec::new();

    File::open(build_result.dest_wasm)?.read_to_end(&mut wasm_buf)?;

    let local_code_hash = hex::encode(blake2(&wasm_buf));

    println!("Local code hash: 0x{local_code_hash}");

    if local_code_hash == code_hash {
        println!("Code hashes are matching.");
    } else {
        println!("Code hashes do not match.");
    }

    Ok(())
}
